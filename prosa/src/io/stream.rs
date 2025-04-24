//! Module that define stream IO that could be use by a ProSA processor
use std::{
    fmt, io,
    net::{Ipv4Addr, SocketAddrV4},
    os::fd::{AsFd, AsRawFd, BorrowedFd, RawFd},
    path::Path,
    pin::Pin,
    task::{Context, Poll},
};

use openssl::ssl::{self, SslConnector};
use prosa_utils::config::ssl::SslConfig;
use serde::{Deserialize, Serialize};
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    net::{TcpStream, ToSocketAddrs},
};
use tokio_openssl::SslStream;
use url::Url;

use super::{SocketAddr, url_is_ssl};

/// ProSA socket object to handle TCP/SSL socket with or without proxy
#[derive(Debug)]
pub enum Stream {
    #[cfg(target_family = "unix")]
    /// Unix socket (only on unix systems)
    Unix(tokio::net::UnixStream),
    /// TCP socket
    Tcp(TcpStream),
    /// SSL socket
    Ssl(SslStream<TcpStream>),
    #[cfg(feature = "http-proxy")]
    /// TCP socket using Http proxy
    TcpHttpProxy(TcpStream),
    #[cfg(feature = "http-proxy")]
    /// SSL socket using Http proxy
    SslHttpProxy(SslStream<TcpStream>),
}

impl Stream {
    /// Returns the local address that this stream is bound to.
    ///
    /// ```
    /// use tokio::io;
    /// use url::Url;
    /// use prosa::io::stream::Stream;
    /// use prosa::io::SocketAddr;
    /// use std::net::{Ipv4Addr, SocketAddrV4};
    ///
    /// async fn accepting() -> Result<(), io::Error> {
    ///     let stream: Stream = Stream::connect_tcp("127.0.0.1:80").await?;
    ///
    ///     assert_eq!(stream.local_addr()?,
    ///                SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 80)));
    ///
    ///     Ok(())
    /// }
    /// ```
    pub fn local_addr(&self) -> Result<SocketAddr, io::Error> {
        match self {
            #[cfg(target_family = "unix")]
            Stream::Unix(s) => s.local_addr().map(|addr| addr.into()),
            Stream::Tcp(s) => s.local_addr().map(|addr| addr.into()),
            Stream::Ssl(s) => s.get_ref().local_addr().map(|addr| addr.into()),
            #[cfg(feature = "http-proxy")]
            Stream::TcpHttpProxy(s) => s.local_addr().map(|addr| addr.into()),
            #[cfg(feature = "http-proxy")]
            Stream::SslHttpProxy(s) => s.get_ref().local_addr().map(|addr| addr.into()),
        }
    }

    #[cfg(target_family = "unix")]
    #[cfg_attr(doc, aquamarine::aquamarine)]
    /// Connect a UNIX socket on a path
    ///
    /// ```mermaid
    /// graph LR
    ///     client[Client]
    ///     server[Server]
    ///
    ///     client -- UNIX --> server
    /// ```
    ///
    /// ```
    /// use tokio::io;
    /// use url::Url;
    /// use prosa::io::stream::Stream;
    ///
    /// async fn connecting() -> Result<(), io::Error> {
    ///     let stream: Stream = Stream::connect_unix("/var/run/prosa.socket").await?;
    ///
    ///     // Handle the stream like any tokio stream
    ///
    ///     Ok(())
    /// }
    /// ```
    pub async fn connect_unix<P>(path: P) -> Result<Stream, io::Error>
    where
        P: AsRef<Path>,
    {
        Ok(Stream::Unix(tokio::net::UnixStream::connect(path).await?))
    }

    #[cfg_attr(doc, aquamarine::aquamarine)]
    /// Connect a TCP socket to a distant
    ///
    /// ```mermaid
    /// graph LR
    ///     client[Client]
    ///     server[Server]
    ///
    ///     client -- TCP --> server
    /// ```
    ///
    /// ```
    /// use tokio::io;
    /// use url::Url;
    /// use prosa::io::stream::Stream;
    ///
    /// async fn connecting() -> Result<(), io::Error> {
    ///     let stream: Stream = Stream::connect_tcp("worldline.com:80").await?;
    ///
    ///     // Handle the stream like any tokio stream
    ///
    ///     Ok(())
    /// }
    /// ```
    pub async fn connect_tcp<A>(addr: A) -> Result<Stream, io::Error>
    where
        A: ToSocketAddrs,
    {
        Ok(Stream::Tcp(TcpStream::connect(addr).await?))
    }

    /// Method to create an SSL stream from a TCP stream
    async fn create_ssl<S>(
        tcp_stream: S,
        ssl_connector: &ssl::SslConnector,
        domain: &str,
    ) -> Result<SslStream<S>, io::Error>
    where
        S: AsyncRead + AsyncWrite + std::marker::Unpin,
    {
        let ssl = ssl_connector.configure()?.into_ssl(domain)?;
        let mut stream = SslStream::new(ssl, tcp_stream).unwrap();
        if let Err(e) = Pin::new(&mut stream).connect().await {
            if e.code() != ssl::ErrorCode::ZERO_RETURN {
                return Err(io::Error::new(
                    io::ErrorKind::Interrupted,
                    format!("Can't connect the SSL socket `{e}`"),
                ));
            }
        }

        Ok(stream)
    }

    #[cfg_attr(doc, aquamarine::aquamarine)]
    /// Connect an SSL socket to a distant
    ///
    /// ```mermaid
    /// graph LR
    ///     client[Client]
    ///     server[Server]
    ///
    ///     client -- TCP+TLS --> server
    /// ```
    ///
    /// ```
    /// use tokio::io;
    /// use url::Url;
    /// use prosa_utils::config::ssl::SslConfig;
    /// use prosa::io::stream::Stream;
    ///
    /// async fn connecting() -> Result<(), io::Error> {
    ///     let ssl_config = SslConfig::default();
    ///     if let Ok(ssl_context_builder) = ssl_config.init_tls_client_context() {
    ///         let ssl_context = ssl_context_builder.build();
    ///         let stream: Stream = Stream::connect_ssl(&Url::parse("worldline.com:443").unwrap(), &ssl_context).await?;
    ///
    ///         // Handle the stream like any tokio stream
    ///     }
    ///
    ///     Ok(())
    /// }
    /// ```
    pub async fn connect_ssl(
        url: &Url,
        ssl_context: &ssl::SslConnector,
    ) -> Result<Stream, io::Error> {
        let addrs = url.socket_addrs(|| url.port_or_known_default())?;
        Ok(Stream::Ssl(
            Self::create_ssl(
                TcpStream::connect(&*addrs).await?,
                ssl_context,
                url.domain().ok_or(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("Can't retrieve domain name from url `{url}`"),
                ))?,
            )
            .await?,
        ))
    }

    #[cfg(feature = "http-proxy")]
    /// Method to connect a TCP stream through an HTTP proxy
    async fn connect_http_proxy(
        host: &str,
        port: u16,
        proxy: &Url,
    ) -> Result<TcpStream, io::Error> {
        let proxy_addrs = proxy.socket_addrs(|| proxy.port_or_known_default())?;
        let mut tcp_stream = TcpStream::connect(&*proxy_addrs).await?;
        if let (username, Some(password)) = (proxy.username(), proxy.password()) {
            if let Err(e) = async_http_proxy::http_connect_tokio_with_basic_auth(
                &mut tcp_stream,
                host,
                port,
                username,
                password,
            )
            .await
            {
                return Err(io::Error::new(
                    io::ErrorKind::ConnectionAborted,
                    format!("Can't connect to the http proxy with basic_auth `{e}`"),
                ));
            }
        } else if let Err(e) =
            async_http_proxy::http_connect_tokio(&mut tcp_stream, host, port).await
        {
            return Err(io::Error::new(
                io::ErrorKind::ConnectionAborted,
                format!("Can't connect to the http proxy `{e}`"),
            ));
        }

        Ok(tcp_stream)
    }

    #[cfg(feature = "http-proxy")]
    #[cfg_attr(doc, aquamarine::aquamarine)]
    /// Connect a TCP socket to a distant through an HTTP proxy
    ///
    /// ```mermaid
    /// graph LR
    ///     client[Client]
    ///     server[Server]
    ///     proxy[Proxy]
    ///
    ///     client -- TCP --> proxy
    ///     proxy --> server
    /// ```
    ///
    /// ```
    /// use tokio::io;
    /// use url::Url;
    /// use prosa::io::stream::Stream;
    ///
    /// async fn connecting() -> Result<(), io::Error> {
    ///     let proxy_url = Url::parse("http://user:pwd@proxy:3128").unwrap();
    ///     let stream: Stream = Stream::connect_tcp_with_http_proxy("worldline.com", 443, &proxy_url).await?;
    ///
    ///     // Handle the stream like any tokio stream
    ///
    ///     Ok(())
    /// }
    /// ```
    pub async fn connect_tcp_with_http_proxy(
        host: &str,
        port: u16,
        proxy: &Url,
    ) -> Result<Stream, io::Error> {
        Ok(Stream::TcpHttpProxy(
            Self::connect_http_proxy(host, port, proxy).await?,
        ))
    }

    #[cfg(feature = "http-proxy")]
    #[cfg_attr(doc, aquamarine::aquamarine)]
    /// Connect an SSL socket to a distant through an HTTP proxy
    ///
    /// ```mermaid
    /// graph LR
    ///     client[Client]
    ///     server[Server]
    ///     proxy[Proxy]
    ///
    ///     client -- TCP+TLS --> proxy
    ///     proxy --> server
    /// ```
    ///
    /// ```
    /// use tokio::io;
    /// use url::Url;
    /// use prosa_utils::config::ssl::SslConfig;
    /// use prosa::io::stream::Stream;
    ///
    /// async fn connecting() -> Result<(), io::Error> {
    ///     let proxy_url = Url::parse("http://user:pwd@proxy:3128").unwrap();
    ///     let ssl_config = SslConfig::default();
    ///     if let Ok(ssl_context_builder) = ssl_config.init_tls_client_context() {
    ///         let ssl_context = ssl_context_builder.build();
    ///         let stream: Stream = Stream::connect_ssl_with_http_proxy("worldline.com", 443, &ssl_context, &proxy_url).await?;
    ///
    ///         // Handle the stream like any tokio stream
    ///     }
    ///
    ///     Ok(())
    /// }
    /// ```
    pub async fn connect_ssl_with_http_proxy(
        host: &str,
        port: u16,
        ssl_connector: &ssl::SslConnector,
        proxy: &Url,
    ) -> Result<Stream, io::Error> {
        Ok(Stream::SslHttpProxy(
            Self::create_ssl(
                Self::connect_http_proxy(host, port, proxy).await?,
                ssl_connector,
                host,
            )
            .await?,
        ))
    }

    /// Sets the value of the TCP_NODELAY option on the ProSA socket
    pub fn set_nodelay(&self, nodelay: bool) -> Result<(), io::Error> {
        match self {
            #[cfg(target_family = "unix")]
            Stream::Unix(_) => Ok(()),
            Stream::Tcp(s) => s.set_nodelay(nodelay),
            Stream::Ssl(s) => s.get_ref().set_nodelay(nodelay),
            #[cfg(feature = "http-proxy")]
            Stream::TcpHttpProxy(s) => s.set_nodelay(nodelay),
            #[cfg(feature = "http-proxy")]
            Stream::SslHttpProxy(s) => s.get_ref().set_nodelay(nodelay),
        }
    }

    /// Gets the value of the TCP_NODELAY option for the ProSA socket
    pub fn nodelay(&self) -> Result<bool, io::Error> {
        match self {
            #[cfg(target_family = "unix")]
            Stream::Unix(_) => Ok(true),
            Stream::Tcp(s) => s.nodelay(),
            Stream::Ssl(s) => s.get_ref().nodelay(),
            #[cfg(feature = "http-proxy")]
            Stream::TcpHttpProxy(s) => s.nodelay(),
            #[cfg(feature = "http-proxy")]
            Stream::SslHttpProxy(s) => s.get_ref().nodelay(),
        }
    }

    /// Sets the value for the IP_TTL option on the ProSA socket
    pub fn set_ttl(&self, ttl: u32) -> Result<(), io::Error> {
        match self {
            #[cfg(target_family = "unix")]
            Stream::Unix(_) => Ok(()),
            Stream::Tcp(s) => s.set_ttl(ttl),
            Stream::Ssl(s) => s.get_ref().set_ttl(ttl),
            #[cfg(feature = "http-proxy")]
            Stream::TcpHttpProxy(s) => s.set_ttl(ttl),
            #[cfg(feature = "http-proxy")]
            Stream::SslHttpProxy(s) => s.get_ref().set_ttl(ttl),
        }
    }

    /// Gets the value of the IP_TTL option for the ProSA socket
    pub fn ttl(&self) -> Result<u32, io::Error> {
        match self {
            #[cfg(target_family = "unix")]
            Stream::Unix(_) => Ok(0),
            Stream::Tcp(s) => s.ttl(),
            Stream::Ssl(s) => s.get_ref().ttl(),
            #[cfg(feature = "http-proxy")]
            Stream::TcpHttpProxy(s) => s.ttl(),
            #[cfg(feature = "http-proxy")]
            Stream::SslHttpProxy(s) => s.get_ref().ttl(),
        }
    }

    /// Method to know if the stream is SSL
    pub fn is_ssl(&self) -> bool {
        match self {
            Stream::Ssl(_) => true,
            #[cfg(feature = "http-proxy")]
            Stream::SslHttpProxy(_) => true,
            _ => false,
        }
    }

    /// Method to check the protocol selected via Application Layer Protocol Negotiation (ALPN)
    ///
    /// ```
    /// use prosa::io::stream::Stream;
    ///
    /// async fn processing(stream: Stream) {
    ///     let is_http2 = stream.selected_alpn_check(|alpn| alpn == b"h2");
    ///     // is_http2 is true if the server sent the HTTP/2 ALPN value `h2`
    /// }
    /// ```
    pub fn selected_alpn_check<F>(&self, f: F) -> bool
    where
        F: Fn(&[u8]) -> bool,
    {
        match self {
            Stream::Ssl(s) => {
                if let Some(alpn) = s.ssl().selected_alpn_protocol() {
                    f(alpn)
                } else {
                    false
                }
            }
            #[cfg(feature = "http-proxy")]
            Stream::SslHttpProxy(s) => {
                if let Some(alpn) = s.ssl().selected_alpn_protocol() {
                    f(alpn)
                } else {
                    false
                }
            }
            _ => false,
        }
    }
}

impl AsFd for Stream {
    fn as_fd(&self) -> BorrowedFd<'_> {
        match self {
            #[cfg(target_family = "unix")]
            Stream::Unix(s) => s.as_fd(),
            Stream::Tcp(s) => s.as_fd(),
            Stream::Ssl(s) => s.get_ref().as_fd(),
            #[cfg(feature = "http-proxy")]
            Stream::TcpHttpProxy(s) => s.as_fd(),
            #[cfg(feature = "http-proxy")]
            Stream::SslHttpProxy(s) => s.get_ref().as_fd(),
        }
    }
}

impl AsRawFd for Stream {
    fn as_raw_fd(&self) -> RawFd {
        match self {
            #[cfg(target_family = "unix")]
            Stream::Unix(s) => s.as_raw_fd(),
            Stream::Tcp(s) => s.as_raw_fd(),
            Stream::Ssl(s) => s.get_ref().as_raw_fd(),
            #[cfg(feature = "http-proxy")]
            Stream::TcpHttpProxy(s) => s.as_raw_fd(),
            #[cfg(feature = "http-proxy")]
            Stream::SslHttpProxy(s) => s.get_ref().as_raw_fd(),
        }
    }
}

impl AsyncRead for Stream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        match self.get_mut() {
            #[cfg(target_family = "unix")]
            Stream::Unix(s) => {
                let stream = Pin::new(s);
                stream.poll_read(cx, buf)
            }
            Stream::Tcp(s) => {
                let stream = Pin::new(s);
                stream.poll_read(cx, buf)
            }
            Stream::Ssl(s) => {
                let stream = Pin::new(s);
                stream.poll_read(cx, buf)
            }
            #[cfg(feature = "http-proxy")]
            Stream::TcpHttpProxy(s) => {
                let stream = Pin::new(s);
                stream.poll_read(cx, buf)
            }
            #[cfg(feature = "http-proxy")]
            Stream::SslHttpProxy(s) => {
                let stream = Pin::new(s);
                stream.poll_read(cx, buf)
            }
        }
    }
}

impl AsyncWrite for Stream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        match self.get_mut() {
            #[cfg(target_family = "unix")]
            Stream::Unix(s) => {
                let stream = Pin::new(s);
                stream.poll_write(cx, buf)
            }
            Stream::Tcp(s) => {
                let stream = Pin::new(s);
                stream.poll_write(cx, buf)
            }
            Stream::Ssl(s) => {
                let stream = Pin::new(s);
                stream.poll_write(cx, buf)
            }
            #[cfg(feature = "http-proxy")]
            Stream::TcpHttpProxy(s) => {
                let stream = Pin::new(s);
                stream.poll_write(cx, buf)
            }
            #[cfg(feature = "http-proxy")]
            Stream::SslHttpProxy(s) => {
                let stream = Pin::new(s);
                stream.poll_write(cx, buf)
            }
        }
    }

    fn poll_write_vectored(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[io::IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        match self.get_mut() {
            #[cfg(target_family = "unix")]
            Stream::Unix(s) => {
                let stream = Pin::new(s);
                stream.poll_write_vectored(cx, bufs)
            }
            Stream::Tcp(s) => {
                let stream = Pin::new(s);
                stream.poll_write_vectored(cx, bufs)
            }
            Stream::Ssl(s) => {
                let stream = Pin::new(s);
                stream.poll_write_vectored(cx, bufs)
            }
            #[cfg(feature = "http-proxy")]
            Stream::TcpHttpProxy(s) => {
                let stream = Pin::new(s);
                stream.poll_write_vectored(cx, bufs)
            }
            #[cfg(feature = "http-proxy")]
            Stream::SslHttpProxy(s) => {
                let stream = Pin::new(s);
                stream.poll_write_vectored(cx, bufs)
            }
        }
    }

    fn is_write_vectored(&self) -> bool {
        match self {
            #[cfg(target_family = "unix")]
            Stream::Unix(s) => s.is_write_vectored(),
            Stream::Tcp(s) => s.is_write_vectored(),
            Stream::Ssl(s) => s.is_write_vectored(),
            #[cfg(feature = "http-proxy")]
            Stream::TcpHttpProxy(s) => s.is_write_vectored(),
            #[cfg(feature = "http-proxy")]
            Stream::SslHttpProxy(s) => s.is_write_vectored(),
        }
    }

    #[inline]
    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            #[cfg(target_family = "unix")]
            Stream::Unix(s) => {
                let stream = Pin::new(s);
                stream.poll_flush(cx)
            }
            Stream::Tcp(s) => {
                let stream = Pin::new(s);
                stream.poll_flush(cx)
            }
            Stream::Ssl(s) => {
                let stream = Pin::new(s);
                stream.poll_flush(cx)
            }
            #[cfg(feature = "http-proxy")]
            Stream::TcpHttpProxy(s) => {
                let stream = Pin::new(s);
                stream.poll_flush(cx)
            }
            #[cfg(feature = "http-proxy")]
            Stream::SslHttpProxy(s) => {
                let stream = Pin::new(s);
                stream.poll_flush(cx)
            }
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            #[cfg(target_family = "unix")]
            Stream::Unix(s) => {
                let stream = Pin::new(s);
                stream.poll_shutdown(cx)
            }
            Stream::Tcp(s) => {
                let stream = Pin::new(s);
                stream.poll_shutdown(cx)
            }
            Stream::Ssl(s) => {
                let stream = Pin::new(s);
                stream.poll_shutdown(cx)
            }
            #[cfg(feature = "http-proxy")]
            Stream::TcpHttpProxy(s) => {
                let stream = Pin::new(s);
                stream.poll_shutdown(cx)
            }
            #[cfg(feature = "http-proxy")]
            Stream::SslHttpProxy(s) => {
                let stream = Pin::new(s);
                stream.poll_shutdown(cx)
            }
        }
    }
}

impl fmt::Display for Stream {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let addr = self
            .local_addr()
            .unwrap_or(SocketAddr::V4(SocketAddrV4::new(
                Ipv4Addr::new(0, 0, 0, 0),
                0,
            )));
        match self {
            #[cfg(target_family = "unix")]
            Stream::Unix(_) => write!(f, "unix://{addr}"),
            Stream::Tcp(_) => write!(f, "tcp://{addr}"),
            Stream::Ssl(_) => write!(f, "ssl://{addr}"),
            #[cfg(feature = "http-proxy")]
            Stream::TcpHttpProxy(_) => write!(f, "tcp+http_proxy://{addr}"),
            #[cfg(feature = "http-proxy")]
            Stream::SslHttpProxy(_) => write!(f, "ssl+http_proxy://{addr}"),
        }
    }
}

#[cfg(target_family = "unix")]
impl From<tokio::net::UnixStream> for Stream {
    fn from(stream: tokio::net::UnixStream) -> Self {
        Stream::Unix(stream)
    }
}

impl From<TcpStream> for Stream {
    fn from(stream: TcpStream) -> Self {
        Stream::Tcp(stream)
    }
}

/// Configuration struct of an network target
///
/// ```
/// use tokio::io;
/// use url::Url;
/// use prosa::io::stream::{TargetSetting, Stream};
///
/// async fn connecting() -> Result<(), io::Error> {
///     let wl_target = TargetSetting::new(Url::parse("https://worldline.com").unwrap(), None, None);
///     let stream: Stream = wl_target.connect().await?;
///
///     // Handle the stream like any tokio stream
///
///     Ok(())
/// }
/// ```
#[derive(Deserialize, Serialize, Clone)]
pub struct TargetSetting {
    /// Url of the target destination
    pub url: Url,
    /// SSL configuration for target destination
    pub ssl: Option<SslConfig>,
    /// Optional proxy use to reach the target
    pub proxy: Option<Url>,
    #[serde(skip)]
    /// SSL configuration for target destination
    ssl_context: Option<SslConnector>,
    #[serde(skip_serializing)]
    #[serde(default = "TargetSetting::get_default_connect_timeout")]
    /// Timeout for socket connection in milliseconds
    pub connect_timeout: u32,
}

impl TargetSetting {
    fn get_default_connect_timeout() -> u32 {
        5000
    }

    /// Method to create manually a target
    pub fn new(url: Url, ssl: Option<SslConfig>, proxy: Option<Url>) -> TargetSetting {
        let mut target = TargetSetting {
            url,
            ssl,
            proxy,
            ssl_context: None,
            connect_timeout: Self::get_default_connect_timeout(),
        };

        target.init_ssl_context();
        target
    }

    /// Method to init the ssl context out of the ssl target configuration.
    /// Must be call when the configuration is retrieved
    pub fn init_ssl_context(&mut self) {
        if let Some(ssl_config) = &self.ssl {
            if let Ok(ssl_context_builder) = ssl_config.init_tls_client_context() {
                self.ssl_context = Some(ssl_context_builder.build());
            }
        }
    }

    /// Method to connect a ProSA stream to the remote target using the configuration
    pub async fn connect(&self) -> Result<Stream, io::Error> {
        #[cfg(target_family = "unix")]
        if self.url.scheme() == "unix" || self.url.scheme() == "file" {
            return Stream::connect_unix(self.url.path()).await;
        }

        let ssl_context = if self.ssl_context.is_some() {
            self.ssl_context.clone()
        } else if let Some(ssl_config) = &self.ssl {
            if let Ok(ssl_context_builder) = ssl_config.init_tls_client_context() {
                Some(ssl_context_builder.build())
            } else {
                None
            }
        } else if url_is_ssl(&self.url) {
            let ssl_config = SslConfig::default();
            if let Ok(ssl_context_builder) = ssl_config.init_tls_client_context() {
                Some(ssl_context_builder.build())
            } else {
                None
            }
        } else {
            None
        };

        if let Some(proxy_url) = &self.proxy {
            if proxy_url.scheme() == "http" {
                #[cfg(feature = "http-proxy")]
                if let Some(ssl_cx) = ssl_context {
                    return Stream::connect_ssl_with_http_proxy(
                        self.url.host_str().unwrap_or_default(),
                        self.url.port_or_known_default().unwrap_or_default(),
                        &ssl_cx,
                        proxy_url,
                    )
                    .await;
                } else {
                    return Stream::connect_tcp_with_http_proxy(
                        self.url.host_str().unwrap_or_default(),
                        self.url.port_or_known_default().unwrap_or_default(),
                        proxy_url,
                    )
                    .await;
                }

                #[cfg(not(feature = "http-proxy"))]
                return Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    "http-proxy feature is disable in ProSA",
                ));
            } else {
                return Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    format!("proxy type {}", proxy_url.scheme()),
                ));
            }
        }

        if let Some(ssl_cx) = ssl_context {
            Stream::connect_ssl(&self.url, &ssl_cx).await
        } else {
            let addrs = self.url.socket_addrs(|| self.url.port_or_known_default())?;
            Stream::connect_tcp(&*addrs).await
        }
    }
}

impl From<Url> for TargetSetting {
    fn from(url: Url) -> Self {
        TargetSetting {
            url,
            ssl: None,
            proxy: None,
            ssl_context: None,
            connect_timeout: Self::get_default_connect_timeout(),
        }
    }
}

impl fmt::Debug for TargetSetting {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut binding = f.debug_struct("TargetSetting");
        binding
            .field("url", &self.url)
            .field("ssl", &self.ssl)
            .field("connect_timeout", &self.connect_timeout);
        if let Some(proxy_url) = &self.proxy {
            binding.field("proxy", proxy_url);
        }
        binding.finish()
    }
}

impl fmt::Display for TargetSetting {
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
                let _ = url.set_scheme(format!("{url_scheme}+ssl").as_str());
            }
        }

        if let Some(proxy_url) = &self.proxy {
            write!(f, "{url} -proxy {proxy_url}")
        } else {
            write!(f, "{url}")
        }
    }
}
