//! Module that define IO that could be use by a ProSA processor
use std::{
    fmt,
    hash::{Hash, Hasher},
    net::{SocketAddrV4, SocketAddrV6},
    path::Path,
};

use url::Url;

pub use prosa_macros::io;
pub use prosa_utils::config::ssl::SslConfig;

pub mod listener;
pub mod stream;

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

/// Method to known if the url indicate an SSL protocol
///
/// ```
/// use url::Url;
/// use prosa::io::url_is_ssl;
///
/// assert!(!url_is_ssl(&Url::parse("http://localhost").unwrap()));
/// assert!(url_is_ssl(&Url::parse("https://localhost").unwrap()));
/// ```
pub fn url_is_ssl(url: &Url) -> bool {
    let scheme = url.scheme();
    if scheme.ends_with("+ssl") || scheme.ends_with("+tls") {
        true
    } else {
        matches!(url.scheme(), "ssl" | "tls" | "https" | "wss")
    }
}

/// Internal Socket adress enum to define IPv4, IPv6 and unix socket.
#[derive(Debug, Clone)]
pub enum SocketAddr {
    #[cfg(target_family = "unix")]
    /// UNIX socket address
    Unix(tokio::net::unix::SocketAddr),
    /// IPv4 address
    V4(SocketAddrV4),
    /// IPv6 address
    V6(SocketAddrV6),
}

impl SocketAddr {
    /// Returns true if this is a loopback address (IPv4: 127.0.0.0/8, IPv6: ::1).
    /// These properties are defined by [IETF RFC 1122](https://tools.ietf.org/html/rfc1122), and [IETF RFC 4291 section 2.5.3](https://tools.ietf.org/html/rfc4291#section-2.5.3).
    pub fn is_loopback(&self) -> bool {
        match self {
            #[cfg(target_family = "unix")]
            SocketAddr::Unix(_) => true,
            SocketAddr::V4(ipv4) => ipv4.ip().is_loopback(),
            SocketAddr::V6(ipv6) => ipv6.ip().is_loopback(),
        }
    }

    /// Returns `true` if the adress is a UNIX address, and `false` otherwise.
    pub fn is_unix(&self) -> bool {
        #[cfg(target_family = "unix")]
        {
            matches!(self, SocketAddr::Unix(_))
        }
        #[cfg(not(target_family = "unix"))]
        {
            false
        }
    }

    /// Returns `true` if the adress is an IPV4 address, and `false` otherwise.
    pub fn is_ipv4(&self) -> bool {
        matches!(self, SocketAddr::V4(_))
    }

    /// Returns `true` if the adress is an IPV6 address, and `false` otherwise.
    pub fn is_ipv6(&self) -> bool {
        matches!(self, SocketAddr::V6(_))
    }

    /// Returns the IP address associated with this socket address.
    pub fn ip(&self) -> std::net::IpAddr {
        match self {
            #[cfg(target_family = "unix")]
            SocketAddr::Unix(_) => std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED),
            SocketAddr::V4(ipv4) => std::net::IpAddr::V4(*ipv4.ip()),
            SocketAddr::V6(ipv6) => std::net::IpAddr::V6(*ipv6.ip()),
        }
    }

    /// Changes the IP address associated with this socket address.
    pub const fn set_ip(&mut self, new_ip: std::net::IpAddr) {
        match new_ip {
            std::net::IpAddr::V4(ipv4_addr) => {
                *self = SocketAddr::V4(SocketAddrV4::new(ipv4_addr, self.port()))
            }
            std::net::IpAddr::V6(ipv6_addr) => {
                *self = SocketAddr::V6(SocketAddrV6::new(ipv6_addr, self.port(), 0, 0))
            }
        }
    }

    /// Returns the port number associated with this socket address.
    pub const fn port(&self) -> u16 {
        match self {
            #[cfg(target_family = "unix")]
            SocketAddr::Unix(_) => 0u16,
            SocketAddr::V4(ipv4) => ipv4.port(),
            SocketAddr::V6(ipv6) => ipv6.port(),
        }
    }

    /// Changes the port number associated with this socket address.
    pub fn set_port(&mut self, port: u16) {
        match self {
            #[cfg(target_family = "unix")]
            SocketAddr::Unix(_) => {}
            SocketAddr::V4(ipv4) => ipv4.set_port(port),
            SocketAddr::V6(ipv6) => ipv6.set_port(port),
        }
    }
}

impl PartialEq for SocketAddr {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            #[cfg(target_family = "unix")]
            (SocketAddr::Unix(s), SocketAddr::Unix(o)) => s.as_pathname() == o.as_pathname(),
            (SocketAddr::V4(s), SocketAddr::V4(o)) => s == o,
            (SocketAddr::V6(s), SocketAddr::V6(o)) => s == o,
            _ => false,
        }
    }
}

impl Hash for SocketAddr {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            #[cfg(target_family = "unix")]
            SocketAddr::Unix(unix) => unix.as_pathname().hash(state),
            SocketAddr::V4(ipv4) => ipv4.hash(state),
            SocketAddr::V6(ipv6) => ipv6.hash(state),
        }
    }
}

impl fmt::Display for SocketAddr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            #[cfg(target_family = "unix")]
            SocketAddr::Unix(path) => write!(
                f,
                "{}",
                path.as_pathname()
                    .unwrap_or(Path::new("undefined"))
                    .display()
            ),
            SocketAddr::V4(ipv4) => write!(f, "{ipv4}"),
            SocketAddr::V6(ipv6) => write!(f, "{ipv6}"),
        }
    }
}

impl<I: Into<std::net::IpAddr>> From<(I, u16)> for SocketAddr {
    fn from(pieces: (I, u16)) -> Self {
        match pieces.0.into() {
            std::net::IpAddr::V4(ipv4) => SocketAddr::V4(SocketAddrV4::new(ipv4, pieces.1)),
            std::net::IpAddr::V6(ipv6) => SocketAddr::V6(SocketAddrV6::new(ipv6, pieces.1, 0, 0)),
        }
    }
}

impl From<std::net::SocketAddrV4> for SocketAddr {
    fn from(ipv4: std::net::SocketAddrV4) -> Self {
        SocketAddr::V4(ipv4)
    }
}

impl From<std::net::SocketAddrV6> for SocketAddr {
    fn from(ipv6: std::net::SocketAddrV6) -> Self {
        SocketAddr::V6(ipv6)
    }
}

impl From<std::net::SocketAddr> for SocketAddr {
    fn from(addr: std::net::SocketAddr) -> Self {
        match addr {
            std::net::SocketAddr::V4(ipv4) => SocketAddr::V4(ipv4),
            std::net::SocketAddr::V6(ipv6) => SocketAddr::V6(ipv6),
        }
    }
}

#[cfg(target_family = "unix")]
impl From<tokio::net::unix::SocketAddr> for SocketAddr {
    fn from(addr: tokio::net::unix::SocketAddr) -> Self {
        SocketAddr::Unix(addr)
    }
}

#[cfg(test)]
mod tests {
    use futures_util::future;
    use listener::StreamListener;

    #[cfg(feature = "openssl")]
    use prosa_utils::config::ssl::{SslConfig, SslConfigContext as _, Store};

    use std::os::fd::AsRawFd as _;
    use stream::Stream;
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt};

    use super::*;

    #[cfg(target_family = "unix")]
    #[tokio::test]
    async fn unix_client_server() {
        let addr = "/tmp/prosa_unix_client_server_test.sock";
        let listener = StreamListener::Unix(tokio::net::UnixListener::bind(addr).unwrap());
        assert!(listener.as_raw_fd() > 0);
        assert!(
            format!("{listener:?}").contains("UnixListener"),
            "listener `{listener:?}` don't contain UnixListener"
        );
        assert!(
            format!("{listener:?}").contains(addr),
            "listener `{listener:?}` don't contain {addr}"
        );
        assert_eq!(
            "unix:///tmp/prosa_unix_client_server_test.sock",
            &listener.to_string()
        );

        let server = async move {
            let (mut client_stream, client_addr) = listener.accept().await.unwrap();
            assert!(client_addr.is_loopback());

            let mut buf = [0; 5];
            client_stream.read_exact(&mut buf).await.unwrap();
            assert_eq!(&buf, b"ProSA");

            client_stream.write_all(b"Worldline").await.unwrap();
        };

        let client = async {
            let mut stream = Stream::connect_unix(addr).await.unwrap();
            assert!(stream.as_raw_fd() > 0);
            assert!(
                format!("{stream:?}").contains("UnixStream"),
                "stream `{stream:?}` don't contain UnixStream"
            );
            assert!(
                format!("{stream:?}").contains(addr),
                "stream `{stream:?}` don't contain {addr}"
            );

            stream.write_all(b"ProSA").await.unwrap();

            let mut buf = Vec::new();
            stream.read_to_end(&mut buf).await.unwrap();
            assert_eq!(buf, b"Worldline");

            let _ = stream.shutdown().await;
        };

        future::join(server, client).await;
        std::fs::remove_file(addr).unwrap();
    }

    #[tokio::test]
    async fn tcp_client_server() {
        let addr = "localhost:41800";
        let listener = StreamListener::bind(addr).await.unwrap();
        assert!(listener.as_raw_fd() > 0);
        assert!(
            format!("{listener:?}").contains("Tcp"),
            "listener `{listener:?}` don't contain Tcp"
        );
        assert!(
            format!("{listener:?}").contains("TcpListener"),
            "listener `{listener:?}` don't contain TcpListener"
        );
        assert!(listener.to_string().starts_with("tcp://"));

        let server = async move {
            let (mut client_stream, client_addr) = listener.accept().await.unwrap();
            assert!(client_addr.is_loopback());

            let mut buf = [0; 5];
            client_stream.read_exact(&mut buf).await.unwrap();
            assert_eq!(&buf, b"ProSA");

            // Should do nothing
            client_stream = listener.handshake(client_stream).await.unwrap();

            client_stream.write_all(b"Worldline").await.unwrap();
        };

        let client = async {
            let mut stream = Stream::connect_tcp(addr).await.unwrap();
            assert!(stream.as_raw_fd() > 0);
            assert!(
                format!("{stream:?}").contains("Tcp"),
                "stream `{stream:?}` don't contain Tcp"
            );
            assert!(
                format!("{stream:?}").contains("TcpStream"),
                "stream `{stream:?}` don't contain TcpStream"
            );
            assert!(stream.to_string().starts_with("tcp://"));

            stream.write_all(b"ProSA").await.unwrap();

            let mut buf = Vec::new();
            stream.read_to_end(&mut buf).await.unwrap();
            assert_eq!(buf, b"Worldline");

            let _ = stream.shutdown().await;
        };

        future::join(server, client).await;
    }

    #[cfg(feature = "openssl")]
    #[tokio::test]
    async fn openssl_client_server() {
        let addr = "localhost:41443";
        let addr_url = Url::parse(format!("tls://{addr}").as_str()).unwrap();
        let cert_path = std::env::temp_dir()
            .join("test_openssl_client_server.pem")
            .to_str()
            .unwrap()
            .to_string();

        let mut ssl_config = SslConfig::new_self_cert(cert_path.clone());
        let listener = {
            let ssl_acceptor_builder: ::openssl::ssl::SslAcceptorBuilder = ssl_config
                .init_tls_server_context(addr_url.host_str())
                .unwrap();
            let ssl_acceptor = ssl_acceptor_builder.build();
            StreamListener::bind(addr)
                .await
                .unwrap()
                .ssl_acceptor(ssl_acceptor, Some(ssl_config.get_ssl_timeout()))
        };

        assert!(listener.as_raw_fd() > 0);
        assert!(
            format!("{listener:?}").contains("Ssl"),
            "listener `{listener:?}` don't contain Ssl"
        );
        assert!(
            format!("{listener:?}").contains("TcpListener"),
            "listener `{listener:?}` don't contain TcpListener"
        );
        assert!(listener.to_string().starts_with("ssl://"));

        let server = async move {
            let (mut client_stream, client_addr) = listener.accept().await.unwrap();
            assert!(client_addr.is_loopback());

            let mut buf = [0; 5];
            client_stream.read_exact(&mut buf).await.unwrap();
            assert_eq!(&buf, b"ProSA");

            client_stream.write_all(b"Worldline").await.unwrap();

            let _ = client_stream.shutdown().await;
        };

        ssl_config.set_store(Store::File { path: cert_path });
        let client = async {
            let mut stream = {
                let ssl_client_context: ::openssl::ssl::SslConnectorBuilder =
                    ssl_config.init_tls_client_context().unwrap();

                Stream::connect_openssl(&addr_url, &ssl_client_context.build())
                    .await
                    .unwrap()
            };

            assert!(stream.as_raw_fd() > 0);
            assert!(
                format!("{stream:?}").contains("Ssl"),
                "stream `{stream:?}` don't contain Ssl"
            );
            assert!(stream.to_string().starts_with("ssl://"));

            stream.write_all(b"ProSA").await.unwrap();

            let mut buf = Vec::new();
            stream.read_to_end(&mut buf).await.unwrap();
            assert_eq!(buf, b"Worldline");

            let _ = stream.shutdown().await;
        };

        future::join(server, client).await;
    }

    #[cfg(feature = "openssl")]
    #[tokio::test]
    async fn openssl_client_server_raw() {
        let addr = "localhost:41453";
        let addr_url = Url::parse(format!("tls://{addr}").as_str()).unwrap();
        let cert_path = std::env::temp_dir()
            .join("test_openssl_client_server_raw.pem")
            .to_str()
            .unwrap()
            .to_string();

        let mut ssl_config = SslConfig::new_self_cert(cert_path.clone());
        let listener = {
            let ssl_acceptor_builder: ::openssl::ssl::SslAcceptorBuilder = ssl_config
                .init_tls_server_context(addr_url.host_str())
                .unwrap();
            let ssl_acceptor = ssl_acceptor_builder.build();
            StreamListener::bind(addr)
                .await
                .unwrap()
                .ssl_acceptor(ssl_acceptor, Some(ssl_config.get_ssl_timeout()))
        };

        assert!(listener.as_raw_fd() > 0);
        assert!(
            format!("{listener:?}").contains("Ssl"),
            "listener `{listener:?}` don't contain Ssl"
        );
        assert!(
            format!("{listener:?}").contains("TcpListener"),
            "listener `{listener:?}` don't contain TcpListener"
        );
        assert!(listener.to_string().starts_with("ssl://"));

        let server = async move {
            let (mut client_stream, client_addr) = listener.accept_raw().await.unwrap();
            assert!(client_addr.is_loopback());
            client_stream = listener.handshake(client_stream).await.unwrap();

            let mut buf = [0; 5];
            client_stream.read_exact(&mut buf).await.unwrap();
            assert_eq!(&buf, b"ProSA");

            client_stream.write_all(b"Worldline").await.unwrap();

            let _ = client_stream.shutdown().await;
        };

        ssl_config.set_store(Store::File { path: cert_path });
        let client = async {
            let mut stream = {
                let ssl_client_context: ::openssl::ssl::SslConnectorBuilder =
                    ssl_config.init_tls_client_context().unwrap();

                Stream::connect_openssl(&addr_url, &ssl_client_context.build())
                    .await
                    .unwrap()
            };

            assert!(stream.as_raw_fd() > 0);
            assert!(
                format!("{stream:?}").contains("Ssl"),
                "stream `{stream:?}` don't contain Ssl"
            );
            assert!(stream.to_string().starts_with("ssl://"));

            stream.write_all(b"ProSA").await.unwrap();

            let mut buf = Vec::new();
            stream.read_to_end(&mut buf).await.unwrap();
            assert_eq!(buf, b"Worldline");

            let _ = stream.shutdown().await;
        };

        future::join(server, client).await;
    }

    #[cfg(feature = "openssl")]
    #[tokio::test]
    async fn ssl_client_server_with_config() {
        let addr_str = "tls://localhost:41463";
        let addr = Url::parse(addr_str).unwrap();
        let cert_path = std::env::temp_dir()
            .join("test_ssl_client_server_with_config.pem")
            .to_str()
            .unwrap()
            .to_string();

        let mut server_ssl_config = SslConfig::new_self_cert(cert_path.clone());
        server_ssl_config.set_alpn(vec!["prosa/1".into(), "h2".into()]);

        let listener_settings =
            listener::ListenerSetting::new(addr.clone(), Some(server_ssl_config));
        assert!(
            format!("{listener_settings:?}").contains("tls")
                && format!("{listener_settings:?}").contains("localhost")
                && format!("{listener_settings:?}").contains("41463"),
            "`{listener_settings:?}` Not contain the address {addr_str}"
        );
        assert!(
            listener_settings.to_string().starts_with(addr_str),
            "`{listener_settings}` Not start with the address {addr_str}"
        );
        assert!(listener_settings.to_string().starts_with(addr_str));

        let listener = listener_settings.bind().await.unwrap();
        assert!(listener.as_raw_fd() > 0);
        assert!(
            format!("{listener:?}").contains("Ssl"),
            "listener `{listener:?}` don't contain Ssl"
        );
        assert!(
            format!("{listener:?}").contains("TcpListener"),
            "listener `{listener:?}` don't contain TcpListener"
        );

        let server = async move {
            let (mut client_stream, client_addr) = listener.accept().await.unwrap();
            assert!(client_addr.is_loopback());

            let mut buf = [0; 5];
            client_stream.read_exact(&mut buf).await.unwrap();
            assert_eq!(&buf, b"ProSA");

            // Should do nothing
            client_stream = listener.handshake(client_stream).await.unwrap();

            client_stream.write_all(b"Worldline").await.unwrap();

            let _ = client_stream.shutdown().await;
        };

        let mut client_ssl_config = SslConfig::default();
        client_ssl_config.set_alpn(vec!["http/1.1".into(), "prosa/1".into()]);
        client_ssl_config.set_store(Store::File { path: cert_path });
        let target_settings = stream::TargetSetting::new(addr, Some(client_ssl_config), None);
        assert_eq!(addr_str, target_settings.to_string());

        let client = async {
            let mut stream = target_settings.connect().await.unwrap();
            assert!(stream.as_raw_fd() > 0);
            assert!(
                format!("{stream:?}").contains("Ssl") || format!("{stream:?}").contains("Tls"),
                "stream `{stream:?}` don't contain Ssl or Tls"
            );
            if stream.is_ssl() {
                assert!(stream.selected_alpn_check(|alpn| { alpn == b"prosa/1".as_slice() }));
            } else {
                panic!("Should be an SSL stream for client");
            }

            stream.write_all(b"ProSA").await.unwrap();

            let mut buf = Vec::new();
            stream.read_to_end(&mut buf).await.unwrap();
            assert_eq!(buf, b"Worldline");

            let _ = stream.shutdown().await;
        };

        future::join(server, client).await;
    }

    #[cfg(all(feature = "openssl", not(feature = "openssl-vendored")))]
    #[tokio::test]
    async fn ssl_client_public_with_config() {
        let addr_str = "https://worldline.com/";
        let addr = Url::parse(addr_str).unwrap();

        let target_settings = stream::TargetSetting::new(addr, Some(SslConfig::default()), None);
        assert_eq!(addr_str, target_settings.to_string());

        let mut stream = target_settings.connect().await.unwrap();
        assert!(stream.as_raw_fd() > 0);
        assert!(
            format!("{stream:?}").contains("Ssl") || format!("{stream:?}").contains("Tls"),
            "stream `{stream:?}` don't contain Ssl or Tls"
        );

        let _ = stream.shutdown().await;
    }
}
