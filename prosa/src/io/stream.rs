//! Module that define stream IO that could be use by a ProSA processor
use std::{
    fmt, io,
    net::{Ipv4Addr, SocketAddrV4},
    os::fd::{AsFd, AsRawFd, BorrowedFd, RawFd},
    path::Path,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

#[cfg(feature = "openssl")]
use prosa_utils::config::ssl::SslConfigContext;
use prosa_utils::config::{ssl::SslConfig, url_authentication};

use serde::{Deserialize, Serialize};
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    net::{TcpStream, ToSocketAddrs},
    time::timeout,
};
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
    #[cfg(feature = "openssl")]
    /// SSL socket
    OpenSsl(tokio_openssl::SslStream<TcpStream>),
    #[cfg(feature = "http-proxy")]
    /// TCP socket using Http proxy
    TcpHttpProxy(TcpStream),
    #[cfg(all(feature = "openssl", feature = "http-proxy"))]
    /// SSL socket using Http proxy
    OpenSslHttpProxy(tokio_openssl::SslStream<TcpStream>),
}

impl Stream {
    /// Returns the socket address of the remote peer of this TCP connection.
    ///
    /// ```
    /// use tokio::io;
    /// use url::Url;
    /// use prosa::io::stream::Stream;
    /// use prosa::io::SocketAddr;
    /// use std::net::{Ipv4Addr, SocketAddrV4};
    ///
    /// async fn accepting() -> Result<(), io::Error> {
    ///     let stream: Stream = Stream::connect_tcp("127.0.0.1:8080").await?;
    ///
    ///     assert_eq!(stream.peer_addr()?,
    ///                SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 8080)));
    ///
    ///     Ok(())
    /// }
    /// ```
    pub fn peer_addr(&self) -> Result<SocketAddr, io::Error> {
        match self {
            #[cfg(target_family = "unix")]
            Stream::Unix(s) => s.peer_addr().map(|addr| addr.into()),
            Stream::Tcp(s) => s.peer_addr().map(|addr| addr.into()),
            #[cfg(feature = "openssl")]
            Stream::OpenSsl(s) => s.get_ref().peer_addr().map(|addr| addr.into()),
            #[cfg(feature = "http-proxy")]
            Stream::TcpHttpProxy(s) => s.peer_addr().map(|addr| addr.into()),
            #[cfg(all(feature = "openssl", feature = "http-proxy"))]
            Stream::OpenSslHttpProxy(s) => s.get_ref().peer_addr().map(|addr| addr.into()),
        }
    }

    /// Returns the local address that this stream is bound to.
    ///
    /// ```
    /// use tokio::io;
    /// use url::Url;
    /// use prosa::io::stream::Stream;
    /// use prosa::io::SocketAddr;
    /// use std::net::{IpAddr, Ipv4Addr, SocketAddrV4};
    ///
    /// async fn accepting() -> Result<(), io::Error> {
    ///     let stream: Stream = Stream::connect_tcp("127.0.0.1:8080").await?;
    ///
    ///     assert_eq!(stream.local_addr()?.ip(), IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)));
    ///
    ///     Ok(())
    /// }
    /// ```
    pub fn local_addr(&self) -> Result<SocketAddr, io::Error> {
        match self {
            #[cfg(target_family = "unix")]
            Stream::Unix(s) => s.local_addr().map(|addr| addr.into()),
            Stream::Tcp(s) => s.local_addr().map(|addr| addr.into()),
            #[cfg(feature = "openssl")]
            Stream::OpenSsl(s) => s.get_ref().local_addr().map(|addr| addr.into()),
            #[cfg(feature = "http-proxy")]
            Stream::TcpHttpProxy(s) => s.local_addr().map(|addr| addr.into()),
            #[cfg(all(feature = "openssl", feature = "http-proxy"))]
            Stream::OpenSslHttpProxy(s) => s.get_ref().local_addr().map(|addr| addr.into()),
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

    #[cfg(feature = "openssl")]
    /// Method to create an SSL stream from a TCP stream
    async fn create_openssl<S>(
        tcp_stream: S,
        ssl_connector: &openssl::ssl::SslConnector,
        domain: &str,
    ) -> Result<tokio_openssl::SslStream<S>, io::Error>
    where
        S: AsyncRead + AsyncWrite + std::marker::Unpin,
    {
        let ssl = ssl_connector.configure()?.into_ssl(domain)?;
        let mut stream = tokio_openssl::SslStream::new(ssl, tcp_stream).unwrap();
        if let Err(e) = Pin::new(&mut stream).connect().await
            && e.code() != openssl::ssl::ErrorCode::ZERO_RETURN
        {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                format!("Can't connect the SSL socket `{e}`"),
            ));
        }

        Ok(stream)
    }

    #[cfg(feature = "openssl")]
    #[cfg_attr(doc, aquamarine::aquamarine)]
    /// Connect an OpenSSL socket to a distant
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
    /// use prosa_utils::config::ssl::{SslConfig, SslConfigContext};
    /// use prosa::io::stream::Stream;
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
        ssl_context: &openssl::ssl::SslConnector,
    ) -> Result<Stream, io::Error> {
        let addrs = url.socket_addrs(|| url.port_or_known_default())?;
        Ok(Stream::OpenSsl(
            Self::create_openssl(
                TcpStream::connect(&*addrs).await?,
                ssl_context,
                url.host_str().ok_or(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("Can't retrieve host from url `{url}` for ssl"),
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

    #[cfg(all(feature = "openssl", feature = "http-proxy"))]
    #[cfg_attr(doc, aquamarine::aquamarine)]
    /// Connect an OpenSSL socket to a distant through an HTTP proxy
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
    /// use prosa_utils::config::ssl::{SslConfig, SslConfigContext};
    /// use prosa::io::stream::Stream;
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
    pub async fn connect_openssl_with_http_proxy(
        host: &str,
        port: u16,
        ssl_connector: &openssl::ssl::SslConnector,
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

    /// Sets the value of the TCP_NODELAY option on the ProSA socket
    pub fn set_nodelay(&self, nodelay: bool) -> Result<(), io::Error> {
        match self {
            #[cfg(target_family = "unix")]
            Stream::Unix(_) => Ok(()),
            Stream::Tcp(s) => s.set_nodelay(nodelay),
            #[cfg(feature = "openssl")]
            Stream::OpenSsl(s) => s.get_ref().set_nodelay(nodelay),
            #[cfg(feature = "http-proxy")]
            Stream::TcpHttpProxy(s) => s.set_nodelay(nodelay),
            #[cfg(all(feature = "openssl", feature = "http-proxy"))]
            Stream::OpenSslHttpProxy(s) => s.get_ref().set_nodelay(nodelay),
        }
    }

    /// Gets the value of the TCP_NODELAY option for the ProSA socket
    pub fn nodelay(&self) -> Result<bool, io::Error> {
        match self {
            #[cfg(target_family = "unix")]
            Stream::Unix(_) => Ok(true),
            Stream::Tcp(s) => s.nodelay(),
            #[cfg(feature = "openssl")]
            Stream::OpenSsl(s) => s.get_ref().nodelay(),
            #[cfg(feature = "http-proxy")]
            Stream::TcpHttpProxy(s) => s.nodelay(),
            #[cfg(all(feature = "openssl", feature = "http-proxy"))]
            Stream::OpenSslHttpProxy(s) => s.get_ref().nodelay(),
        }
    }

    /// Sets the value for the IP_TTL option on the ProSA socket
    pub fn set_ttl(&self, ttl: u32) -> Result<(), io::Error> {
        match self {
            #[cfg(target_family = "unix")]
            Stream::Unix(_) => Ok(()),
            Stream::Tcp(s) => s.set_ttl(ttl),
            #[cfg(feature = "openssl")]
            Stream::OpenSsl(s) => s.get_ref().set_ttl(ttl),
            #[cfg(feature = "http-proxy")]
            Stream::TcpHttpProxy(s) => s.set_ttl(ttl),
            #[cfg(all(feature = "openssl", feature = "http-proxy"))]
            Stream::OpenSslHttpProxy(s) => s.get_ref().set_ttl(ttl),
        }
    }

    /// Gets the value of the IP_TTL option for the ProSA socket
    pub fn ttl(&self) -> Result<u32, io::Error> {
        match self {
            #[cfg(target_family = "unix")]
            Stream::Unix(_) => Ok(0),
            Stream::Tcp(s) => s.ttl(),
            #[cfg(feature = "openssl")]
            Stream::OpenSsl(s) => s.get_ref().ttl(),
            #[cfg(feature = "http-proxy")]
            Stream::TcpHttpProxy(s) => s.ttl(),
            #[cfg(all(feature = "openssl", feature = "http-proxy"))]
            Stream::OpenSslHttpProxy(s) => s.get_ref().ttl(),
        }
    }

    /// Method to know if the stream is SSL
    pub fn is_ssl(&self) -> bool {
        match self {
            #[cfg(feature = "openssl")]
            Stream::OpenSsl(_) => true,
            #[cfg(all(feature = "openssl", feature = "http-proxy"))]
            Stream::OpenSslHttpProxy(_) => true,
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
    pub fn selected_alpn_check<F>(&self, _f: F) -> bool
    where
        F: Fn(&[u8]) -> bool,
    {
        match self {
            #[cfg(feature = "openssl")]
            Stream::OpenSsl(s) => {
                if let Some(alpn) = s.ssl().selected_alpn_protocol() {
                    _f(alpn)
                } else {
                    false
                }
            }
            #[cfg(all(feature = "openssl", feature = "http-proxy"))]
            Stream::OpenSslHttpProxy(s) => {
                if let Some(alpn) = s.ssl().selected_alpn_protocol() {
                    _f(alpn)
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
            #[cfg(feature = "openssl")]
            Stream::OpenSsl(s) => s.get_ref().as_fd(),
            #[cfg(feature = "http-proxy")]
            Stream::TcpHttpProxy(s) => s.as_fd(),
            #[cfg(all(feature = "openssl", feature = "http-proxy"))]
            Stream::OpenSslHttpProxy(s) => s.get_ref().as_fd(),
        }
    }
}

impl AsRawFd for Stream {
    fn as_raw_fd(&self) -> RawFd {
        match self {
            #[cfg(target_family = "unix")]
            Stream::Unix(s) => s.as_raw_fd(),
            Stream::Tcp(s) => s.as_raw_fd(),
            #[cfg(feature = "openssl")]
            Stream::OpenSsl(s) => s.get_ref().as_raw_fd(),
            #[cfg(feature = "http-proxy")]
            Stream::TcpHttpProxy(s) => s.as_raw_fd(),
            #[cfg(all(feature = "openssl", feature = "http-proxy"))]
            Stream::OpenSslHttpProxy(s) => s.get_ref().as_raw_fd(),
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
            #[cfg(feature = "openssl")]
            Stream::OpenSsl(s) => {
                let stream = Pin::new(s);
                stream.poll_read(cx, buf)
            }
            #[cfg(feature = "http-proxy")]
            Stream::TcpHttpProxy(s) => {
                let stream = Pin::new(s);
                stream.poll_read(cx, buf)
            }
            #[cfg(all(feature = "openssl", feature = "http-proxy"))]
            Stream::OpenSslHttpProxy(s) => {
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
            #[cfg(feature = "openssl")]
            Stream::OpenSsl(s) => {
                let stream = Pin::new(s);
                stream.poll_write(cx, buf)
            }
            #[cfg(feature = "http-proxy")]
            Stream::TcpHttpProxy(s) => {
                let stream = Pin::new(s);
                stream.poll_write(cx, buf)
            }
            #[cfg(all(feature = "openssl", feature = "http-proxy"))]
            Stream::OpenSslHttpProxy(s) => {
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
            #[cfg(feature = "openssl")]
            Stream::OpenSsl(s) => {
                let stream = Pin::new(s);
                stream.poll_write_vectored(cx, bufs)
            }
            #[cfg(feature = "http-proxy")]
            Stream::TcpHttpProxy(s) => {
                let stream = Pin::new(s);
                stream.poll_write_vectored(cx, bufs)
            }
            #[cfg(all(feature = "openssl", feature = "http-proxy"))]
            Stream::OpenSslHttpProxy(s) => {
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
            #[cfg(feature = "openssl")]
            Stream::OpenSsl(s) => s.is_write_vectored(),
            #[cfg(feature = "http-proxy")]
            Stream::TcpHttpProxy(s) => s.is_write_vectored(),
            #[cfg(all(feature = "openssl", feature = "http-proxy"))]
            Stream::OpenSslHttpProxy(s) => s.is_write_vectored(),
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
            #[cfg(feature = "openssl")]
            Stream::OpenSsl(s) => {
                let stream = Pin::new(s);
                stream.poll_flush(cx)
            }
            #[cfg(feature = "http-proxy")]
            Stream::TcpHttpProxy(s) => {
                let stream = Pin::new(s);
                stream.poll_flush(cx)
            }
            #[cfg(all(feature = "openssl", feature = "http-proxy"))]
            Stream::OpenSslHttpProxy(s) => {
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
            #[cfg(feature = "openssl")]
            Stream::OpenSsl(s) => {
                let stream = Pin::new(s);
                stream.poll_shutdown(cx)
            }
            #[cfg(feature = "http-proxy")]
            Stream::TcpHttpProxy(s) => {
                let stream = Pin::new(s);
                stream.poll_shutdown(cx)
            }
            #[cfg(all(feature = "openssl", feature = "http-proxy"))]
            Stream::OpenSslHttpProxy(s) => {
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
            #[cfg(feature = "openssl")]
            Stream::OpenSsl(_) => write!(f, "ssl://{addr}"),
            #[cfg(feature = "http-proxy")]
            Stream::TcpHttpProxy(_) => write!(f, "tcp+http_proxy://{addr}"),
            #[cfg(all(feature = "openssl", feature = "http-proxy"))]
            Stream::OpenSslHttpProxy(_) => write!(f, "ssl+http_proxy://{addr}"),
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

#[cfg(feature = "openssl")]
impl From<tokio_openssl::SslStream<TcpStream>> for Stream {
    fn from(openssl_stream: tokio_openssl::SslStream<TcpStream>) -> Self {
        Stream::OpenSsl(openssl_stream)
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
    #[cfg(feature = "openssl")]
    #[serde(skip)]
    /// OpenSSL configuration for target destination
    openssl_context: Option<::openssl::ssl::SslConnector>,
    #[serde(skip_serializing)]
    #[serde(default = "TargetSetting::get_default_connect_timeout")]
    /// Timeout for socket connection in milliseconds
    pub connect_timeout: u64,
}

impl TargetSetting {
    fn get_default_connect_timeout() -> u64 {
        5000
    }

    /// Method to create manually a target
    pub fn new(url: Url, ssl: Option<SslConfig>, proxy: Option<Url>) -> TargetSetting {
        let mut target = TargetSetting {
            url,
            ssl,
            proxy,
            #[cfg(feature = "openssl")]
            openssl_context: None,
            connect_timeout: Self::get_default_connect_timeout(),
        };

        target.init_ssl_context();
        target
    }

    /// Method to know if the target will be connected with SSL
    pub fn is_ssl(&self) -> bool {
        #[cfg(feature = "openssl")]
        if self.openssl_context.is_some() {
            return true;
        }

        self.ssl.is_some() || url_is_ssl(&self.url)
    }

    /// Getter of the URL with masked inner credential
    pub fn get_safe_url(&self) -> Url {
        let mut url = self.url.clone();
        url.set_query(None);
        if !url.username().is_empty() {
            let _ = url.set_username("***");
        }
        if url.password().is_some() {
            let _ = url.set_password(Some("***"));
        }

        url
    }

    /// Method to get authentication value out of URL username/password
    ///
    /// - If user password is provided, it return *Basic* authentication with base64 encoded username:password
    /// - If only password is provided, it return *Bearer* authentication with the password as token
    ///
    /// ```
    /// use url::Url;
    /// use prosa::io::stream::TargetSetting;
    ///
    /// let basic_auth_target = TargetSetting::from(Url::parse("http://user:pass@localhost:8080").unwrap());
    /// assert_eq!(Some(String::from("Basic dXNlcjpwYXNz")), basic_auth_target.get_authentication());
    ///
    /// let bearer_auth_target = TargetSetting::from(Url::parse("http://:token@localhost:8080").unwrap());
    /// assert_eq!(Some(String::from("Bearer token")), bearer_auth_target.get_authentication());
    /// ```
    pub fn get_authentication(&self) -> Option<String> {
        url_authentication(&self.url)
    }

    /// Method to init the ssl context out of the ssl target configuration.
    /// Must be call when the configuration is retrieved
    pub fn init_ssl_context(&mut self) {
        #[cfg(feature = "openssl")]
        if let Some(ssl_config) = self.ssl.as_ref() {
            // Init OpenSSL context by default
            let ssl_context_builder: Option<openssl::ssl::SslConnectorBuilder> =
                SslConfigContext::init_tls_client_context(ssl_config).ok();
            self.openssl_context = ssl_context_builder.map(|c| c.build());
        }
    }

    /// Method to connect a ProSA stream to the remote target using the configuration
    pub async fn connect(&self) -> Result<Stream, io::Error> {
        #[cfg(target_family = "unix")]
        if self.url.scheme() == "unix" || self.url.scheme() == "file" {
            return timeout(
                Duration::from_millis(self.connect_timeout),
                Stream::connect_unix(self.url.path()),
            )
            .await
            .map_err(|e| {
                io::Error::new(
                    io::ErrorKind::TimedOut,
                    format!("unix timeout after {e} for {}", self.get_safe_url()),
                )
            })?;
        }

        #[cfg(feature = "openssl")]
        let openssl_context = if self.openssl_context.is_some() {
            self.openssl_context.clone()
        } else if let Some(ssl_config) = &self.ssl {
            let ssl_context_builder: Option<openssl::ssl::SslConnectorBuilder> =
                SslConfigContext::init_tls_client_context(ssl_config).ok();
            ssl_context_builder.map(|c| c.build())
        } else if url_is_ssl(&self.url) {
            let ssl_config = SslConfig::default();
            let ssl_context_builder: Option<openssl::ssl::SslConnectorBuilder> =
                SslConfigContext::init_tls_client_context(&ssl_config).ok();
            ssl_context_builder.map(|c| c.build())
        } else {
            None
        };

        if let Some(proxy_url) = &self.proxy {
            if proxy_url.scheme() == "http" {
                #[cfg(feature = "http-proxy")]
                {
                    #[cfg(feature = "openssl")]
                    if let Some(ssl_cx) = openssl_context {
                        return timeout(
                            Duration::from_millis(self.connect_timeout),
                            Stream::connect_openssl_with_http_proxy(
                                self.url.host_str().unwrap_or_default(),
                                self.url.port_or_known_default().unwrap_or_default(),
                                &ssl_cx,
                                proxy_url,
                            ),
                        )
                        .await
                        .map_err(|e| {
                            io::Error::new(
                                io::ErrorKind::TimedOut,
                                format!(
                                    "openssl with proxy timeout after {e} for {}",
                                    self.get_safe_url()
                                ),
                            )
                        })?;
                    }

                    return timeout(
                        Duration::from_millis(self.connect_timeout),
                        Stream::connect_tcp_with_http_proxy(
                            self.url.host_str().unwrap_or_default(),
                            self.url.port_or_known_default().unwrap_or_default(),
                            proxy_url,
                        ),
                    )
                    .await
                    .map_err(|e| {
                        io::Error::new(
                            io::ErrorKind::TimedOut,
                            format!(
                                "tcp with proxy timeout after {e} for {}",
                                self.get_safe_url()
                            ),
                        )
                    })?;
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

        #[cfg(feature = "openssl")]
        if let Some(ssl_cx) = openssl_context {
            return timeout(
                Duration::from_millis(self.connect_timeout),
                Stream::connect_openssl(&self.url, &ssl_cx),
            )
            .await
            .map_err(|e| {
                io::Error::new(
                    io::ErrorKind::TimedOut,
                    format!("openssl timeout after {e} for {}", self.get_safe_url()),
                )
            })?;
        }

        let addrs = self.url.socket_addrs(|| self.url.port_or_known_default())?;
        timeout(
            Duration::from_millis(self.connect_timeout),
            Stream::connect_tcp(&*addrs),
        )
        .await
        .map_err(|e| {
            io::Error::new(
                io::ErrorKind::TimedOut,
                format!("tcp timeout after {e} for {}", self.get_safe_url()),
            )
        })?
    }
}

impl From<Url> for TargetSetting {
    fn from(url: Url) -> Self {
        TargetSetting {
            url,
            ssl: None,
            proxy: None,
            #[cfg(feature = "openssl")]
            openssl_context: None,
            connect_timeout: Self::get_default_connect_timeout(),
        }
    }
}

impl fmt::Debug for TargetSetting {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut binding = f.debug_struct("TargetSetting");
        binding
            .field("url", &self.get_safe_url())
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
        let mut url = self.get_safe_url();
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

        if f.alternate() {
            if let Some(proxy_url) = &self.proxy {
                write!(f, "{url} -proxy {proxy_url}")
            } else {
                write!(f, "{url}")
            }
        } else {
            // Remove username and password for more visibility
            let _ = url.set_username("");
            let _ = url.set_password(None);
            write!(f, "{url}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_settings_test() {
        let target_without_credential = TargetSetting::new(
            Url::parse("https://localhost:4443/v1?var=1").unwrap(),
            None,
            None,
        );
        assert_eq!(
            "https://localhost:4443/v1",
            target_without_credential.to_string()
        );
        assert_eq!(
            "https://localhost:4443/v1",
            format!("{target_without_credential:#}")
        );

        let target_with_user_password = TargetSetting::new(
            Url::parse("https://admin:admin@localhost:4443/v1?user=admin&password=admin").unwrap(),
            None,
            None,
        );
        assert_eq!(
            "https://localhost:4443/v1",
            target_with_user_password.to_string()
        );
        assert_eq!(
            "https://***:***@localhost:4443/v1",
            format!("{target_with_user_password:#}")
        );
        assert_eq!(
            "TargetSetting { url: Url { scheme: \"https\", cannot_be_a_base: false, username: \"***\", password: Some(\"***\"), host: Some(Domain(\"localhost\")), port: Some(4443), path: \"/v1\", query: None, fragment: None }, ssl: None, connect_timeout: 5000 }",
            format!("{target_with_user_password:?}")
        );

        let target_with_token = TargetSetting::new(
            Url::parse("https://:token@localhost:4443/v1").unwrap(),
            None,
            None,
        );
        assert_eq!("https://localhost:4443/v1", target_with_token.to_string());
        assert_eq!(
            "https://:***@localhost:4443/v1",
            format!("{target_with_token:#}")
        );
        assert_eq!(
            "TargetSetting { url: Url { scheme: \"https\", cannot_be_a_base: false, username: \"\", password: Some(\"***\"), host: Some(Domain(\"localhost\")), port: Some(4443), path: \"/v1\", query: None, fragment: None }, ssl: None, connect_timeout: 5000 }",
            format!("{target_with_token:?}")
        );
    }
}
