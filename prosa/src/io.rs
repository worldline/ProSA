//! Module that define IO that could be use by a ProSA processor
use std::{
    fmt, io,
    os::fd::{AsFd, AsRawFd, BorrowedFd, RawFd},
    pin::Pin,
    task::{Context, Poll},
};

use openssl::ssl::{self, SslContext};
pub use prosa_macros::io;
use prosa_utils::config::ssl::SslConfig;
use serde::{Deserialize, Serialize};
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    net::{TcpStream, ToSocketAddrs},
};
use tokio_openssl::SslStream;
use url::Url;

/// ProSA socket object to handle TCP/SSL socket with or without proxy
#[derive(Debug)]
pub enum Stream {
    /// TCP socket
    Tcp(TcpStream),
    /// SSL socket
    Ssl(SslStream<TcpStream>),
    /// TCP socket using Http proxy
    TcpHttpProxy(TcpStream),
    /// SSL socket using Http proxy
    SslHttpProxy(SslStream<TcpStream>),
}

impl Stream {
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
    /// use prosa::io::Stream;
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
    async fn create_ssl(
        tcp_stream: TcpStream,
        ssl_context: &ssl::SslContext,
    ) -> Result<SslStream<TcpStream>, io::Error> {
        let ssl = ssl::Ssl::new(ssl_context).unwrap();
        let mut stream = SslStream::new(ssl, tcp_stream).unwrap();
        if let Err(e) = Pin::new(&mut stream).connect().await {
            if e.code() != ssl::ErrorCode::ZERO_RETURN {
                return Err(io::Error::new(
                    io::ErrorKind::Interrupted,
                    format!("Can't connect the SSL socket `{}`", e),
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
    /// use prosa::io::Stream;
    ///
    /// async fn connecting() -> Result<(), io::Error> {
    ///     let ssl_config = SslConfig::default();
    ///     if let Ok(ssl_context_builder) = ssl_config.init_tls_client_context() {
    ///         let ssl_context = ssl_context_builder.build();
    ///         let stream: Stream = Stream::connect_ssl("worldline.com:443", &ssl_context).await?;
    ///
    ///         // Handle the stream like any tokio stream
    ///     }
    ///
    ///     Ok(())
    /// }
    /// ```
    pub async fn connect_ssl<A>(addr: A, ssl_context: &ssl::SslContext) -> Result<Stream, io::Error>
    where
        A: ToSocketAddrs,
    {
        Ok(Stream::Ssl(
            Self::create_ssl(TcpStream::connect(addr).await?, ssl_context).await?,
        ))
    }

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
                    format!("Can't connect to the http proxy with basic_auth `{}`", e),
                ));
            }
        } else if let Err(e) =
            async_http_proxy::http_connect_tokio(&mut tcp_stream, host, port).await
        {
            return Err(io::Error::new(
                io::ErrorKind::ConnectionAborted,
                format!("Can't connect to the http proxy `{}`", e),
            ));
        }

        Ok(tcp_stream)
    }

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
    /// use prosa::io::Stream;
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
    /// use prosa::io::Stream;
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
        ssl_context: &ssl::SslContext,
        proxy: &Url,
    ) -> Result<Stream, io::Error> {
        Ok(Stream::SslHttpProxy(
            Self::create_ssl(
                Self::connect_http_proxy(host, port, proxy).await?,
                ssl_context,
            )
            .await?,
        ))
    }

    #[cfg_attr(doc, aquamarine::aquamarine)]
    /// Accept an SSL socket from a TcpListener
    ///
    /// ```mermaid
    /// graph RL
    ///     clients[Clients]
    ///     server[Server]
    ///
    ///     clients --> server
    /// ```
    ///
    /// ```
    /// use tokio::io;
    /// use tokio::net::TcpListener;
    /// use prosa_utils::config::ssl::SslConfig;
    /// use prosa::io::Stream;
    ///
    /// async fn listenning() -> Result<(), io::Error> {
    ///     let ssl_context = SslConfig::default().init_tls_server_context().unwrap().build();
    ///     let listener = TcpListener::bind("0.0.0.0:4443").await?;
    ///
    ///     loop {
    ///         let (stream, cli_addr) = listener.accept().await?;
    ///         let stream = Stream::accept_ssl(stream, &ssl_context).await?;
    ///
    ///         // Use stream ...
    ///     }
    ///
    ///     Ok(())
    /// }
    /// ```
    pub async fn accept_ssl(stream: TcpStream, context: &SslContext) -> Result<Stream, io::Error> {
        let ssl = ssl::Ssl::new(context)?;
        let mut ssl_stream = SslStream::new(ssl, stream).map_err(|e| {
            io::Error::new(
                io::ErrorKind::Other,
                format!("Can't create SslStream: {}", e),
            )
        })?;
        if let Err(e) = Pin::new(&mut ssl_stream).accept().await {
            if e.code() != ssl::ErrorCode::ZERO_RETURN {
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    format!("Can't accept the client: {}", e),
                ));
            }
        }

        Ok(Stream::Ssl(ssl_stream))
    }

    /// Sets the value of the TCP_NODELAY option on the ProSA socket
    pub fn set_nodelay(&self, nodelay: bool) -> Result<(), io::Error> {
        match self {
            Stream::Tcp(s) => s.set_nodelay(nodelay),
            Stream::Ssl(s) => s.get_ref().set_nodelay(nodelay),
            Stream::TcpHttpProxy(s) => s.set_nodelay(nodelay),
            Stream::SslHttpProxy(s) => s.get_ref().set_nodelay(nodelay),
        }
    }

    /// Gets the value of the TCP_NODELAY option for the ProSA socket
    pub fn nodelay(&self) -> Result<bool, io::Error> {
        match self {
            Stream::Tcp(s) => s.nodelay(),
            Stream::Ssl(s) => s.get_ref().nodelay(),
            Stream::TcpHttpProxy(s) => s.nodelay(),
            Stream::SslHttpProxy(s) => s.get_ref().nodelay(),
        }
    }

    /// Sets the value for the IP_TTL option on the ProSA socket
    pub fn set_ttl(&self, ttl: u32) -> Result<(), io::Error> {
        match self {
            Stream::Tcp(s) => s.set_ttl(ttl),
            Stream::Ssl(s) => s.get_ref().set_ttl(ttl),
            Stream::TcpHttpProxy(s) => s.set_ttl(ttl),
            Stream::SslHttpProxy(s) => s.get_ref().set_ttl(ttl),
        }
    }

    /// Gets the value of the IP_TTL option for the ProSA socket
    pub fn ttl(&self) -> Result<u32, io::Error> {
        match self {
            Stream::Tcp(s) => s.ttl(),
            Stream::Ssl(s) => s.get_ref().ttl(),
            Stream::TcpHttpProxy(s) => s.ttl(),
            Stream::SslHttpProxy(s) => s.get_ref().ttl(),
        }
    }
}

impl AsFd for Stream {
    fn as_fd(&self) -> BorrowedFd<'_> {
        match self {
            Stream::Tcp(s) => s.as_fd(),
            Stream::Ssl(s) => s.get_ref().as_fd(),
            Stream::TcpHttpProxy(s) => s.as_fd(),
            Stream::SslHttpProxy(s) => s.get_ref().as_fd(),
        }
    }
}

impl AsRawFd for Stream {
    fn as_raw_fd(&self) -> RawFd {
        match self {
            Stream::Tcp(s) => s.as_raw_fd(),
            Stream::Ssl(s) => s.get_ref().as_raw_fd(),
            Stream::TcpHttpProxy(s) => s.as_raw_fd(),
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
            Stream::Tcp(s) => {
                let stream = Pin::new(s);
                stream.poll_read(cx, buf)
            }
            Stream::Ssl(s) => {
                let stream = Pin::new(s);
                stream.poll_read(cx, buf)
            }
            Stream::TcpHttpProxy(s) => {
                let stream = Pin::new(s);
                stream.poll_read(cx, buf)
            }
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
            Stream::Tcp(s) => {
                let stream = Pin::new(s);
                stream.poll_write(cx, buf)
            }
            Stream::Ssl(s) => {
                let stream = Pin::new(s);
                stream.poll_write(cx, buf)
            }
            Stream::TcpHttpProxy(s) => {
                let stream = Pin::new(s);
                stream.poll_write(cx, buf)
            }
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
            Stream::Tcp(s) => {
                let stream = Pin::new(s);
                stream.poll_write_vectored(cx, bufs)
            }
            Stream::Ssl(s) => {
                let stream = Pin::new(s);
                stream.poll_write_vectored(cx, bufs)
            }
            Stream::TcpHttpProxy(s) => {
                let stream = Pin::new(s);
                stream.poll_write_vectored(cx, bufs)
            }
            Stream::SslHttpProxy(s) => {
                let stream = Pin::new(s);
                stream.poll_write_vectored(cx, bufs)
            }
        }
    }

    fn is_write_vectored(&self) -> bool {
        match self {
            Stream::Tcp(s) => s.is_write_vectored(),
            Stream::Ssl(s) => s.is_write_vectored(),
            Stream::TcpHttpProxy(s) => s.is_write_vectored(),
            Stream::SslHttpProxy(s) => s.is_write_vectored(),
        }
    }

    #[inline]
    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Stream::Tcp(s) => {
                let stream = Pin::new(s);
                stream.poll_flush(cx)
            }
            Stream::Ssl(s) => {
                let stream = Pin::new(s);
                stream.poll_flush(cx)
            }
            Stream::TcpHttpProxy(s) => {
                let stream = Pin::new(s);
                stream.poll_flush(cx)
            }
            Stream::SslHttpProxy(s) => {
                let stream = Pin::new(s);
                stream.poll_flush(cx)
            }
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Stream::Tcp(s) => {
                let stream = Pin::new(s);
                stream.poll_shutdown(cx)
            }
            Stream::Ssl(s) => {
                let stream = Pin::new(s);
                stream.poll_shutdown(cx)
            }
            Stream::TcpHttpProxy(s) => {
                let stream = Pin::new(s);
                stream.poll_shutdown(cx)
            }
            Stream::SslHttpProxy(s) => {
                let stream = Pin::new(s);
                stream.poll_shutdown(cx)
            }
        }
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
/// use prosa::io::{TargetSetting, Stream};
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
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct TargetSetting {
    /// Url of the target destination
    pub url: Url,
    /// SSL configuration for target destination
    pub ssl: Option<SslConfig>,
    /// Optional proxy use to reach the target
    pub proxy: Option<Url>,
    #[serde(skip)]
    /// SSL configuration for target destination
    pub ssl_context: Option<SslContext>,
}

impl TargetSetting {
    /// Method to create manually a target
    pub fn new(url: Url, ssl: Option<SslConfig>, proxy: Option<Url>) -> TargetSetting {
        let mut target = TargetSetting {
            url,
            ssl,
            proxy,
            ssl_context: None,
        };

        target.init_ssl_context();
        target
    }

    /// Method to known if the url indicate an SSL protocol
    pub fn url_is_ssl(url: &Url) -> bool {
        let scheme = url.scheme();
        if scheme.ends_with("+ssl") || scheme.ends_with("+tls") {
            true
        } else {
            matches!(url.scheme(), "ssl" | "tls" | "https")
        }
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
        let ssl_context = if self.ssl_context.is_some() {
            self.ssl_context.clone()
        } else if let Some(ssl_config) = &self.ssl {
            if let Ok(ssl_context_builder) = ssl_config.init_tls_client_context() {
                Some(ssl_context_builder.build())
            } else {
                None
            }
        } else if Self::url_is_ssl(&self.url) {
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
            if let Some(ssl_cx) = ssl_context {
                Stream::connect_ssl_with_http_proxy(
                    self.url.host_str().unwrap_or_default(),
                    self.url.port_or_known_default().unwrap_or_default(),
                    &ssl_cx,
                    proxy_url,
                )
                .await
            } else {
                Stream::connect_tcp_with_http_proxy(
                    self.url.host_str().unwrap_or_default(),
                    self.url.port_or_known_default().unwrap_or_default(),
                    proxy_url,
                )
                .await
            }
        } else {
            let addrs = self.url.socket_addrs(|| self.url.port_or_known_default())?;
            if let Some(ssl_cx) = ssl_context {
                Stream::connect_ssl(&*addrs, &ssl_cx).await
            } else {
                Stream::connect_tcp(&*addrs).await
            }
        }
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
                let _ = url.set_scheme(format!("{}+ssl", url_scheme).as_str());
            }
        }

        if let Some(proxy_url) = &self.proxy {
            writeln!(f, "{} -proxy {}", url, proxy_url)
        } else {
            writeln!(f, "{}", url)
        }
    }
}

/// Trait to define ProSA IO.
/// Implement with the procedural macro io
pub trait IO {
    /// Frame error trigger when the frame operation can't be executed
    type Error;

    /// Method call to parse a frame
    fn parse_frame<F>(&mut self) -> std::result::Result<Option<F>, Self::Error>;

    /// Method to wait a complete frame
    fn read_frame<F>(
        &mut self,
    ) -> impl std::future::Future<Output = Result<Option<F>, Self::Error>> + Send;
    /// Method to write a frame and wait for completion
    fn write_frame<F>(
        &mut self,
        frame: F,
    ) -> impl std::future::Future<Output = Result<(), Self::Error>> + Send;
}
