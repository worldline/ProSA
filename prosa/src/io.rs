//! Module that define IO that could be use by a ProSA processor
use std::{
    fmt,
    net::{SocketAddrV4, SocketAddrV6},
    path::Path,
};

pub use prosa_macros::io;
use url::Url;

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
#[derive(Debug)]
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
    use listener::{ListenerSetting, StreamListener};
    use openssl::ssl::SslVerifyMode;
    use prosa_utils::config::ssl::{SslConfig, Store};
    use std::{env, os::fd::AsRawFd as _};
    use stream::{Stream, TargetSetting};
    use tokio::{
        fs::File,
        io::{AsyncReadExt as _, AsyncWriteExt},
    };

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

            let mut buf = vec![];
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

            let mut buf = vec![];
            stream.read_to_end(&mut buf).await.unwrap();
            assert_eq!(buf, b"Worldline");

            let _ = stream.shutdown().await;
        };

        future::join(server, client).await;
    }

    #[tokio::test]
    async fn ssl_client_server() {
        let addr = "localhost:41443";
        let addr_url = Url::parse(format!("tls://{addr}").as_str()).unwrap();

        let ssl_config = SslConfig::default();
        let ssl_acceptor = ssl_config
            .init_tls_server_context(addr_url.host_str())
            .unwrap()
            .build();
        let listener = StreamListener::bind(addr)
            .await
            .unwrap()
            .ssl_acceptor(ssl_acceptor, Some(ssl_config.get_ssl_timeout()));
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
        };

        let client = async {
            let mut ssl_client_context = ssl_config.init_tls_client_context().unwrap();
            ssl_client_context.set_verify(SslVerifyMode::NONE);

            let mut stream = Stream::connect_ssl(&addr_url, &ssl_client_context.build())
                .await
                .unwrap();
            assert!(stream.as_raw_fd() > 0);
            assert!(
                format!("{stream:?}").contains("Ssl"),
                "stream `{stream:?}` don't contain Ssl"
            );
            assert!(stream.to_string().starts_with("ssl://"));

            stream.write_all(b"ProSA").await.unwrap();

            let mut buf = vec![];
            stream.read_to_end(&mut buf).await.unwrap();
            assert_eq!(buf, b"Worldline");

            let _ = stream.shutdown().await;
        };

        future::join(server, client).await;
    }

    #[tokio::test]
    async fn ssl_client_server_raw() {
        let addr = "localhost:41453";
        let addr_url = Url::parse(format!("tls://{addr}").as_str()).unwrap();

        let ssl_config = SslConfig::default();
        let ssl_acceptor = ssl_config
            .init_tls_server_context(addr_url.host_str())
            .unwrap()
            .build();
        let listener = StreamListener::bind(addr)
            .await
            .unwrap()
            .ssl_acceptor(ssl_acceptor, Some(ssl_config.get_ssl_timeout()));
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
        };

        let client = async {
            let mut ssl_client_context = ssl_config.init_tls_client_context().unwrap();
            ssl_client_context.set_verify(SslVerifyMode::NONE);

            let mut stream = Stream::connect_ssl(&addr_url, &ssl_client_context.build())
                .await
                .unwrap();
            assert!(stream.as_raw_fd() > 0);
            assert!(
                format!("{stream:?}").contains("Ssl"),
                "stream `{stream:?}` don't contain Ssl"
            );
            assert!(stream.to_string().starts_with("ssl://"));

            stream.write_all(b"ProSA").await.unwrap();

            let mut buf = vec![];
            stream.read_to_end(&mut buf).await.unwrap();
            assert_eq!(buf, b"Worldline");

            let _ = stream.shutdown().await;
        };

        future::join(server, client).await;
    }

    #[tokio::test]
    async fn ssl_client_server_with_config() {
        let temp_cert_dir = env::temp_dir();
        let addr_str = "tls://localhost:41463";
        let addr = Url::parse(addr_str).unwrap();

        let mut server_ssl_config = SslConfig::default();
        server_ssl_config.set_alpn(vec!["prosa/1".into(), "h2".into()]);

        let listener_settings = ListenerSetting::new(addr.clone(), Some(server_ssl_config));
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
        if let StreamListener::Ssl(_, acceptor, _) = &listener {
            let server_cert = acceptor.context().certificate().unwrap();
            let mut server_cert_file = File::create(temp_cert_dir.join("prosa_test_server.pem"))
                .await
                .unwrap();
            server_cert_file
                .write_all(&server_cert.to_pem().unwrap())
                .await
                .unwrap();
        }
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
        };

        let mut client_ssl_config = SslConfig::default();
        client_ssl_config.set_alpn(vec!["http/1.1".into(), "prosa/1".into()]);
        let ssl_store = Store::File {
            path: temp_cert_dir.to_str().unwrap().to_string(),
        };
        client_ssl_config.set_store(ssl_store);
        let target_settings = TargetSetting::new(addr, Some(client_ssl_config), None);
        assert_eq!(addr_str, target_settings.to_string());

        let client = async {
            let mut stream = target_settings.connect().await.unwrap();
            assert!(stream.as_raw_fd() > 0);
            assert!(
                format!("{stream:?}").contains("Ssl"),
                "stream `{stream:?}` don't contain Ssl"
            );
            if let Stream::Ssl(s) = &stream {
                assert_eq!(
                    Some(b"prosa/1".as_slice()),
                    s.ssl().selected_alpn_protocol()
                );
            } else {
                panic!("Should be an SSL stream for client");
            }

            stream.write_all(b"ProSA").await.unwrap();

            let mut buf = vec![];
            stream.read_to_end(&mut buf).await.unwrap();
            assert_eq!(buf, b"Worldline");

            let _ = stream.shutdown().await;
        };

        future::join(server, client).await;
    }
}
