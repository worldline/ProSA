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

#[cfg(feature = "openssl")]
/// SSL parameters a listener serves to the clients it accepts.
///
/// Cheap to clone, because an OpenSSL context is reference counted, so an accept loop can hand one
/// to the task that handshakes a client and go straight back to accepting. A client is served the
/// parameters the listener held when it was accepted.
///
/// ```
/// use tokio::io;
/// use prosa::io::listener::{ListenerSetting, StreamListener};
///
/// async fn accepting(setting: &ListenerSetting) -> Result<(), io::Error> {
///     let stream_listener: StreamListener = setting.bind().await?;
///
///     loop {
///         let (stream, addr) = stream_listener.accept_raw().await?;
///
///         // Owned snapshot of the SSL parameters, so the accept loop never carries the handshake
///         let handshaker = stream_listener.handshaker().cloned();
///         tokio::spawn(async move {
///             let stream = match handshaker {
///                 Some(handshaker) => handshaker.handshake(stream).await?,
///                 None => stream,
///             };
///
///             // Handle the stream like any tokio stream
///             Ok::<_, io::Error>(())
///         });
///     }
/// }
/// ```
#[derive(Clone)]
pub struct SslHandshaker {
    /// Acceptor holding the certificate served to the clients
    acceptor: ::openssl::ssl::SslAcceptor,
    /// Timeout of the SSL handshake with a client
    timeout: Duration,
}

#[cfg(feature = "openssl")]
impl SslHandshaker {
    /// Method to create the SSL parameters served by a listener.
    /// By default, the SSL handshake timeout is 3 seconds
    pub fn new(acceptor: ::openssl::ssl::SslAcceptor, timeout: Option<Duration>) -> SslHandshaker {
        SslHandshaker {
            acceptor,
            timeout: timeout.unwrap_or(StreamListener::DEFAULT_SSL_TIMEOUT),
        }
    }

    /// Getter of the timeout of the SSL handshake with a client
    pub fn ssl_timeout(&self) -> Duration {
        self.timeout
    }

    /// Method to negotiate SSL with a client that has just been accepted.
    /// A stream that is not a plain TCP one is returned as is
    pub async fn handshake(&self, stream: Stream) -> Result<Stream, io::Error> {
        let Stream::Tcp(tcp_stream) = stream else {
            return Ok(stream);
        };

        let ssl = openssl::ssl::Ssl::new(self.acceptor.context())
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
        let mut stream = tokio_openssl::SslStream::new(ssl, tcp_stream)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
        if let Err(e) = tokio::time::timeout(self.timeout, std::pin::Pin::new(&mut stream).accept())
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

#[cfg(feature = "openssl")]
impl fmt::Debug for SslHandshaker {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SslHandshaker")
            .field("ssl_timeout", &self.timeout)
            .field(
                "certificate",
                &self.acceptor.context().certificate().map(|c| c.to_text()),
            )
            .finish()
    }
}

/// ProSA socket object to handle TCP/SSL server socket
pub enum StreamListener {
    #[cfg(target_family = "unix")]
    /// Unix server socket (only on unix systems)
    Unix(tokio::net::UnixListener),
    /// TCP server socket
    Tcp(TcpListener),
    #[cfg(feature = "openssl")]
    /// OpenSSL server socket.
    ///
    /// The SSL parameters are held apart from the socket so
    /// [`StreamListener::set_handshaker`] can serve new ones on the socket it is already bound to
    OpenSsl(TcpListener, SslHandshaker),
}

impl fmt::Debug for StreamListener {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            #[cfg(target_family = "unix")]
            StreamListener::Unix(l) => f.debug_struct("Unix").field("listener", &l).finish(),
            StreamListener::Tcp(l) => f.debug_struct("Tcp").field("listener", &l).finish(),
            #[cfg(feature = "openssl")]
            StreamListener::OpenSsl(l, ssl) => f
                .debug_struct("Ssl")
                .field("listener", &l)
                .field("ssl", &ssl)
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
            StreamListener::OpenSsl(listener, _) => listener.local_addr().map(|addr| addr.into()),
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
        self.set_handshaker(Some(SslHandshaker::new(ssl_acceptor, ssl_timeout)))
    }

    #[cfg(feature = "openssl")]
    /// Getter of the SSL parameters served to a client accepted now, [`None`] on a plain listener.
    ///
    /// Clone it into the task that handshakes the client, so the accept loop can go straight back
    /// to accepting.
    ///
    /// These are the parameters the listener holds, and the ones [`StreamListener::accept`] and
    /// [`StreamListener::handshake`] serve. Rotating a copy taken from here doesn't rotate them, so
    /// a listener that can't be replaced by [`StreamListener::set_handshaker`] keeps serving the
    /// superseded certificate. Bind such a listener with [`ListenerSetting::bind_raw`] instead, so
    /// the handshaker held beside it is the only one
    pub fn handshaker(&self) -> Option<&SslHandshaker> {
        match self {
            StreamListener::OpenSsl(_l, handshaker) => Some(handshaker),
            _ => None,
        }
    }

    #[cfg(feature = "openssl")]
    /// Method to serve new SSL parameters on the socket that is already bound.
    ///
    /// The socket is moved into the returned listener rather than bound again, so the port is
    /// never released and no client is refused: rotating a certificate, turning SSL on and turning
    /// it off all keep the same socket. The connections that are established, and the ones in the
    /// middle of their handshake, keep the parameters they started with; only the clients accepted
    /// afterwards are served the new ones.
    ///
    /// A Unix socket never serves SSL and is returned as is.
    ///
    /// The listener has to be owned to be replaced, so this is for a listener a single task holds.
    /// One that is shared should be bound with [`ListenerSetting::bind_raw`] and rotated through
    /// the handshaker held beside it, otherwise the copy the listener keeps is the one it serves.
    ///
    /// ```
    /// use tokio::io;
    /// use prosa::io::listener::{ListenerSetting, StreamListener};
    ///
    /// async fn rotating(setting: &ListenerSetting) -> Result<(), io::Error> {
    ///     let mut stream_listener: StreamListener = setting.bind().await?;
    ///     let addr = stream_listener.local_addr()?;
    ///
    ///     // The certificate the configuration points at has been renewed. Built before the
    ///     // listener is touched, so a broken certificate leaves it serving the one it has
    ///     let handshaker = setting.build_handshaker().await?;
    ///     stream_listener = stream_listener.set_handshaker(handshaker);
    ///
    ///     // Still the very same socket
    ///     assert_eq!(addr, stream_listener.local_addr()?);
    ///
    ///     Ok(())
    /// }
    /// ```
    pub fn set_handshaker(self, handshaker: Option<SslHandshaker>) -> StreamListener {
        let listener = match self {
            StreamListener::Tcp(listener) | StreamListener::OpenSsl(listener, _) => listener,
            #[cfg(target_family = "unix")]
            unix_listener => return unix_listener,
        };

        match handshaker {
            Some(handshaker) => StreamListener::OpenSsl(listener, handshaker),
            None => StreamListener::Tcp(listener),
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
            StreamListener::OpenSsl(l, handshaker) => {
                let (stream, addr) = l.accept().await?;

                // Read after the accept, so a client that connects once the certificate has been
                // rotated is served the new one
                handshaker
                    .handshake(Stream::Tcp(stream))
                    .await
                    .map(|stream| (stream, addr.into()))
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
            StreamListener::OpenSsl(l, _handshaker) => {
                l.accept().await.map(|s| (Stream::Tcp(s.0), s.1.into()))
            }
        }
    }

    /// Method to do an handshake with a client after an accept (Do nothing if the handshake is already done)
    pub async fn handshake(&self, stream: Stream) -> Result<Stream, io::Error> {
        #[cfg(feature = "openssl")]
        if let StreamListener::OpenSsl(_l, handshaker) = self {
            return handshaker.handshake(stream).await;
        }

        Ok(stream)
    }
}

impl AsFd for StreamListener {
    fn as_fd(&self) -> BorrowedFd<'_> {
        match self {
            #[cfg(target_family = "unix")]
            StreamListener::Unix(l) => l.as_fd(),
            StreamListener::Tcp(l) => l.as_fd(),
            #[cfg(feature = "openssl")]
            StreamListener::OpenSsl(l, _) => l.as_fd(),
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
            StreamListener::OpenSsl(l, _) => l.as_raw_fd(),
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
            StreamListener::OpenSsl(_, _) => write!(f, "ssl://{addr}"),
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

    /// Method to know if serving `other` needs a new socket, because it doesn't listen on the same
    /// address.
    ///
    /// Only the host and the port are compared, so everything else is served on the socket that is
    /// already bound, whether that rotates a certificate or turns SSL on or off. A scheme carries
    /// SSL rather than an address, and `max_socket` is a cap the processor enforces rather than a
    /// property of the socket, so neither of them ever needs a new one.
    ///
    /// ```
    /// use url::Url;
    /// use prosa::io::listener::ListenerSetting;
    ///
    /// let plain = ListenerSetting::from(Url::parse("tcp://[::]:8080").unwrap());
    ///
    /// // Adding SSL under the same URL is served on the socket that is already bound
    /// let mut with_ssl = plain.clone();
    /// with_ssl.set_alpn(vec!["h2".into()]);
    /// assert!(!plain.needs_rebind(&with_ssl));
    ///
    /// // And so is turning SSL on with the scheme
    /// let ssl_scheme = ListenerSetting::from(Url::parse("ssl://[::]:8080").unwrap());
    /// assert!(!plain.needs_rebind(&ssl_scheme));
    ///
    /// // Listening somewhere else needs a new socket
    /// let moved = ListenerSetting::from(Url::parse("tcp://[::]:8081").unwrap());
    /// assert!(plain.needs_rebind(&moved));
    /// ```
    pub fn needs_rebind(&self, other: &ListenerSetting) -> bool {
        self.url.host() != other.url.host()
            || self.url.port_or_known_default() != other.url.port_or_known_default()
    }

    #[cfg(feature = "openssl")]
    /// Method to build the SSL parameters this configuration serves, [`None`] when it listens
    /// without SSL.
    ///
    /// The certificates are read again on every call, on the blocking pool, because [`SslConfig`]
    /// holds their *paths*: a rotation that rewrites a file in place leaves the configuration equal
    /// to what it was, so a caller has nothing to compare and should call this on every
    /// configuration reload.
    ///
    /// Hand the result to [`StreamListener::set_handshaker`] to serve it without rebinding. It is
    /// built before the listener is touched, so a broken certificate leaves the listener serving
    /// the one it already has.
    ///
    /// A listener that is SSL through its URL scheme alone is served a default [`SslConfig`], which
    /// signs a certificate of its own rather than reading one. That certificate is signed again on
    /// every call, so such a listener serves a new identity on every configuration reload and a
    /// client that pins it stops trusting it. Configure a certificate to serve a stable one.
    pub async fn build_handshaker(&self) -> Result<Option<SslHandshaker>, io::Error> {
        // A Unix socket never serves SSL, `bind` ignores the SSL configuration for it
        #[cfg(target_family = "unix")]
        if self.url.scheme() == "unix" || self.url.scheme() == "file" {
            return Ok(None);
        }

        if !self.is_ssl() {
            return Ok(None);
        }

        let ssl_config = self.ssl.clone().unwrap_or_default();
        let timeout = ssl_config.get_ssl_timeout();
        let host = self.url.host_str().map(String::from);

        let acceptor = tokio::task::spawn_blocking(move || {
            ssl_config
                .init_tls_server_context(host.as_deref())
                .map(|ssl_context_builder| ssl_context_builder.build())
        })
        .await
        .map_err(io::Error::other)?
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;

        Ok(Some(SslHandshaker::new(acceptor, Some(timeout))))
    }

    /// Bind the socket of the configuration, without SSL
    async fn bind_socket(&self) -> Result<StreamListener, io::Error> {
        #[cfg(target_family = "unix")]
        if self.url.scheme() == "unix" || self.url.scheme() == "file" {
            return Ok(StreamListener::Unix(UnixListener::bind(self.url.path())?));
        }

        let addrs = self.url.socket_addrs(|| self.url.port_or_known_default())?;

        StreamListener::bind(&*addrs).await
    }

    #[cfg(feature = "openssl")]
    /// Method to bind the socket of the configuration and build the SSL parameters to serve on it,
    /// without attaching them to the listener.
    ///
    /// Use this when the listener has to be shared, held in an [`Arc`](std::sync::Arc) by an accept
    /// loop and by the tasks that handshake its clients, so [`StreamListener::set_handshaker`]
    /// can't replace it. Rotating then means replacing the [`SslHandshaker`] returned here, and
    /// because the listener never holds one there is no second copy of it to go stale.
    ///
    /// The listener describes the socket, so it formats as `tcp://` and its
    /// [`StreamListener::handshaker`] is [`None`] even though the clients accepted on it are handed
    /// a certificate. Hand the handshaker to [`StreamListener::set_handshaker`] instead when the
    /// listener is owned by a single task and should serve it itself.
    ///
    /// ```
    /// use std::sync::Arc;
    /// use tokio::io;
    /// use prosa::io::listener::{ListenerSetting, StreamListener};
    ///
    /// async fn accepting(setting: &ListenerSetting) -> Result<(), io::Error> {
    ///     let (listener, handshaker) = setting.bind_raw().await?;
    ///     let listener = Arc::new(listener);
    ///
    ///     // The socket carries no SSL parameters, the accept loop holds the only copy
    ///     assert!(listener.handshaker().is_none());
    ///
    ///     loop {
    ///         let (stream, addr) = listener.accept_raw().await?;
    ///
    ///         let handshaker = handshaker.clone();
    ///         tokio::spawn(async move {
    ///             let stream = match handshaker {
    ///                 Some(handshaker) => handshaker.handshake(stream).await?,
    ///                 None => stream,
    ///             };
    ///
    ///             // Handle the stream like any tokio stream
    ///             Ok::<_, io::Error>(())
    ///         });
    ///     }
    /// }
    /// ```
    pub async fn bind_raw(&self) -> Result<(StreamListener, Option<SslHandshaker>), io::Error> {
        // Built first, so a broken certificate doesn't take the port on its way out
        let handshaker = self.build_handshaker().await?;

        Ok((self.bind_socket().await?, handshaker))
    }

    /// Method to connect a ProSA stream to the remote target using the configuration
    pub async fn bind(&self) -> Result<StreamListener, io::Error> {
        #[cfg(feature = "openssl")]
        {
            let (stream_listener, handshaker) = self.bind_raw().await?;
            Ok(stream_listener.set_handshaker(handshaker))
        }

        #[cfg(not(feature = "openssl"))]
        {
            if self.is_ssl() {
                return Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    "No SSL engine available",
                ));
            }

            self.bind_socket().await
        }
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

    /// Path of a test file no other test, and no other test run, writes to
    fn unique_test_path(name: &str) -> std::path::PathBuf {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir().join(format!("{}-{timestamp}-{name}", std::process::id()))
    }

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

    #[cfg(feature = "openssl")]
    fn served_certificate(listener: &StreamListener) -> Vec<u8> {
        listener
            .handshaker()
            .expect("The listener should accept SSL connections")
            .acceptor
            .context()
            .certificate()
            .expect("The acceptor should serve a certificate")
            .to_pem()
            .expect("The certificate should be readable")
    }

    #[cfg(feature = "openssl")]
    #[tokio::test]
    async fn listener_rotate_certificate_keeps_the_socket() {
        // Port 0 so the test doesn't depend on one being free, and a default SSL configuration
        // because it generates a self signed certificate, a new one on every build
        let setting = ListenerSetting::from(
            Url::parse("https://127.0.0.1:0").expect("Listener url is valid"),
        );
        let mut listener = setting.bind().await.expect("The listener should bind");

        let addr = listener.local_addr().expect("The listener should be bound");
        let certificate = served_certificate(&listener);

        let handshaker = setting
            .build_handshaker()
            .await
            .expect("The certificate should be read");
        listener = listener.set_handshaker(handshaker);

        // The very same socket, so no client was refused and nothing could steal the port
        assert_eq!(
            addr,
            listener.local_addr().expect("The listener should be bound")
        );
        // And a different certificate, so the rotation really re-read it instead of comparing the
        // configuration it was handed with the one it already had
        assert_ne!(certificate, served_certificate(&listener));
    }

    #[cfg(feature = "openssl")]
    #[tokio::test]
    async fn listener_turns_ssl_on_and_off_on_the_same_socket() {
        let plain =
            ListenerSetting::from(Url::parse("tcp://127.0.0.1:0").expect("Listener url is valid"));
        let ssl = ListenerSetting::from(
            Url::parse("https://127.0.0.1:0").expect("Listener url is valid"),
        );

        let mut listener = plain.bind().await.expect("The listener should bind");
        let addr = listener.local_addr().expect("The listener should be bound");
        assert!(listener.handshaker().is_none());

        // Turning SSL on reuses the socket that is already bound
        listener = listener.set_handshaker(
            ssl.build_handshaker()
                .await
                .expect("The certificate should be read"),
        );
        assert!(!served_certificate(&listener).is_empty());
        assert_eq!(
            addr,
            listener.local_addr().expect("The listener should be bound")
        );

        // And so does turning it off
        listener = listener.set_handshaker(
            plain
                .build_handshaker()
                .await
                .expect("A plain listener has no certificate to read"),
        );
        assert!(listener.handshaker().is_none());
        assert_eq!(
            addr,
            listener.local_addr().expect("The listener should be bound")
        );
    }

    #[cfg(feature = "openssl")]
    #[tokio::test]
    async fn listener_setting_build_handshaker() {
        // A plain listener has no SSL parameters to serve
        assert!(
            ListenerSetting::from(Url::parse("tcp://127.0.0.1:0").expect("Listener url is valid"))
                .build_handshaker()
                .await
                .expect("A plain listener has no certificate to read")
                .is_none()
        );

        // And neither does a Unix socket, even with an explicit SSL configuration, because `bind`
        // ignores it there
        assert!(
            ListenerSetting::new(
                Url::parse("unix:///tmp/prosa_build_handshaker.sock")
                    .expect("Listener url is valid"),
                Some(SslConfig::default()),
            )
            .build_handshaker()
            .await
            .expect("A Unix listener has no certificate to read")
            .is_none()
        );

        let handshaker = ListenerSetting::from(
            Url::parse("https://127.0.0.1:0").expect("Listener url is valid"),
        )
        .build_handshaker()
        .await
        .expect("The certificate should be read")
        .expect("An SSL listener serves SSL parameters");
        assert_eq!(
            SslConfig::default().get_ssl_timeout(),
            handshaker.ssl_timeout()
        );
    }

    #[cfg(feature = "openssl")]
    fn peer_certificate(stream: &Stream) -> Vec<u8> {
        let Stream::OpenSsl(ssl_stream) = stream else {
            panic!("The client should be connected over SSL");
        };

        ssl_stream
            .ssl()
            .peer_certificate()
            .expect("The listener should serve a certificate")
            .to_pem()
            .expect("The certificate should be readable")
    }

    #[cfg(feature = "openssl")]
    #[tokio::test]
    async fn listener_rotate_certificate_serves_the_new_one() -> io::Result<()> {
        use prosa_utils::config::ssl::Store;
        use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

        let cert_path = unique_test_path("test_listener_rotate_certificate.pem")
            .to_str()
            .expect("Certificate path should exist")
            .to_string();

        // Regenerates a self signed certificate on every build and writes it there, so the client
        // trusts whichever one the listener currently serves. Port 0 so the test doesn't depend on
        // one being free
        let setting = ListenerSetting::new(
            Url::parse("https://localhost:0").expect("Listener url is valid"),
            Some(SslConfig::new_self_cert(cert_path.clone())),
        );
        let mut listener = setting.bind().await?;
        let addr = listener.local_addr()?;
        let url =
            Url::parse(&format!("tls://localhost:{}", addr.port())).expect("Target url is valid");

        let connect = || async {
            let mut client_config = SslConfig::default();
            client_config.set_store(Store::File {
                path: cert_path.clone(),
            });
            let connector: ::openssl::ssl::SslConnectorBuilder =
                client_config.init_tls_client_context()?;

            Stream::connect_openssl(&url, &connector.build()).await
        };

        let (served, connected) = futures_util::future::join(listener.accept(), connect()).await;
        let (mut served, _) = served?;
        let mut connected = connected?;
        let certificate = peer_certificate(&connected);

        // The certificate the configuration points at is renewed while the listener accepts
        let handshaker = setting.build_handshaker().await?;
        listener = listener.set_handshaker(handshaker);

        // The very same socket, so no client was refused and nothing could steal the port
        assert_eq!(addr, listener.local_addr()?);

        // The connection established before the rotation keeps working
        connected.write_all(b"ProSA").await?;
        let mut buf = [0; 5];
        served.read_exact(&mut buf).await?;
        assert_eq!(&buf, b"ProSA");

        // And a client connecting afterwards is served the new certificate
        let (served, connected) = futures_util::future::join(listener.accept(), connect()).await;
        served?;
        assert_ne!(certificate, peer_certificate(&connected?));

        Ok(())
    }

    #[cfg(feature = "openssl")]
    #[tokio::test]
    async fn listener_setting_bind_raw_leaves_the_ssl_parameters_out() {
        // An SSL listener bound raw serves the socket only, so the handshaker handed back is the
        // only copy of the SSL parameters
        let (listener, handshaker) = ListenerSetting::from(
            Url::parse("https://localhost:0").expect("Listener url is valid"),
        )
        .bind_raw()
        .await
        .expect("The listener should bind");
        assert!(handshaker.is_some());
        assert!(listener.handshaker().is_none());
        assert!(matches!(listener, StreamListener::Tcp(_)));
        assert!(listener.to_string().starts_with("tcp://"));

        // A plain listener has none to hand back
        let (listener, handshaker) =
            ListenerSetting::from(Url::parse("tcp://localhost:0").expect("Listener url is valid"))
                .bind_raw()
                .await
                .expect("The listener should bind");
        assert!(handshaker.is_none());
        assert!(listener.handshaker().is_none());

        // And neither does a Unix socket, even with an explicit SSL configuration
        let socket_path = unique_test_path("test_listener_bind_raw.sock");
        let (listener, handshaker) = ListenerSetting::new(
            Url::parse(&format!(
                "unix://{}",
                socket_path.to_str().expect("Socket path should be string")
            ))
            .expect("Listener url is valid"),
            Some(SslConfig::default()),
        )
        .bind_raw()
        .await
        .expect("The listener should bind");
        assert!(handshaker.is_none());
        assert!(matches!(listener, StreamListener::Unix(_)));
    }

    #[cfg(feature = "openssl")]
    #[tokio::test]
    async fn listener_bound_raw_rotates_through_its_own_handshaker() -> io::Result<()> {
        use prosa_utils::config::ssl::Store;
        use std::sync::Arc;

        let cert_path = unique_test_path("test_listener_bind_raw_rotate.pem")
            .to_str()
            .expect("Certificate path should exist")
            .to_string();

        let setting = ListenerSetting::new(
            Url::parse("https://localhost:0").expect("Listener url is valid"),
            Some(SslConfig::new_self_cert(cert_path.clone())),
        );

        // The shape of a listener that is shared with the tasks handshaking its clients, so it
        // can't be replaced to rotate
        let (listener, mut handshaker) = setting.bind_raw().await?;
        let listener = Arc::new(listener);
        let addr = listener.local_addr()?;
        let url =
            Url::parse(&format!("tls://localhost:{}", addr.port())).expect("Target url is valid");

        let connect = || async {
            let mut client_config = SslConfig::default();
            client_config.set_store(Store::File {
                path: cert_path.clone(),
            });
            let connector: ::openssl::ssl::SslConnectorBuilder =
                client_config.init_tls_client_context()?;

            Stream::connect_openssl(&url, &connector.build()).await
        };

        let serve = |handshaker: Option<SslHandshaker>| {
            let listener = listener.clone();
            async move {
                let (stream, _addr) = listener.accept_raw().await?;
                match handshaker {
                    Some(handshaker) => handshaker.handshake(stream).await,
                    None => Ok(stream),
                }
            }
        };

        let (served, connected) =
            futures_util::future::join(serve(handshaker.clone()), connect()).await;
        served?;
        let certificate = peer_certificate(&connected?);

        // Rotating replaces the only copy there is
        handshaker = setting.build_handshaker().await?;

        // The very same socket, and the listener still carries nothing that could go stale
        assert_eq!(addr, listener.local_addr()?);
        assert!(listener.handshaker().is_none());

        // And the next client is served the new certificate
        let (served, connected) =
            futures_util::future::join(serve(handshaker.clone()), connect()).await;
        served?;
        assert_ne!(certificate, peer_certificate(&connected?));

        Ok(())
    }

    #[test]
    fn listener_setting_needs_rebind() {
        let plain = ListenerSetting::from(
            Url::parse("tcp://localhost:8080").expect("Listener url is invalid"),
        );

        // Turning SSL on with the scheme keeps the socket, like turning it on with a configuration
        assert!(!plain.needs_rebind(&ListenerSetting::from(
            Url::parse("ssl://localhost:8080").expect("Listener url is invalid")
        )));
        assert!(!plain.needs_rebind(&ListenerSetting::new(
            plain.url.clone(),
            Some(SslConfig::default())
        )));

        // And so does anything else that doesn't name an address
        let mut path_changed = plain.clone();
        path_changed.url.set_path("/v2");
        path_changed.max_socket = plain.max_socket / 2;
        assert!(!plain.needs_rebind(&path_changed));

        // Listening on another port or another host needs a new socket
        assert!(plain.needs_rebind(&ListenerSetting::from(
            Url::parse("tcp://localhost:8081").expect("Listener url is invalid")
        )));
        assert!(plain.needs_rebind(&ListenerSetting::from(
            Url::parse("tcp://127.0.0.1:8080").expect("Listener url is invalid")
        )));

        // A scheme that implies another port does too
        assert!(
            ListenerSetting::from(Url::parse("http://localhost").expect("Listener url is invalid"))
                .needs_rebind(&ListenerSetting::from(
                    Url::parse("https://localhost").expect("Listener url is invalid")
                ))
        );
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
