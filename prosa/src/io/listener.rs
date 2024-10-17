//! Module that define listener IO that could be use by a ProSA processor
use std::{
    fmt, io,
    os::fd::{AsFd, AsRawFd, BorrowedFd, RawFd},
    pin::Pin,
    time::Duration,
};

use openssl::ssl::SslAcceptor;
use prosa_utils::config::ssl::SslConfig;
use serde::{Deserialize, Serialize};

pub use prosa_macros::io;
use tokio::{
    net::{TcpListener, ToSocketAddrs, UnixListener},
    time::timeout,
};
use url::Url;

use super::{stream::Stream, url_is_ssl, SocketAddr};

/// ProSA socket object to handle TCP/SSL server socket
pub enum StreamListener {
    #[cfg(target_family = "unix")]
    /// Unix server socket (only on unix systems)
    Unix(tokio::net::UnixListener),
    /// TCP server socket
    Tcp(TcpListener),
    /// SSL server socket
    Ssl(TcpListener, SslAcceptor, Duration),
}

impl fmt::Debug for StreamListener {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            #[cfg(target_family = "unix")]
            StreamListener::Unix(l) => f.debug_struct("Unix").field("listener", &l).finish(),
            StreamListener::Tcp(l) => f.debug_struct("Tcp").field("listener", &l).finish(),
            StreamListener::Ssl(l, a, t) => f
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

    #[cfg_attr(doc, aquamarine::aquamarine)]
    /// Accept TCP connections from clients
    ///
    /// ```mermaid
    /// graph LR
    ///     clients[Clients]
    ///     server[Server]
    ///
    ///     clients -- TCP --> server
    /// ```
    ///
    /// ```
    /// use tokio::io;
    /// use prosa::io::listener::StreamListener;
    ///
    /// async fn accepting() -> Result<(), io::Error> {
    ///     let stream_listenner: StreamListener = StreamListener::bind("0.0.0.0:10000").await?;
    ///
    ///     loop {
    ///         let (stream, addr) = stream_listenner.accept().await?;
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

    #[cfg_attr(doc, aquamarine::aquamarine)]
    /// Set an OpenSSL acceptor to accept SSL connections from clients
    /// By default, the SSL connect timeout is 3 seconds
    ///
    /// ```mermaid
    /// graph LR
    ///     clients[Clients]
    ///     server[Server]
    ///
    ///     clients -- TLS --> server
    /// ```
    ///
    /// ```
    /// use tokio::io;
    /// use prosa_utils::config::ssl::SslConfig;
    /// use prosa::io::listener::StreamListener;
    ///
    /// async fn accepting() -> Result<(), io::Error> {
    ///     let ssl_acceptor = SslConfig::default().init_tls_server_context(None).unwrap().build();
    ///     let stream_listenner: StreamListener = StreamListener::bind("0.0.0.0:10000").await?.ssl_acceptor(ssl_acceptor, None);
    ///
    ///     loop {
    ///         // The client SSL handshake will happen here
    ///         let (stream, addr) = stream_listenner.accept().await?;
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
        match self {
            StreamListener::Tcp(listenner) => StreamListener::Ssl(
                listenner,
                ssl_acceptor,
                ssl_timeout.unwrap_or(Self::DEFAULT_SSL_TIMEOUT),
            ),
            StreamListener::Ssl(listenner, _, _) => StreamListener::Ssl(
                listenner,
                ssl_acceptor,
                ssl_timeout.unwrap_or(Self::DEFAULT_SSL_TIMEOUT),
            ),
            _ => self,
        }
    }

    /// Method to accept a client after a bind
    /// The ssl_timeout is used to not wait undefinetly the client handshake
    pub async fn accept(&self) -> Result<(Stream, SocketAddr), io::Error> {
        match self {
            #[cfg(target_family = "unix")]
            StreamListener::Unix(l) => l.accept().await.map(|s| (Stream::Unix(s.0), s.1.into())),
            StreamListener::Tcp(l) => l.accept().await.map(|s| (Stream::Tcp(s.0), s.1.into())),
            StreamListener::Ssl(l, ssl_acceptor, ssl_timeout) => {
                let ssl = openssl::ssl::Ssl::new(ssl_acceptor.context())
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
                let (stream, addr) = l.accept().await?;
                let mut stream = tokio_openssl::SslStream::new(ssl, stream)
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
                if let Err(e) = timeout(*ssl_timeout, Pin::new(&mut stream).accept())
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
                    if e.code() != openssl::ssl::ErrorCode::ZERO_RETURN {
                        return Err(io::Error::new(
                            io::ErrorKind::Other,
                            format!("Can't accept the client: {}", e),
                        ));
                    }
                }

                Ok((Stream::Ssl(stream), addr.into()))
            }
        }
    }
}

impl AsFd for StreamListener {
    fn as_fd(&self) -> BorrowedFd<'_> {
        match self {
            #[cfg(target_family = "unix")]
            StreamListener::Unix(l) => l.as_fd(),
            StreamListener::Tcp(l) => l.as_fd(),
            StreamListener::Ssl(l, _, _) => l.as_fd(),
        }
    }
}

impl AsRawFd for StreamListener {
    fn as_raw_fd(&self) -> RawFd {
        match self {
            #[cfg(target_family = "unix")]
            StreamListener::Unix(l) => l.as_raw_fd(),
            StreamListener::Tcp(l) => l.as_raw_fd(),
            StreamListener::Ssl(l, _, _) => l.as_raw_fd(),
        }
    }
}

#[cfg(target_family = "unix")]
impl From<tokio::net::UnixListener> for StreamListener {
    fn from(listenner: tokio::net::UnixListener) -> Self {
        StreamListener::Unix(listenner)
    }
}

impl From<TcpListener> for StreamListener {
    fn from(listenner: TcpListener) -> Self {
        StreamListener::Tcp(listenner)
    }
}

/// Configuration struct of an network listenner
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
#[derive(Deserialize, Serialize, Clone)]
pub struct ListenerSetting {
    /// Url of the listening
    pub url: Url,
    /// SSL configuration for target destination
    pub ssl: Option<SslConfig>,
    #[serde(skip)]
    /// OpenSSL configuration for target destination
    ssl_context: Option<SslAcceptor>,
    #[serde(skip_serializing)]
    #[serde(default = "ListenerSetting::default_max_socket")]
    /// Maximum number of socket
    pub max_socket: u64,
}

impl ListenerSetting {
    #[cfg(target_family = "unix")]
    fn default_max_socket() -> u64 {
        rlimit::Resource::NOFILE
            .get_soft()
            .unwrap_or(u32::MAX as u64)
            - 1
    }

    #[cfg(target_family = "windows")]
    fn default_max_socket() -> u64 {
        (rlimit::getmaxstdio() as u64) - 1
    }

    #[cfg(all(not(target_family = "unix"), not(target_family = "windows")))]
    fn default_max_socket() -> u64 {
        (u32::MAX as u64) - 1
    }

    /// Method to create manually a target
    pub fn new(url: Url, ssl: Option<SslConfig>) -> ListenerSetting {
        let mut target = ListenerSetting {
            url: url.clone(),
            ssl,
            ssl_context: None,
            max_socket: Self::default_max_socket(),
        };

        target.init_ssl_context(url.domain());
        target
    }

    /// Method to init the ssl context out of the ssl target configuration.
    /// Must be call when the configuration is retrieved
    pub fn init_ssl_context(&mut self, domain: Option<&str>) {
        if let Some(ssl_config) = &self.ssl {
            if let Ok(ssl_context_builder) = ssl_config.init_tls_server_context(domain) {
                self.ssl_context = Some(ssl_context_builder.build());
            }
        }
    }

    /// Method to connect a ProSA stream to the remote target using the configuration
    pub async fn bind(&self) -> Result<StreamListener, io::Error> {
        #[cfg(target_family = "unix")]
        if self.url.scheme() == "unix" || self.url.scheme() == "file" {
            return Ok(StreamListener::Unix(UnixListener::bind(self.url.path())?));
        }

        let addrs = self.url.socket_addrs(|| self.url.port_or_known_default())?;
        let mut stream_listener = StreamListener::bind(&*addrs).await?;

        if let Some(ssl_acceptor) = &self.ssl_context {
            stream_listener = stream_listener.ssl_acceptor(
                ssl_acceptor.clone(),
                self.ssl.as_ref().map(|c| c.get_ssl_timeout()),
            );
        } else if let Some(ssl_config) = &self.ssl {
            if let Ok(ssl_acceptor_builder) = ssl_config.init_tls_server_context(self.url.domain())
            {
                stream_listener = stream_listener.ssl_acceptor(
                    ssl_acceptor_builder.build(),
                    Some(ssl_config.get_ssl_timeout()),
                );
            }
        } else if url_is_ssl(&self.url) {
            let ssl_config = SslConfig::default();
            if let Ok(ssl_acceptor_builder) = ssl_config.init_tls_server_context(self.url.domain())
            {
                stream_listener = stream_listener.ssl_acceptor(
                    ssl_acceptor_builder.build(),
                    Some(ssl_config.get_ssl_timeout()),
                );
            }
        }

        Ok(stream_listener)
    }
}

impl From<Url> for ListenerSetting {
    fn from(url: Url) -> Self {
        ListenerSetting {
            url,
            ssl: None,
            ssl_context: None,
            max_socket: Self::default_max_socket(),
        }
    }
}

impl fmt::Debug for ListenerSetting {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ListenerSetting")
            .field("url", &self.url)
            .field("ssl", &self.ssl)
            .field("max_socket", &self.max_socket)
            .finish()
    }
}

impl fmt::Display for ListenerSetting {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut url = self.url.clone();
        if self.ssl.is_some() {
            let url_scheme = url.scheme();
            if url_scheme.is_empty() {
                let _ = url.set_scheme("ssl");
            } else if !url_scheme.ends_with("ssl")
                && !url_scheme.ends_with("tls")
                && !url_scheme.ends_with("https")
                && !url_scheme.ends_with("wss")
            {
                let _ = url.set_scheme(format!("{}+ssl", url_scheme).as_str());
            }
        }

        write!(f, "{} -max_socket {}", url, self.max_socket)
    }
}
