//! OpenSSL part of the streams and the listeners: the context they keep across connections, and
//! the code that connects and handshakes with it.
use std::{
    fmt, io,
    pin::Pin,
    sync::{
        Arc, PoisonError, RwLock,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use openssl::ssl::{SslAcceptor, SslConnector};
use prosa_utils::config::ssl::{SslConfig, SslContextBuild};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    net::TcpStream,
    time::timeout,
};
use url::Url;

use super::{
    get_safe_url,
    listener::{SslHandshaker, StreamListener},
    stream::{Stream, TargetSetting},
};

/// SSL context kept across connections, with the generation of the [`SslConfig`] files it was
/// built from
pub(crate) struct SslContextCache<C> {
    context: RwLock<Option<(C, u64)>>,
    rebuilding: AtomicBool,
}

impl<C> Default for SslContextCache<C> {
    fn default() -> Self {
        SslContextCache {
            context: RwLock::new(None),
            rebuilding: AtomicBool::new(false),
        }
    }
}

impl<C: SslContextBuild> SslContextCache<C> {
    /// Cache holding a context built elsewhere, never built again
    pub(crate) fn with_context(context: C) -> Self {
        SslContextCache {
            context: RwLock::new(Some((context, 0))),
            rebuilding: AtomicBool::new(false),
        }
    }

    pub(crate) fn current(&self) -> Option<(C, u64)> {
        self.context
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    pub(crate) fn store(&self, context: C, generation: u64) {
        *self.context.write().unwrap_or_else(PoisonError::into_inner) = Some((context, generation));
    }

    /// Context to connect with now.
    ///
    /// The first one is built and waited for. Then the current one is returned straight away, and
    /// one built again in the background once the files of `config` changed, for the next
    /// connections.
    pub(crate) async fn get(
        self: &Arc<Self>,
        config: &SslConfig,
        host: Option<&str>,
        name: &(dyn fmt::Display + Sync),
    ) -> std::io::Result<C> {
        if let Some((context, built)) = self.current() {
            if built != config.generation() && !self.rebuilding.swap(true, Ordering::AcqRel) {
                let (cache, config) = (self.clone(), config.clone());
                let (host, name) = (host.map(String::from), name.to_string());
                tokio::task::spawn_blocking(move || cache.rebuild(&config, host.as_deref(), &name));
            }

            return Ok(context);
        }

        let (config, host) = (config.clone(), host.map(String::from));
        let (context, generation) = tokio::task::spawn_blocking(move || {
            // Read before the files, so a change made while reading them is a new generation.
            // Watching them starts here, out of the runtime
            let generation = config.generation();
            C::build(&config, host.as_deref()).map(|context| (context, generation))
        })
        .await
        .map_err(std::io::Error::other)??;
        self.store(context.clone(), generation);
        Ok(context)
    }

    /// Build the context again. Files that can't be read, caught half written, keep the previous
    /// context until they change again
    pub(crate) fn rebuild(&self, config: &SslConfig, host: Option<&str>, name: &str) {
        let generation = config.generation();
        match C::build(config, host) {
            Ok(context) => self.store(context, generation),
            Err(err) => {
                log::warn!("Can't read the certificates of {name}, keep the previous ones: {err}");
                if let Some((_, built)) = self
                    .context
                    .write()
                    .unwrap_or_else(PoisonError::into_inner)
                    .as_mut()
                {
                    *built = generation;
                }
            }
        }

        self.rebuilding.store(false, Ordering::Release);
    }
}

impl Stream {
    /// Method to create an SSL stream from a TCP stream
    async fn create_openssl<S>(
        tcp_stream: S,
        ssl_connector: &SslConnector,
        domain: &str,
    ) -> Result<tokio_openssl::SslStream<S>, io::Error>
    where
        S: AsyncRead + AsyncWrite + Unpin,
    {
        let ssl = ssl_connector.configure()?.into_ssl(domain)?;
        let mut stream = tokio_openssl::SslStream::new(ssl, tcp_stream)?;
        if let Err(e) = Pin::new(&mut stream).connect().await {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                format!("Can't connect the OpenSSL socket `{e}`"),
            ));
        }

        Ok(stream)
    }

    /// Connect an OpenSSL socket to a distant
    ///
    #[doc = simple_mermaid::mermaid!("diagrams/stream_openssl.mmd")]
    ///
    /// ```
    /// use tokio::io;
    /// use url::Url;
    /// use prosa::io::{
    ///     SslConfig,
    ///     SslConfigContext,
    ///     stream::Stream,
    /// };
    ///
    /// async fn connecting() -> Result<(), io::Error> {
    ///     let ssl_config = SslConfig::default();
    ///     if let Ok(ssl_context_builder) = ssl_config.init_tls_client_context() {
    ///         let ssl_context = ssl_context_builder.build();
    ///         let stream: Stream = Stream::connect_openssl(&Url::parse("worldline.com:443").unwrap(), &ssl_context).await?;
    ///
    ///         // Handle the stream like any tokio stream
    ///     }
    ///
    ///     Ok(())
    /// }
    /// ```
    pub async fn connect_openssl(
        url: &Url,
        ssl_context: &SslConnector,
    ) -> Result<Stream, io::Error> {
        let addrs = super::lookup_url(url).await?;
        Ok(Stream::OpenSsl(
            Self::create_openssl(
                TcpStream::connect(&*addrs).await?,
                ssl_context,
                url.host_str().ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidInput,
                        format!("Can't retrieve host from url `{url}` for ssl"),
                    )
                })?,
            )
            .await?,
        ))
    }

    /// Connect an OpenSSL socket to a distant through an HTTP proxy
    ///
    #[doc = simple_mermaid::mermaid!("diagrams/stream_openssl_proxy.mmd")]
    ///
    /// ```
    /// use tokio::io;
    /// use url::Url;
    /// use prosa::io::{
    ///     SslConfig,
    ///     SslConfigContext,
    ///     stream::Stream,
    /// };
    ///
    /// async fn connecting() -> Result<(), io::Error> {
    ///     let proxy_url = Url::parse("http://user:pwd@proxy:3128").unwrap();
    ///     let ssl_config = SslConfig::default();
    ///     if let Ok(ssl_context_builder) = ssl_config.init_tls_client_context() {
    ///         let ssl_context = ssl_context_builder.build();
    ///         let stream: Stream = Stream::connect_openssl_with_http_proxy("worldline.com", 443, &ssl_context, &proxy_url).await?;
    ///
    ///         // Handle the stream like any tokio stream
    ///     }
    ///
    ///     Ok(())
    /// }
    /// ```
    #[cfg(feature = "http-proxy")]
    pub async fn connect_openssl_with_http_proxy(
        host: &str,
        port: u16,
        ssl_connector: &SslConnector,
        proxy: &Url,
    ) -> Result<Stream, io::Error> {
        Ok(Stream::OpenSslHttpProxy(
            Self::create_openssl(
                Self::connect_http_proxy(host, port, proxy).await?,
                ssl_connector,
                host,
            )
            .await?,
        ))
    }
}

impl TargetSetting {
    /// SSL connector to connect with now, [`None`] for a target that isn't reached over SSL.
    /// The deadline of the connection includes building the first one.
    pub(super) async fn ssl_connector(&self) -> Result<Option<SslConnector>, io::Error> {
        if !self.is_ssl() {
            return Ok(None);
        }

        let default_config;
        let ssl_config = match self.ssl() {
            Some(ssl_config) => ssl_config,
            None => {
                default_config = SslConfig::default();
                &default_config
            }
        };
        timeout(
            Duration::from_millis(self.connect_timeout),
            self.ssl_context.get(ssl_config, None, self),
        )
        .await
        .map_err(|e| {
            io::Error::new(
                io::ErrorKind::TimedOut,
                format!("SSL context timeout after {e} for {}", self.get_safe_url()),
            )
        })?
        .map(Some)
    }

    /// Connect to the target over SSL with the connector
    pub(super) async fn connect_ssl(&self, ssl_connector: &SslConnector) -> io::Result<Stream> {
        timeout(
            Duration::from_millis(self.connect_timeout),
            Stream::connect_openssl(&self.url, ssl_connector),
        )
        .await
        .map_err(|e| {
            io::Error::new(
                io::ErrorKind::TimedOut,
                format!("openssl timeout after {e} for {}", self.get_safe_url()),
            )
        })?
    }

    /// Connect to the target over SSL through an HTTP proxy with the connector
    #[cfg(feature = "http-proxy")]
    pub(super) async fn connect_ssl_with_http_proxy(
        &self,
        ssl_connector: &SslConnector,
        proxy_url: &Url,
    ) -> io::Result<Stream> {
        timeout(
            Duration::from_millis(self.connect_timeout),
            Stream::connect_openssl_with_http_proxy(
                self.url.host_str().unwrap_or_default(),
                self.url.port_or_known_default().unwrap_or_default(),
                ssl_connector,
                proxy_url,
            ),
        )
        .await
        .map_err(|e| {
            io::Error::new(
                io::ErrorKind::TimedOut,
                format!(
                    "openssl with proxy timeout after {e} for {} -proxy {}",
                    self.get_safe_url(),
                    get_safe_url(proxy_url)
                ),
            )
        })?
    }
}

impl SslHandshaker {
    /// Method to create the SSL parameters served by a listener.
    /// By default, the SSL handshake timeout is 3 seconds
    ///
    /// They read no file, so they serve that acceptor for as long as they live: their certificate
    /// source decides when to replace them.
    pub fn new(acceptor: SslAcceptor, timeout: Option<Duration>) -> SslHandshaker {
        SslHandshaker {
            acceptor: Arc::new(SslContextCache::with_context(acceptor)),
            source: None,
            timeout: timeout.unwrap_or(StreamListener::DEFAULT_SSL_TIMEOUT),
        }
    }

    /// Build the parameters from a configuration
    pub(super) async fn build(
        ssl_config: SslConfig,
        host: Option<String>,
    ) -> Result<SslHandshaker, io::Error> {
        let handshaker = SslHandshaker {
            acceptor: Arc::default(),
            timeout: ssl_config.get_ssl_timeout(),
            source: Some(Arc::new((ssl_config, host))),
        };
        handshaker.acceptor().await?;
        Ok(handshaker)
    }

    /// Acceptor to serve now
    pub(super) async fn acceptor(&self) -> Result<SslAcceptor, io::Error> {
        match &self.source {
            Some(source) => {
                let (ssl_config, host) = source.as_ref();
                self.acceptor
                    .get(ssl_config, host.as_deref(), &"the listener")
                    .await
            }
            None => self
                .acceptor
                .current()
                .map(|(acceptor, _)| acceptor)
                .ok_or_else(|| io::Error::other("No SSL acceptor to serve")),
        }
    }

    /// Text of the certificate served now, for debugging
    pub(super) fn certificate_text(&self) -> Option<String> {
        self.acceptor.current().and_then(|(acceptor, _)| {
            let text = acceptor.context().certificate()?.to_text().ok()?;
            Some(String::from_utf8_lossy(&text).into_owned())
        })
    }

    /// Negotiate SSL with a client that has just been accepted
    pub(super) async fn handshake_openssl(&self, stream: Stream) -> Result<Stream, io::Error> {
        let Stream::Tcp(tcp_stream) = stream else {
            return Ok(stream);
        };

        let acceptor = self.acceptor().await?;
        let ssl = openssl::ssl::Ssl::new(acceptor.context())
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
        let mut stream = tokio_openssl::SslStream::new(ssl, tcp_stream)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
        if let Err(e) = timeout(self.timeout, Pin::new(&mut stream).accept())
            .await
            .map_err(|_| {
                io::Error::new(
                    io::ErrorKind::TimedOut,
                    format!(
                        "SSL timeout[{} ms] for {stream:?}",
                        self.timeout.as_millis()
                    ),
                )
            })?
        {
            return Err(io::Error::other(format!("Can't accept the client: {e}")));
        }

        Ok(Stream::OpenSsl(stream))
    }
}

impl StreamListener {
    /// Set an OpenSSL acceptor to accept SSL connections from clients
    /// By default, the SSL connect timeout is 3 seconds
    ///
    #[doc = simple_mermaid::mermaid!("diagrams/listener_tls.mmd")]
    ///
    /// ```
    /// use tokio::io;
    /// use prosa::io::{
    ///     listener::StreamListener,
    ///     SslConfig,
    ///     SslConfigContext,
    /// };
    ///
    /// async fn accepting() -> Result<(), io::Error> {
    ///     let ssl_acceptor = SslConfig::default().init_tls_server_context(None).unwrap().build();
    ///     let stream_listener: StreamListener = StreamListener::bind("0.0.0.0:10000").await?.ssl_acceptor(ssl_acceptor, None);
    ///
    ///     loop {
    ///         // The client SSL handshake will happen here
    ///         let (stream, addr) = stream_listener.accept().await?;
    ///
    ///         // Handle the stream like any tokio stream
    ///     }
    ///
    ///     Ok(())
    /// }
    /// ```
    pub fn ssl_acceptor(
        self,
        ssl_acceptor: SslAcceptor,
        ssl_timeout: Option<Duration>,
    ) -> StreamListener {
        self.set_handshaker(Some(SslHandshaker::new(ssl_acceptor, ssl_timeout)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::listener::{
        ListenerSetting,
        tests::{peer_certificate, served_certificate, unique_test_path, write_certificate},
    };
    use prosa_utils::config::ssl::SslConfigContext as _;

    #[tokio::test]
    async fn listener_serves_the_certificate_renewed_on_disk() -> io::Result<()> {
        use prosa_utils::config::ssl::Store;

        let dir = unique_test_path("listener_renewed");
        std::fs::create_dir_all(&dir)?;
        let cert_path = dir.join("cert.pem");
        let key_path = dir.join("cert.key");
        write_certificate(&cert_path, &key_path);

        let setting = ListenerSetting::new(
            Url::parse("https://localhost:0").expect("Listener url is valid"),
            Some(SslConfig::new_cert_key(
                cert_path.to_string_lossy().into_owned(),
                key_path.to_string_lossy().into_owned(),
                None,
            )),
        );
        let listener = setting.bind().await?;
        let url = Url::parse(&format!(
            "tls://localhost:{}",
            listener.local_addr()?.port()
        ))
        .expect("Target url is valid");

        // The client trusts whichever certificate is on disk when it connects
        let accept = async || {
            let mut client_config = SslConfig::default();
            client_config.set_store(Store::File {
                path: cert_path.to_string_lossy().into_owned(),
            });
            let connector: ::openssl::ssl::SslConnectorBuilder =
                client_config.init_tls_client_context()?;
            let connector = connector.build();
            let (served, connected) = futures_util::future::join(
                listener.accept(),
                Stream::connect_openssl(&url, &connector),
            )
            .await;
            served?;
            Ok::<_, io::Error>(peer_certificate(&connected?))
        };
        let served = || served_certificate(&listener);

        // Accepts clients until the listener serves another certificate than `previous`, at most
        // `rounds` of them. A client accepted meanwhile is served the current one, so it may not
        // trust it
        let accept_until_renewed = async |previous: &[u8], rounds: usize| {
            for _ in 0..rounds {
                let _ = accept().await;
                if served() != previous {
                    return true;
                }
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
            false
        };

        let certificate = accept().await?;
        assert_eq!(served(), certificate);

        // Served straight away from the current context, and built again in the background for
        // the next clients
        write_certificate(&cert_path, &key_path);
        assert!(accept_until_renewed(&certificate, 100).await);
        let renewed = served();
        assert_eq!(renewed, accept().await?);

        // A key caught half written keeps the context it has
        std::fs::write(&key_path, "half written")?;
        assert!(!accept_until_renewed(&renewed, 10).await);
        assert_eq!(renewed, accept().await?);

        // And it's read again once written
        write_certificate(&cert_path, &key_path);
        assert!(accept_until_renewed(&renewed, 100).await);
        assert_eq!(served(), accept().await?);

        std::fs::remove_dir_all(dir)
    }

    #[tokio::test]
    async fn listener_handed_an_acceptor_never_builds_it_again() -> io::Result<()> {
        let acceptor = SslConfig::default().init_tls_server_context(None)?.build();
        let handshaker = SslHandshaker::new(acceptor, None);
        let certificate = handshaker
            .acceptor()
            .await?
            .context()
            .certificate()
            .map(|cert| cert.to_pem())
            .transpose()?;
        assert!(handshaker.source.is_none());
        assert_eq!(
            certificate,
            handshaker
                .clone()
                .acceptor()
                .await?
                .context()
                .certificate()
                .map(|cert| cert.to_pem())
                .transpose()?
        );
        Ok(())
    }

    /// Handshakes and connections are spawned, so they must be `Send`
    #[allow(dead_code)]
    fn handshake_and_connect_are_send(
        handshaker: SslHandshaker,
        stream: Stream,
        target: crate::io::stream::TargetSetting,
    ) {
        fn is_send<T: Send>(_: T) {}
        is_send(async move { handshaker.handshake(stream).await });
        is_send(async move { target.connect().await });
    }

    #[tokio::test]
    async fn target_keeps_its_ssl_context_until_its_files_change() -> io::Result<()> {
        use prosa_utils::config::ssl::Store;

        let dir = unique_test_path("target_ssl");
        std::fs::create_dir_all(&dir)?;
        let cert_path = dir.join("cert.pem");
        let key_path = dir.join("cert.key");
        write_certificate(&cert_path, &key_path);

        // The listener serves the certificate on disk, and the target trusts it
        let listener = ListenerSetting::new(
            Url::parse("https://localhost:0").expect("Listener url is valid"),
            Some(SslConfig::new_cert_key(
                cert_path.to_string_lossy().into_owned(),
                key_path.to_string_lossy().into_owned(),
                None,
            )),
        )
        .bind()
        .await?;
        let mut client_config = SslConfig::default();
        client_config.set_store(Store::File {
            path: cert_path.to_string_lossy().into_owned(),
        });
        let mut target = TargetSetting::new(
            Url::parse(&format!(
                "tls://localhost:{}",
                listener.local_addr()?.port()
            ))
            .expect("Target url is valid"),
            Some(client_config),
            None,
        );

        let connected = async |target: &TargetSetting| {
            let (served, connected) =
                futures_util::future::join(listener.accept(), target.connect()).await;
            served.and(connected).map(|_| ())
        };
        let context = |target: &TargetSetting| {
            target
                .ssl_context
                .current()
                .map(|(connector, _)| std::ptr::from_ref(connector.context()) as usize)
        };
        // Connects until the target holds another context than `previous`, at most `rounds` times
        let connect_until_rebuilt = async |target: &TargetSetting, previous, rounds: usize| {
            for _ in 0..rounds {
                let _ = connected(target).await;
                if context(target) != previous {
                    return true;
                }
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
            false
        };

        // Built once, and kept by the target and its clones
        connected(&target).await?;
        let first = context(&target);
        assert!(first.is_some());
        let clone = target.clone();
        connected(&clone).await?;
        connected(&target).await?;
        assert_eq!(first, context(&target));
        assert_eq!(first, context(&clone));

        // Built again once the trusted certificate is renewed, for the clones too
        write_certificate(&cert_path, &key_path);
        assert!(connect_until_rebuilt(&target, first, 100).await);
        assert_eq!(context(&target), context(&clone));
        connected(&target).await?;

        // A store caught half written keeps the context it has
        let renewed = context(&target);
        std::fs::write(&cert_path, "half written")?;
        assert!(!connect_until_rebuilt(&target, renewed, 10).await);

        // A new configuration is a context of its own, the clones keep theirs
        target.set_alpn(vec!["prosa/1".into()]);
        assert!(context(&target).is_none());
        assert_eq!(renewed, context(&clone));

        std::fs::remove_dir_all(dir)
    }

    #[tokio::test]
    async fn target_fails_without_a_first_ssl_context() {
        let target = TargetSetting::new(
            Url::parse("tls://localhost:1").expect("Target url is valid"),
            Some(SslConfig::new_pkcs12("/nonexistent/prosa.p12".into())),
            None,
        );
        assert!(target.connect().await.is_err());
        assert!(target.ssl_context.current().is_none());
    }
}
