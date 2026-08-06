//! Module that define listener IO that could be use by a ProSA processor
use std::{
    fmt, io,
    net::{Ipv4Addr, SocketAddrV4},
    os::fd::{AsFd, AsRawFd, BorrowedFd, RawFd},
    time::Duration,
};

use prosa_utils::config::ssl::SslConfig;
#[cfg(feature = "openssl")]
use prosa_utils::config::ssl::SslConfigContext as _;

use serde::{Deserialize, Serialize};

pub use prosa_macros::io;
use tokio::net::{TcpListener, ToSocketAddrs, UnixListener};
use url::Url;

use super::{SafeUrl, SocketAddr, get_safe_url, stream::Stream, url_is_ssl};

/// ProSA socket object to handle TCP/SSL server socket
pub enum StreamListener {
    #[cfg(target_family = "unix")]
    /// Unix server socket (only on unix systems)
    Unix(tokio::net::UnixListener),
    /// TCP server socket
    Tcp(TcpListener),
    #[cfg(feature = "openssl")]
    /// OpenSSL server socket
    OpenSsl(TcpListener, ::openssl::ssl::SslAcceptor, Duration),
}

impl fmt::Debug for StreamListener {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            #[cfg(target_family = "unix")]
            StreamListener::Unix(l) => f.debug_struct("Unix").field("listener", &l).finish(),
            StreamListener::Tcp(l) => f.debug_struct("Tcp").field("listener", &l).finish(),
            #[cfg(feature = "openssl")]
            StreamListener::OpenSsl(l, a, t) => f
                .debug_struct("Ssl")
                .field("listener", &l)
                .field("ssl_timeout", &t)
                .field(
                    "certificate",
                    &a.context().certificate().map(|c| c.to_text()),
                )
                .finish(),
        }
    }
}

impl StreamListener {
    /// Default SSL handshake timeout
    pub const DEFAULT_SSL_TIMEOUT: Duration = Duration::new(3, 0);

    /// Returns the local address that this listener is bound to.
    ///
    /// This can be useful, for example, when binding to port 0 to figure out
    /// which port was actually bound.
    ///
    /// ```
    /// use tokio::io;
    /// use prosa::io::listener::StreamListener;
    /// use prosa::io::SocketAddr;
    /// use std::net::{Ipv4Addr, SocketAddrV4};
    ///
    /// async fn accepting() -> Result<(), io::Error> {
    ///     let stream_listener: StreamListener = StreamListener::bind("0.0.0.0:10000").await?;
    ///
    ///     assert_eq!(stream_listener.local_addr()?,
    ///                SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(0, 0, 0, 0), 10000)));
    ///
    ///     Ok(())
    /// }
    /// ```
    pub fn local_addr(&self) -> Result<SocketAddr, io::Error> {
        match self {
            #[cfg(target_family = "unix")]
            StreamListener::Unix(listener) => listener.local_addr().map(|addr| addr.into()),
            StreamListener::Tcp(listener) => listener.local_addr().map(|addr| addr.into()),
            #[cfg(feature = "openssl")]
            StreamListener::OpenSsl(listener, _, _) => {
                listener.local_addr().map(|addr| addr.into())
            }
        }
    }

    /// Accept TCP connections from clients
    ///
    #[doc = simple_mermaid::mermaid!("diagrams/listener_tcp.mmd")]
    ///
    /// ```
    /// use tokio::io;
    /// use prosa::io::listener::StreamListener;
    ///
    /// async fn accepting() -> Result<(), io::Error> {
    ///     let stream_listener: StreamListener = StreamListener::bind("0.0.0.0:10000").await?;
    ///
    ///     loop {
    ///         let (stream, addr) = stream_listener.accept().await?;
    ///
    ///         // Handle the stream like any tokio stream
    ///     }
    ///
    ///     Ok(())
    /// }
    /// ```
    pub async fn bind<A: ToSocketAddrs>(addr: A) -> Result<StreamListener, io::Error> {
        Ok(StreamListener::Tcp(TcpListener::bind(addr).await?))
    }

    #[cfg(feature = "openssl")]
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
        ssl_acceptor: ::openssl::ssl::SslAcceptor,
        ssl_timeout: Option<Duration>,
    ) -> StreamListener {
        match self {
            StreamListener::Tcp(listener) => StreamListener::OpenSsl(
                listener,
                ssl_acceptor,
                ssl_timeout.unwrap_or(Self::DEFAULT_SSL_TIMEOUT),
            ),
            StreamListener::OpenSsl(listener, _, _) => StreamListener::OpenSsl(
                listener,
                ssl_acceptor,
                ssl_timeout.unwrap_or(Self::DEFAULT_SSL_TIMEOUT),
            ),
            _ => self,
        }
    }

    /// Method to accept a client after a bind
    ///
    /// ```
    /// use tokio::io;
    /// use prosa::io::{
    ///     listener::StreamListener,
    ///     SslConfig,
    ///     SslConfigContext,
    /// };
    ///
    /// # #[cfg(feature="openssl")]
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
    pub async fn accept(&self) -> Result<(Stream, SocketAddr), io::Error> {
        match self {
            #[cfg(target_family = "unix")]
            StreamListener::Unix(l) => l.accept().await.map(|s| (Stream::Unix(s.0), s.1.into())),
            StreamListener::Tcp(l) => l.accept().await.map(|s| (Stream::Tcp(s.0), s.1.into())),
            #[cfg(feature = "openssl")]
            StreamListener::OpenSsl(l, ssl_acceptor, ssl_timeout) => {
                let ssl = openssl::ssl::Ssl::new(ssl_acceptor.context())
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
                let (stream, addr) = l.accept().await?;
                let mut stream = tokio_openssl::SslStream::new(ssl, stream)
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
                if let Err(e) =
                    tokio::time::timeout(*ssl_timeout, std::pin::Pin::new(&mut stream).accept())
                        .await
                        .map_err(|_| {
                            io::Error::new(
                                io::ErrorKind::TimedOut,
                                format!(
                                    "SSL timeout[{} ms] for {:?}",
                                    ssl_timeout.as_millis(),
                                    stream
                                ),
                            )
                        })?
                {
                    return Err(io::Error::other(format!("Can't accept the client: {e}")));
                }

                Ok((Stream::OpenSsl(stream), addr.into()))
            }
        }
    }

    /// Method to accept a client after a bind without SSL handshake (must be done with handshake after)
    ///
    /// ```
    /// use tokio::io;
    /// use prosa::io::{
    ///     listener::StreamListener,
    ///     SslConfig,
    ///     SslConfigContext,
    /// };
    ///
    /// # #[cfg(feature="openssl")]
    /// async fn accepting() -> Result<(), io::Error> {
    ///     let ssl_acceptor = SslConfig::default().init_tls_server_context(None).unwrap().build();
    ///     let stream_listener: StreamListener = StreamListener::bind("0.0.0.0:10000").await?.ssl_acceptor(ssl_acceptor, None);
    ///
    ///     loop {
    ///         let (stream, addr) = stream_listener.accept_raw().await?;
    ///
    ///         // The client SSL handshake will happen here
    ///         let stream = stream_listener.handshake(stream).await?;
    ///
    ///         // Handle the stream like any tokio stream
    ///     }
    ///
    ///     Ok(())
    /// }
    /// ```
    pub async fn accept_raw(&self) -> Result<(Stream, SocketAddr), io::Error> {
        match self {
            #[cfg(target_family = "unix")]
            StreamListener::Unix(l) => l.accept().await.map(|s| (Stream::Unix(s.0), s.1.into())),
            StreamListener::Tcp(l) => l.accept().await.map(|s| (Stream::Tcp(s.0), s.1.into())),
            #[cfg(feature = "openssl")]
            StreamListener::OpenSsl(l, _ssl_acceptor, _ssl_timeout) => {
                l.accept().await.map(|s| (Stream::Tcp(s.0), s.1.into()))
            }
        }
    }

    /// Method to do an handshake with a client after an accept (Do nothing if the handshake is already done)
    pub async fn handshake(&self, stream: Stream) -> Result<Stream, io::Error> {
        match stream {
            Stream::Tcp(tcp_stream) => match self {
                #[cfg(feature = "openssl")]
                StreamListener::OpenSsl(_l, ssl_acceptor, ssl_timeout) => {
                    let ssl = openssl::ssl::Ssl::new(ssl_acceptor.context())
                        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
                    let mut stream = tokio_openssl::SslStream::new(ssl, tcp_stream)
                        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
                    if let Err(e) =
                        tokio::time::timeout(*ssl_timeout, std::pin::Pin::new(&mut stream).accept())
                            .await
                            .map_err(|_| {
                                io::Error::new(
                                    io::ErrorKind::TimedOut,
                                    format!(
                                        "SSL timeout[{} ms] for {:?}",
                                        ssl_timeout.as_millis(),
                                        stream
                                    ),
                                )
                            })?
                    {
                        return Err(io::Error::other(format!("Can't accept the client: {e}")));
                    }

                    Ok(Stream::OpenSsl(stream))
                }
                _ => Ok(Stream::Tcp(tcp_stream)),
            },
            s => Ok(s),
        }
    }
}

impl AsFd for StreamListener {
    fn as_fd(&self) -> BorrowedFd<'_> {
        match self {
            #[cfg(target_family = "unix")]
            StreamListener::Unix(l) => l.as_fd(),
            StreamListener::Tcp(l) => l.as_fd(),
            #[cfg(feature = "openssl")]
            StreamListener::OpenSsl(l, _, _) => l.as_fd(),
        }
    }
}

impl AsRawFd for StreamListener {
    fn as_raw_fd(&self) -> RawFd {
        match self {
            #[cfg(target_family = "unix")]
            StreamListener::Unix(l) => l.as_raw_fd(),
            StreamListener::Tcp(l) => l.as_raw_fd(),
            #[cfg(feature = "openssl")]
            StreamListener::OpenSsl(l, _, _) => l.as_raw_fd(),
        }
    }
}

impl fmt::Display for StreamListener {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let addr = self
            .local_addr()
            .unwrap_or(SocketAddr::V4(SocketAddrV4::new(
                Ipv4Addr::new(0, 0, 0, 0),
                0,
            )));
        match self {
            #[cfg(target_family = "unix")]
            StreamListener::Unix(_) => write!(f, "unix://{addr}"),
            StreamListener::Tcp(_) => write!(f, "tcp://{addr}"),
            #[cfg(feature = "openssl")]
            StreamListener::OpenSsl(_, _, _) => write!(f, "ssl://{addr}"),
        }
    }
}

#[cfg(target_family = "unix")]
impl From<tokio::net::UnixListener> for StreamListener {
    fn from(listener: tokio::net::UnixListener) -> Self {
        StreamListener::Unix(listener)
    }
}

impl From<TcpListener> for StreamListener {
    fn from(listener: TcpListener) -> Self {
        StreamListener::Tcp(listener)
    }
}

/// Configuration struct of an network listener
///
/// ```
/// use tokio::io;
/// use url::Url;
/// use prosa::io::stream::Stream;
/// use prosa::io::listener::{ListenerSetting, StreamListener};
///
/// async fn accepting() -> Result<(), io::Error> {
///     let wl_target = ListenerSetting::new(Url::parse("https://[::]").unwrap(), None);
///     let stream: StreamListener = wl_target.bind().await?;
///
///     // Use the StreamListener object to accept clients
///
///     Ok(())
/// }
/// ```
#[derive(Deserialize, Serialize, Clone, PartialEq)]
pub struct ListenerSetting {
    /// Url of the listening
    pub url: Url,
    /// SSL configuration of the listener
    ssl: Option<SslConfig>,
    #[serde(skip_serializing)]
    #[serde(default = "ListenerSetting::default_max_socket")]
    /// Maximum number of socket
    pub max_socket: u64,
}

impl ListenerSetting {
    fn default_max_socket() -> u64 {
        cfg_select! {
            target_family = "unix" => {
                rlimit::Resource::NOFILE
                    .get_soft()
                    .unwrap_or(u32::MAX as u64)
                    - 1
            }
            target_family = "windows" => {
                (rlimit::getmaxstdio() as u64) - 1
            }
            _ => {
                (u32::MAX as u64) - 1
            }
        }
    }

    /// Method to create manually a target
    pub fn new(url: Url, ssl: Option<SslConfig>) -> ListenerSetting {
        ListenerSetting {
            url,
            ssl,
            max_socket: Self::default_max_socket(),
        }
    }

    /// Method to know if the listener will accept SSL connections
    ///
    /// ```
    /// use url::Url;
    /// use prosa::io::listener::ListenerSetting;
    ///
    /// assert!(ListenerSetting::from(Url::parse("https://[::]:4443").unwrap()).is_ssl());
    /// assert!(!ListenerSetting::from(Url::parse("tcp://[::]:8080").unwrap()).is_ssl());
    /// ```
    pub fn is_ssl(&self) -> bool {
        self.ssl.is_some() || url_is_ssl(&self.url)
    }

    /// Getter of the SSL configuration of the listener
    pub fn ssl(&self) -> Option<&SslConfig> {
        self.ssl.as_ref()
    }

    /// Setter of the SSL configuration of the listener
    pub fn set_ssl(&mut self, ssl: Option<SslConfig>) {
        self.ssl = ssl;
    }

    /// Method to set the ALPN protocols to negotiate.
    /// A default SSL configuration is created if the listener accepts SSL connections but none was configured.
    ///
    /// Nothing is done for a plain listener. Idempotent, so it can be called on every configuration reload.
    ///
    /// ```
    /// use url::Url;
    /// use prosa::io::listener::ListenerSetting;
    ///
    /// let mut ssl_listener = ListenerSetting::from(Url::parse("https://[::]:4443").unwrap());
    /// ssl_listener.set_alpn(vec!["h2".into()]);
    /// assert!(ssl_listener.ssl().is_some());
    ///
    /// let mut plain_listener = ListenerSetting::from(Url::parse("tcp://[::]:8080").unwrap());
    /// plain_listener.set_alpn(vec!["h2".into()]);
    /// assert!(plain_listener.ssl().is_none());
    /// ```
    pub fn set_alpn(&mut self, alpn: Vec<String>) {
        if self.is_ssl() {
            self.ssl.get_or_insert_default().set_alpn(alpn);
        }
    }

    /// Return a borrowed safe view of the listener URL.
    ///
    /// Formatting the [`SafeUrl`] masks credentials and omits the query and fragment without
    /// cloning. Callers can use [`SafeUrl::to_url`] to obtain an owned URL without credentials, or
    /// [`SafeUrl::to_mask_url`] to obtain one with masked credentials.
    pub fn get_safe_url(&self) -> SafeUrl<'_> {
        get_safe_url(&self.url)
    }

    /// Method to connect a ProSA stream to the remote target using the configuration
    pub async fn bind(&self) -> Result<StreamListener, io::Error> {
        #[cfg(target_family = "unix")]
        if self.url.scheme() == "unix" || self.url.scheme() == "file" {
            return Ok(StreamListener::Unix(UnixListener::bind(self.url.path())?));
        }

        let addrs = self.url.socket_addrs(|| self.url.port_or_known_default())?;

        #[allow(unused_mut)]
        let mut stream_listener = StreamListener::bind(&*addrs).await?;

        if self.is_ssl() {
            #[cfg(feature = "openssl")]
            {
                let ssl_config = self.ssl.clone().unwrap_or_default();
                let ssl_timeout = ssl_config.get_ssl_timeout();
                let host = self.url.host_str().map(String::from);

                // Reading the certificates blocks, so it's done on the blocking pool
                let ssl_acceptor = tokio::task::spawn_blocking(move || {
                    ssl_config
                        .init_tls_server_context(host.as_deref())
                        .map(|ssl_context_builder| ssl_context_builder.build())
                })
                .await
                .map_err(io::Error::other)?
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
                stream_listener = stream_listener.ssl_acceptor(ssl_acceptor, Some(ssl_timeout));
                return Ok(stream_listener);
            }

            #[cfg(not(feature = "openssl"))]
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "No SSL engine available",
            ));
        }

        Ok(stream_listener)
    }
}

impl From<Url> for ListenerSetting {
    fn from(url: Url) -> Self {
        ListenerSetting {
            url,
            ssl: None,
            max_socket: Self::default_max_socket(),
        }
    }
}

impl fmt::Debug for ListenerSetting {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ListenerSetting")
            .field("url", &self.get_safe_url())
            .field("ssl", &self.ssl)
            .field("max_socket", &self.max_socket)
            .finish()
    }
}

impl fmt::Display for ListenerSetting {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut url = self.get_safe_url().to_mask_url();
        if self.ssl.is_some() {
            let url_scheme = url.scheme();
            if url_scheme.is_empty() {
                let _ = url.set_scheme("ssl");
            } else if !url_scheme.ends_with("ssl")
                && !url_scheme.ends_with("tls")
                && !url_scheme.ends_with("https")
                && !url_scheme.ends_with("wss")
            {
                let _ = url.set_scheme(format!("{url_scheme}+ssl").as_str());
            }
        }

        write!(f, "{} -max_socket {}", url, self.max_socket)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listener_setting_display_redacts_url_secrets() {
        let mut setting = ListenerSetting::from(
            Url::parse("tcp://admin:secret@localhost:8080/v1?token=secret#access_token=secret")
                .expect("Listener URL should be valid"),
        );
        setting.max_socket = 42;

        assert_eq!(
            "tcp://***:***@localhost:8080/v1",
            setting.get_safe_url().to_string()
        );
        assert_eq!(
            "tcp://localhost:8080/v1",
            setting.get_safe_url().to_url().as_str()
        );
        assert_eq!(
            "tcp://***:***@localhost:8080/v1 -max_socket 42",
            setting.to_string()
        );
    }

    #[test]
    fn listener_setting_partial_eq() {
        let config = "url = \"https://localhost:4443\"\nssl = { alpn = [\"h2\"] }\n";
        let setting: ListenerSetting =
            toml::from_str(config).expect("Listener settings should deserialize");

        assert_eq!(
            setting,
            toml::from_str::<ListenerSetting>(config)
                .expect("Listener settings should deserialize")
        );
        assert_ne!(
            setting,
            toml::from_str::<ListenerSetting>(
                "url = \"https://localhost:4444\"\nssl = { alpn = [\"h2\"] }\n"
            )
            .expect("Listener settings should deserialize")
        );
        assert_ne!(
            setting,
            toml::from_str::<ListenerSetting>(
                "url = \"https://localhost:4443\"\nssl = { alpn = [\"http/1.1\"] }\n"
            )
            .expect("Listener settings should deserialize")
        );

        // A programmatically built listener matches its configured counterpart
        let mut built = ListenerSetting::new(
            Url::parse("https://localhost:4443").expect("Listener url is invalid"),
            None,
        );
        built.set_alpn(vec!["h2".into()]);
        assert_eq!(setting, built);
    }

    #[test]
    fn listener_setting_set_alpn() {
        let mut expected_ssl = SslConfig::default();
        expected_ssl.set_alpn(vec!["h2".into()]);

        // An SSL url without SSL configuration gets a default one
        let mut ssl_url = ListenerSetting::from(
            Url::parse("https://[::]:4443").expect("Listener url is invalid"),
        );
        ssl_url.set_alpn(vec!["h2".into()]);
        assert_eq!(Some(&expected_ssl), ssl_url.ssl());

        // Idempotent, so a configuration reload doesn't rebind
        let reloaded = {
            let mut reloaded = ssl_url.clone();
            reloaded.set_alpn(vec!["h2".into()]);
            reloaded
        };
        assert_eq!(ssl_url, reloaded);

        // A plain listener has nothing to negotiate
        let mut plain_url =
            ListenerSetting::from(Url::parse("tcp://[::]:8080").expect("Listener url is invalid"));
        plain_url.set_alpn(vec!["h2".into()]);
        assert_eq!(None, plain_url.ssl());

        // But a plain url with an explicit SSL configuration does use SSL
        let mut plain_url_with_ssl = ListenerSetting::new(
            Url::parse("tcp://[::]:8080").expect("Listener url is invalid"),
            Some(SslConfig::default()),
        );
        plain_url_with_ssl.set_alpn(vec!["h2".into()]);
        assert_eq!(Some(&expected_ssl), plain_url_with_ssl.ssl());
    }
}
