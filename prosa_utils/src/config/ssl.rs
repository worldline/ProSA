//! Definition of SSL configuration

use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fmt, time::Duration};

use super::ConfigError;

#[cfg(feature = "config-openssl")]
pub mod openssl;

/// Trait to define an SSL store with custom SSL objects
pub trait SslStore<C, S> {
    /// Method to read certificates from its path. Get all certificates in subfolders
    fn get_file_certificates(path: &std::path::Path) -> Result<Vec<C>, ConfigError>;

    /// Method to get a cert store
    ///
    /// ```
    /// use prosa_utils::config::ssl::{Store, SslStore};
    ///
    /// let store = Store::File { path: "./target".into() };
    /// let ssl_store = store.get_store().unwrap();
    /// ```
    fn get_store(&self) -> Result<S, ConfigError>;

    /// Method to get all OpenSSL certificate with their names as key
    ///
    /// ```
    /// use prosa_utils::config::ssl::{Store, SslStore};
    ///
    /// let store = Store::File { path: "./target".into() };
    /// let certs_map = store.get_certs().unwrap();
    ///
    /// // No cert in target
    /// assert!(certs_map.is_empty());
    /// ```
    fn get_certs(&self) -> Result<HashMap<String, C>, ConfigError>;
}

/// SSL configuration object for store certificates
#[derive(Default, Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(untagged)]
pub enum Store {
    /// Will use the system trusted certificates
    #[default]
    System,
    /// Store path that contain certificate(s)
    File {
        /// Path of the store (can be directory, file, glob pattern)
        path: String,
    },
    /// Store certs that contain PEMs
    Cert {
        /// List of string PEMs for certificates
        certs: Vec<String>,
    },
}

impl fmt::Display for Store {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Store::System => write!(f, "System store"),
            Store::File { path } => write!(f, "Store cert path [{path}]"),
            Store::Cert { certs: _ } => write!(f, "Store cert list"),
        }?;

        #[cfg(feature = "config-openssl")]
        {
            writeln!(f, ":")?;
            let certs: HashMap<String, ::openssl::x509::X509> =
                self.get_certs().unwrap_or_default();
            for (name, cert) in certs {
                if f.alternate() {
                    writeln!(f, "{name}:\n{cert:#?}")?;
                } else {
                    writeln!(f, "{name}")?;
                }
            }
        }

        Ok(())
    }
}

/// Trait to define SSL configuration context for socket
pub trait SslConfigContext<C, S> {
    /// Method to init an SSL context for a client socket
    ///
    /// ```
    /// use prosa_utils::config::ssl::{Store, SslConfig, SslConfigContext as _};
    ///
    /// let mut client_config = SslConfig::default();
    /// client_config.set_store(Store::File { path: "./target".into() });
    /// if let Ok(mut ssl_context_builder) = client_config.init_tls_client_context() {
    ///     let ssl_context = ssl_context_builder.build();
    /// }
    /// ```
    fn init_tls_client_context(&self) -> Result<C, ConfigError>;

    /// Method to init an SSL context for a server socket
    ///
    /// ```
    /// use prosa_utils::config::ssl::{SslConfig, SslConfigContext as _};
    ///
    /// let server_config = SslConfig::new_pkcs12("server.pkcs12".into());
    /// if let Ok(mut ssl_context_builder) = server_config.init_tls_server_context(None) {
    ///     let ssl_context = ssl_context_builder.build();
    /// }
    /// ```
    fn init_tls_server_context(&self, host: Option<&str>) -> Result<S, ConfigError>;
}

/// SSL configuration for socket
///
/// Client SSL socket
/// ```
/// use std::io;
/// use std::pin::Pin;
/// use tokio::net::TcpStream;
/// use tokio_openssl::SslStream;
/// use openssl::ssl::{ErrorCode, Ssl, SslMethod, SslVerifyMode};
/// use prosa_utils::config::ssl::{SslConfig, SslConfigContext};
///
/// async fn client() -> Result<(), io::Error> {
///     let mut stream = TcpStream::connect("localhost:4443").await?;
///
///     let client_config = SslConfig::default();
///     if let Ok(mut ssl_context_builder) = client_config.init_tls_client_context() {
///         let ssl = ssl_context_builder.build().configure().unwrap().into_ssl("localhost").unwrap();
///         let mut stream = SslStream::new(ssl, stream).unwrap();
///         if let Err(e) = Pin::new(&mut stream).connect().await {
///             if e.code() != ErrorCode::ZERO_RETURN {
///                 eprintln!("Can't connect the client: {}", e);
///             }
///         }
///
///         // SSL stream ...
///     }
///
///     Ok(())
/// }
/// ```
///
/// Server SSL socket
/// ```
/// use std::io;
/// use std::pin::Pin;
/// use tokio::net::TcpListener;
/// use tokio_openssl::SslStream;
/// use openssl::ssl::{ErrorCode, Ssl, SslMethod, SslVerifyMode};
/// use prosa_utils::config::ssl::{SslConfig, SslConfigContext};
///
/// async fn server() -> Result<(), io::Error> {
///     let listener = TcpListener::bind("0.0.0.0:4443").await?;
///
///     let server_config = SslConfig::new_cert_key("cert.pem".into(), "cert.key".into(), Some("passphrase".into()));
///     if let Ok(mut ssl_context_builder) = server_config.init_tls_server_context(None) {
///         ssl_context_builder.set_verify(SslVerifyMode::NONE);
///         let ssl_context = ssl_context_builder.build();
///
///         loop {
///             let (stream, cli_addr) = listener.accept().await?;
///             let ssl = Ssl::new(&ssl_context.context()).unwrap();
///             let mut stream = SslStream::new(ssl, stream).unwrap();
///             if let Err(e) = Pin::new(&mut stream).accept().await {
///                 if e.code() != ErrorCode::ZERO_RETURN {
///                     eprintln!("Can't accept the client {}: {}", cli_addr, e);
///                 }
///             }
///
///             // SSL stream ...
///         }
///     }
///
///     Ok(())
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct SslConfig {
    /// SSL store certificate to verify the remote certificate
    store: Option<Store>,
    /// PKCS12 object for certificate
    pkcs12: Option<String>,
    /// certificate
    cert: Option<String>,
    /// private key
    key: Option<String>,
    /// passphrase for private key or pkcs12
    passphrase: Option<String>,
    #[serde(default)]
    /// ALPN list send by the client, or order of ALPN accepted by the server
    alpn: Vec<String>,
    #[serde(skip_serializing)]
    #[serde(default = "SslConfig::default_modern_security")]
    /// Security level. If `true`, it'll use the [modern version 5 of Mozilla's](https://wiki.mozilla.org/Security/Server_Side_TLS) TLS recommendations.
    pub modern_security: bool,
    #[serde(skip_serializing)]
    #[serde(default = "SslConfig::default_ssl_timeout")]
    /// SSL operation timeout in milliseconds
    pub ssl_timeout: u64,
}

impl SslConfig {
    fn default_modern_security() -> bool {
        true
    }

    fn default_ssl_timeout() -> u64 {
        3000
    }

    /// Method to create an ssl configuration from a pkcs12 manually
    /// Should be use with config instead of building it manually
    pub fn new_pkcs12(pkcs12_path: String) -> SslConfig {
        SslConfig {
            store: None,
            pkcs12: Some(pkcs12_path),
            cert: None,
            key: None,
            passphrase: None,
            alpn: Vec::default(),
            modern_security: Self::default_modern_security(),
            ssl_timeout: Self::default_ssl_timeout(),
        }
    }

    /// Method to create an ssl configuration from a certificate and its key manually
    /// Should be use with config instead of building it manually
    pub fn new_cert_key(
        cert_path: String,
        key_path: String,
        passphrase: Option<String>,
    ) -> SslConfig {
        SslConfig {
            store: None,
            pkcs12: None,
            cert: Some(cert_path),
            key: Some(key_path),
            passphrase,
            alpn: Vec::default(),
            modern_security: Self::default_modern_security(),
            ssl_timeout: Self::default_ssl_timeout(),
        }
    }

    /// Method to create an ssl configuration that will generate a self signed certificate and write it's certificate to the _cert_path_
    /// Should be use with config instead of building it manually
    pub fn new_self_cert(cert_path: String) -> SslConfig {
        SslConfig {
            store: None,
            pkcs12: None,
            cert: Some(cert_path),
            key: None,
            passphrase: None,
            alpn: Vec::default(),
            modern_security: Self::default_modern_security(),
            ssl_timeout: Self::default_ssl_timeout(),
        }
    }

    /// Getter of the SSL timeout
    pub fn get_ssl_timeout(&self) -> Duration {
        Duration::from_millis(self.ssl_timeout)
    }

    /// Setter of the store certificate
    pub fn set_store(&mut self, store: Store) {
        self.store = Some(store);
    }

    /// Setter of the ALPN list send by the client, or order of ALPN accepted by the server
    pub fn set_alpn(&mut self, alpn: Vec<String>) {
        self.alpn = alpn;
    }
}

impl Default for SslConfig {
    fn default() -> SslConfig {
        SslConfig {
            store: None,
            pkcs12: None,
            cert: None,
            key: None,
            passphrase: None,
            alpn: Vec::default(),
            modern_security: Self::default_modern_security(),
            ssl_timeout: Self::default_ssl_timeout(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_store() {
        let inline_store_le_x1_x2 = Store::Cert {
            certs: vec![
                "-----BEGIN CERTIFICATE-----
MIIFazCCA1OgAwIBAgIRAIIQz7DSQONZRGPgu2OCiwAwDQYJKoZIhvcNAQELBQAw
TzELMAkGA1UEBhMCVVMxKTAnBgNVBAoTIEludGVybmV0IFNlY3VyaXR5IFJlc2Vh
cmNoIEdyb3VwMRUwEwYDVQQDEwxJU1JHIFJvb3QgWDEwHhcNMTUwNjA0MTEwNDM4
WhcNMzUwNjA0MTEwNDM4WjBPMQswCQYDVQQGEwJVUzEpMCcGA1UEChMgSW50ZXJu
ZXQgU2VjdXJpdHkgUmVzZWFyY2ggR3JvdXAxFTATBgNVBAMTDElTUkcgUm9vdCBY
MTCCAiIwDQYJKoZIhvcNAQEBBQADggIPADCCAgoCggIBAK3oJHP0FDfzm54rVygc
h77ct984kIxuPOZXoHj3dcKi/vVqbvYATyjb3miGbESTtrFj/RQSa78f0uoxmyF+
0TM8ukj13Xnfs7j/EvEhmkvBioZxaUpmZmyPfjxwv60pIgbz5MDmgK7iS4+3mX6U
A5/TR5d8mUgjU+g4rk8Kb4Mu0UlXjIB0ttov0DiNewNwIRt18jA8+o+u3dpjq+sW
T8KOEUt+zwvo/7V3LvSye0rgTBIlDHCNAymg4VMk7BPZ7hm/ELNKjD+Jo2FR3qyH
B5T0Y3HsLuJvW5iB4YlcNHlsdu87kGJ55tukmi8mxdAQ4Q7e2RCOFvu396j3x+UC
B5iPNgiV5+I3lg02dZ77DnKxHZu8A/lJBdiB3QW0KtZB6awBdpUKD9jf1b0SHzUv
KBds0pjBqAlkd25HN7rOrFleaJ1/ctaJxQZBKT5ZPt0m9STJEadao0xAH0ahmbWn
OlFuhjuefXKnEgV4We0+UXgVCwOPjdAvBbI+e0ocS3MFEvzG6uBQE3xDk3SzynTn
jh8BCNAw1FtxNrQHusEwMFxIt4I7mKZ9YIqioymCzLq9gwQbooMDQaHWBfEbwrbw
qHyGO0aoSCqI3Haadr8faqU9GY/rOPNk3sgrDQoo//fb4hVC1CLQJ13hef4Y53CI
rU7m2Ys6xt0nUW7/vGT1M0NPAgMBAAGjQjBAMA4GA1UdDwEB/wQEAwIBBjAPBgNV
HRMBAf8EBTADAQH/MB0GA1UdDgQWBBR5tFnme7bl5AFzgAiIyBpY9umbbjANBgkq
hkiG9w0BAQsFAAOCAgEAVR9YqbyyqFDQDLHYGmkgJykIrGF1XIpu+ILlaS/V9lZL
ubhzEFnTIZd+50xx+7LSYK05qAvqFyFWhfFQDlnrzuBZ6brJFe+GnY+EgPbk6ZGQ
3BebYhtF8GaV0nxvwuo77x/Py9auJ/GpsMiu/X1+mvoiBOv/2X/qkSsisRcOj/KK
NFtY2PwByVS5uCbMiogziUwthDyC3+6WVwW6LLv3xLfHTjuCvjHIInNzktHCgKQ5
ORAzI4JMPJ+GslWYHb4phowim57iaztXOoJwTdwJx4nLCgdNbOhdjsnvzqvHu7Ur
TkXWStAmzOVyyghqpZXjFaH3pO3JLF+l+/+sKAIuvtd7u+Nxe5AW0wdeRlN8NwdC
jNPElpzVmbUq4JUagEiuTDkHzsxHpFKVK7q4+63SM1N95R1NbdWhscdCb+ZAJzVc
oyi3B43njTOQ5yOf+1CceWxG1bQVs5ZufpsMljq4Ui0/1lvh+wjChP4kqKOJ2qxq
4RgqsahDYVvTH9w7jXbyLeiNdd8XM2w9U/t7y0Ff/9yi0GE44Za4rF2LN9d11TPA
mRGunUHBcnWEvgJBQl9nJEiU0Zsnvgc/ubhPgXRR4Xq37Z0j4r7g1SgEEzwxA57d
emyPxgcYxn/eR44/KJ4EBs+lVDR3veyJm+kXQ99b21/+jh5Xos1AnX5iItreGCc=
-----END CERTIFICATE-----"
                    .to_string(),
                "-----BEGIN CERTIFICATE-----
MIICGzCCAaGgAwIBAgIQQdKd0XLq7qeAwSxs6S+HUjAKBggqhkjOPQQDAzBPMQsw
CQYDVQQGEwJVUzEpMCcGA1UEChMgSW50ZXJuZXQgU2VjdXJpdHkgUmVzZWFyY2gg
R3JvdXAxFTATBgNVBAMTDElTUkcgUm9vdCBYMjAeFw0yMDA5MDQwMDAwMDBaFw00
MDA5MTcxNjAwMDBaME8xCzAJBgNVBAYTAlVTMSkwJwYDVQQKEyBJbnRlcm5ldCBT
ZWN1cml0eSBSZXNlYXJjaCBHcm91cDEVMBMGA1UEAxMMSVNSRyBSb290IFgyMHYw
EAYHKoZIzj0CAQYFK4EEACIDYgAEzZvVn4CDCuwJSvMWSj5cz3es3mcFDR0HttwW
+1qLFNvicWDEukWVEYmO6gbf9yoWHKS5xcUy4APgHoIYOIvXRdgKam7mAHf7AlF9
ItgKbppbd9/w+kHsOdx1ymgHDB/qo0IwQDAOBgNVHQ8BAf8EBAMCAQYwDwYDVR0T
AQH/BAUwAwEB/zAdBgNVHQ4EFgQUfEKWrt5LSDv6kviejM9ti6lyN5UwCgYIKoZI
zj0EAwMDaAAwZQIwe3lORlCEwkSHRhtFcP9Ymd70/aTSVaYgLXTWNLxBo1BfASdW
tL4ndQavEi51mI38AjEAi/V3bNTIZargCyzuFJ0nN6T5U6VR5CmD1/iQMVtCnwr1
/q4AaOeMSQ+2b1tbFfLn
-----END CERTIFICATE-----"
                    .to_string(),
            ],
        };
        assert!(format!("{inline_store_le_x1_x2}").contains("ISRG Root X"));

        let config_store_le_x1_x2: Store = serde_yaml::from_str(
            "certs:
  - |
    -----BEGIN CERTIFICATE-----
    MIIFazCCA1OgAwIBAgIRAIIQz7DSQONZRGPgu2OCiwAwDQYJKoZIhvcNAQELBQAw
    TzELMAkGA1UEBhMCVVMxKTAnBgNVBAoTIEludGVybmV0IFNlY3VyaXR5IFJlc2Vh
    cmNoIEdyb3VwMRUwEwYDVQQDEwxJU1JHIFJvb3QgWDEwHhcNMTUwNjA0MTEwNDM4
    WhcNMzUwNjA0MTEwNDM4WjBPMQswCQYDVQQGEwJVUzEpMCcGA1UEChMgSW50ZXJu
    ZXQgU2VjdXJpdHkgUmVzZWFyY2ggR3JvdXAxFTATBgNVBAMTDElTUkcgUm9vdCBY
    MTCCAiIwDQYJKoZIhvcNAQEBBQADggIPADCCAgoCggIBAK3oJHP0FDfzm54rVygc
    h77ct984kIxuPOZXoHj3dcKi/vVqbvYATyjb3miGbESTtrFj/RQSa78f0uoxmyF+
    0TM8ukj13Xnfs7j/EvEhmkvBioZxaUpmZmyPfjxwv60pIgbz5MDmgK7iS4+3mX6U
    A5/TR5d8mUgjU+g4rk8Kb4Mu0UlXjIB0ttov0DiNewNwIRt18jA8+o+u3dpjq+sW
    T8KOEUt+zwvo/7V3LvSye0rgTBIlDHCNAymg4VMk7BPZ7hm/ELNKjD+Jo2FR3qyH
    B5T0Y3HsLuJvW5iB4YlcNHlsdu87kGJ55tukmi8mxdAQ4Q7e2RCOFvu396j3x+UC
    B5iPNgiV5+I3lg02dZ77DnKxHZu8A/lJBdiB3QW0KtZB6awBdpUKD9jf1b0SHzUv
    KBds0pjBqAlkd25HN7rOrFleaJ1/ctaJxQZBKT5ZPt0m9STJEadao0xAH0ahmbWn
    OlFuhjuefXKnEgV4We0+UXgVCwOPjdAvBbI+e0ocS3MFEvzG6uBQE3xDk3SzynTn
    jh8BCNAw1FtxNrQHusEwMFxIt4I7mKZ9YIqioymCzLq9gwQbooMDQaHWBfEbwrbw
    qHyGO0aoSCqI3Haadr8faqU9GY/rOPNk3sgrDQoo//fb4hVC1CLQJ13hef4Y53CI
    rU7m2Ys6xt0nUW7/vGT1M0NPAgMBAAGjQjBAMA4GA1UdDwEB/wQEAwIBBjAPBgNV
    HRMBAf8EBTADAQH/MB0GA1UdDgQWBBR5tFnme7bl5AFzgAiIyBpY9umbbjANBgkq
    hkiG9w0BAQsFAAOCAgEAVR9YqbyyqFDQDLHYGmkgJykIrGF1XIpu+ILlaS/V9lZL
    ubhzEFnTIZd+50xx+7LSYK05qAvqFyFWhfFQDlnrzuBZ6brJFe+GnY+EgPbk6ZGQ
    3BebYhtF8GaV0nxvwuo77x/Py9auJ/GpsMiu/X1+mvoiBOv/2X/qkSsisRcOj/KK
    NFtY2PwByVS5uCbMiogziUwthDyC3+6WVwW6LLv3xLfHTjuCvjHIInNzktHCgKQ5
    ORAzI4JMPJ+GslWYHb4phowim57iaztXOoJwTdwJx4nLCgdNbOhdjsnvzqvHu7Ur
    TkXWStAmzOVyyghqpZXjFaH3pO3JLF+l+/+sKAIuvtd7u+Nxe5AW0wdeRlN8NwdC
    jNPElpzVmbUq4JUagEiuTDkHzsxHpFKVK7q4+63SM1N95R1NbdWhscdCb+ZAJzVc
    oyi3B43njTOQ5yOf+1CceWxG1bQVs5ZufpsMljq4Ui0/1lvh+wjChP4kqKOJ2qxq
    4RgqsahDYVvTH9w7jXbyLeiNdd8XM2w9U/t7y0Ff/9yi0GE44Za4rF2LN9d11TPA
    mRGunUHBcnWEvgJBQl9nJEiU0Zsnvgc/ubhPgXRR4Xq37Z0j4r7g1SgEEzwxA57d
    emyPxgcYxn/eR44/KJ4EBs+lVDR3veyJm+kXQ99b21/+jh5Xos1AnX5iItreGCc=
    -----END CERTIFICATE-----
  - |
    -----BEGIN CERTIFICATE-----
    MIICGzCCAaGgAwIBAgIQQdKd0XLq7qeAwSxs6S+HUjAKBggqhkjOPQQDAzBPMQsw
    CQYDVQQGEwJVUzEpMCcGA1UEChMgSW50ZXJuZXQgU2VjdXJpdHkgUmVzZWFyY2gg
    R3JvdXAxFTATBgNVBAMTDElTUkcgUm9vdCBYMjAeFw0yMDA5MDQwMDAwMDBaFw00
    MDA5MTcxNjAwMDBaME8xCzAJBgNVBAYTAlVTMSkwJwYDVQQKEyBJbnRlcm5ldCBT
    ZWN1cml0eSBSZXNlYXJjaCBHcm91cDEVMBMGA1UEAxMMSVNSRyBSb290IFgyMHYw
    EAYHKoZIzj0CAQYFK4EEACIDYgAEzZvVn4CDCuwJSvMWSj5cz3es3mcFDR0HttwW
    +1qLFNvicWDEukWVEYmO6gbf9yoWHKS5xcUy4APgHoIYOIvXRdgKam7mAHf7AlF9
    ItgKbppbd9/w+kHsOdx1ymgHDB/qo0IwQDAOBgNVHQ8BAf8EBAMCAQYwDwYDVR0T
    AQH/BAUwAwEB/zAdBgNVHQ4EFgQUfEKWrt5LSDv6kviejM9ti6lyN5UwCgYIKoZI
    zj0EAwMDaAAwZQIwe3lORlCEwkSHRhtFcP9Ymd70/aTSVaYgLXTWNLxBo1BfASdW
    tL4ndQavEi51mI38AjEAi/V3bNTIZargCyzuFJ0nN6T5U6VR5CmD1/iQMVtCnwr1
    /q4AaOeMSQ+2b1tbFfLn
    -----END CERTIFICATE-----",
        )
        .unwrap();
        assert!(format!("{config_store_le_x1_x2}").contains("ISRG Root X"));

        let config_store_file: Store = serde_yaml::from_str("path: \"/opt\"").unwrap();
        assert_eq!(
            Store::File {
                path: "/opt".to_string()
            },
            config_store_file
        );
    }

    #[test]
    fn test_tls_server_context() {
        let ssl_config = SslConfig::default();
        let ssl_acceptor = ssl_config.init_tls_server_context(None).unwrap().build();

        // Check for self signed certificate
        assert!(ssl_acceptor.context().private_key().is_some());
        assert!(ssl_acceptor.context().certificate().is_some());
    }
}
