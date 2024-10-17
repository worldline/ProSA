//! Module that define IO that could be use by a ProSA processor
use std::net::{SocketAddrV4, SocketAddrV6};

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
            format!("{:?}", listener).contains("UnixListener"),
            "listener `{:?}` don't contain UnixListener",
            listener
        );
        assert!(
            format!("{:?}", listener).contains(addr),
            "listener `{:?}` don't contain {}",
            listener,
            addr
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
                format!("{:?}", stream).contains("UnixStream"),
                "stream `{:?}` don't contain UnixStream",
                stream
            );
            assert!(
                format!("{:?}", stream).contains(addr),
                "stream `{:?}` don't contain {}",
                stream,
                addr
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
            format!("{:?}", listener).contains("Tcp"),
            "listener `{:?}` don't contain Tcp",
            listener
        );
        assert!(
            format!("{:?}", listener).contains("TcpListener"),
            "listener `{:?}` don't contain TcpListener",
            listener
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
            let mut stream = Stream::connect_tcp(addr).await.unwrap();
            assert!(stream.as_raw_fd() > 0);
            assert!(
                format!("{:?}", stream).contains("Tcp"),
                "stream `{:?}` don't contain Tcp",
                stream
            );
            assert!(
                format!("{:?}", stream).contains("TcpStream"),
                "stream `{:?}` don't contain TcpStream",
                stream
            );

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
        let addr_url = Url::parse(format!("tls://{}", addr).as_str()).unwrap();

        let ssl_config = SslConfig::default();
        let ssl_acceptor = ssl_config
            .init_tls_server_context(addr_url.domain())
            .unwrap()
            .build();
        let listener = StreamListener::bind(addr)
            .await
            .unwrap()
            .ssl_acceptor(ssl_acceptor, Some(ssl_config.get_ssl_timeout()));
        assert!(listener.as_raw_fd() > 0);
        assert!(
            format!("{:?}", listener).contains("Ssl"),
            "listener `{:?}` don't contain Ssl",
            listener
        );
        assert!(
            format!("{:?}", listener).contains("TcpListener"),
            "listener `{:?}` don't contain TcpListener",
            listener
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
            let mut ssl_client_context = ssl_config.init_tls_client_context().unwrap();
            ssl_client_context.set_verify(SslVerifyMode::NONE);

            let mut stream = Stream::connect_ssl(&addr_url, &ssl_client_context.build())
                .await
                .unwrap();
            assert!(stream.as_raw_fd() > 0);
            assert!(
                format!("{:?}", stream).contains("Ssl"),
                "stream `{:?}` don't contain Ssl",
                stream
            );

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
        let addr_str = "tls://localhost:41453";
        let addr = Url::parse(addr_str).unwrap();

        let listener_settings = ListenerSetting::new(addr.clone(), None);
        assert!(
            format!("{:?}", listener_settings).contains("tls")
                && format!("{:?}", listener_settings).contains("localhost")
                && format!("{:?}", listener_settings).contains("41453"),
            "`{:?}` Not contain the address {}",
            listener_settings,
            addr_str
        );
        assert!(
            listener_settings.to_string().starts_with(addr_str),
            "`{}` Not start with the address {}",
            listener_settings,
            addr_str
        );

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
            format!("{:?}", listener).contains("Ssl"),
            "listener `{:?}` don't contain Ssl",
            listener
        );
        assert!(
            format!("{:?}", listener).contains("TcpListener"),
            "listener `{:?}` don't contain TcpListener",
            listener
        );

        let server = async move {
            let (mut client_stream, client_addr) = listener.accept().await.unwrap();
            assert!(client_addr.is_loopback());

            let mut buf = [0; 5];
            client_stream.read_exact(&mut buf).await.unwrap();
            assert_eq!(&buf, b"ProSA");

            client_stream.write_all(b"Worldline").await.unwrap();
        };

        let mut client_ssl_config = SslConfig::default();
        let ssl_store = Store::new(temp_cert_dir.to_str().unwrap().to_string() + "/");
        client_ssl_config.set_store(ssl_store);
        let target_settings = TargetSetting::new(addr, Some(client_ssl_config), None);
        assert_eq!(addr_str, target_settings.to_string());

        let client = async {
            let mut stream = target_settings.connect().await.unwrap();
            assert!(stream.as_raw_fd() > 0);
            assert!(
                format!("{:?}", stream).contains("Ssl"),
                "stream `{:?}` don't contain Ssl",
                stream
            );

            stream.write_all(b"ProSA").await.unwrap();

            let mut buf = vec![];
            stream.read_to_end(&mut buf).await.unwrap();
            assert_eq!(buf, b"Worldline");

            let _ = stream.shutdown().await;
        };

        future::join(server, client).await;
    }
}
