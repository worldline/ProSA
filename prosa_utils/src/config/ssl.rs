//! Definition of SSL configuration

use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fmt, io,
    path::{Path, PathBuf},
    sync::{
        Arc, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use super::ConfigError;
use crate::file::FileWatch;

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
    /// # #[cfg(feature="config-openssl")]
    /// let ssl_store = store.get_store().unwrap();
    /// ```
    fn get_store(&self) -> Result<S, ConfigError>;

    /// Method to get all OpenSSL certificate with their names as key
    ///
    /// ```
    /// use prosa_utils::config::ssl::{Store, SslStore};
    ///
    /// let store = Store::File { path: "./target".into() };
    /// # #[cfg(feature="config-openssl")]
    /// let certs_map = store.get_certs().unwrap();
    ///
    /// // No cert in target
    /// # #[cfg(feature="config-openssl")]
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

impl TryFrom<&Path> for Store {
    type Error = io::Error;

    /// Initialize a Store with a certificate path
    ///
    /// ```
    /// use std::path::Path;
    /// use prosa_utils::config::ssl::Store;
    ///
    /// let store = Store::try_from(Path::new("cert.pem")).expect("Path should be valid");
    /// assert_eq!(store, Store::File{ path: "cert.pem".into() });
    /// ```
    fn try_from(path: &Path) -> io::Result<Self> {
        path.to_str()
            .map(|s| Store::File {
                path: s.to_string(),
            })
            .ok_or(io::Error::new(
                io::ErrorKind::InvalidFilename,
                "Invalid store path",
            ))
    }
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
    /// # #[cfg(feature="config-openssl")]
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
    /// # #[cfg(feature="config-openssl")]
    /// if let Ok(mut ssl_context_builder) = server_config.init_tls_server_context(None) {
    ///     let ssl_context = ssl_context_builder.build();
    /// }
    /// ```
    fn init_tls_server_context(&self, host: Option<&str>) -> Result<S, ConfigError>;
}

/// SSL configuration for sockets.
///
/// Its [`Debug`](fmt::Debug) implementation deliberately omits the private-key or PKCS#12
/// passphrase.
///
/// Client SSL socket
/// ```
/// use std::io;
/// use std::pin::Pin;
/// use tokio::net::TcpStream;
/// use tokio_openssl::SslStream;
/// use prosa_utils::config::ssl::{SslConfig, SslConfigContext};
///
/// # #[cfg(feature="config-openssl")]
/// async fn client() -> Result<(), io::Error> {
///     let mut stream = TcpStream::connect("localhost:4443").await?;
///
///     let client_config = SslConfig::default();
///     if let Ok(mut ssl_context_builder) = client_config.init_tls_client_context() {
///         let ssl = ssl_context_builder.build().configure().unwrap().into_ssl("localhost").unwrap();
///         let mut stream = SslStream::new(ssl, stream).unwrap();
///         Pin::new(&mut stream)
///             .connect()
///             .await
///             .map_err(|e| io::Error::other(format!("Can't connect the client: {e}")))?;
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
/// # #[cfg(feature="config-openssl")]
/// use openssl::ssl::{Ssl, SslVerifyMode};
/// use prosa_utils::config::ssl::{SslConfig, SslConfigContext};
///
/// # #[cfg(feature="config-openssl")]
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
///                 eprintln!("Can't accept the client {cli_addr}: {e}");
///                 continue;
///             }
///
///             // SSL stream ...
///         }
///     }
///
///     Ok(())
/// }
/// ```
#[derive(Clone, Deserialize, Serialize)]
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
    /// Changes of the certificate files, shared by the clones
    #[serde(skip)]
    reload: Arc<SslReload>,
}

/// Count of the changes of the files an [`SslConfig`] is read from
#[derive(Default)]
struct SslReload {
    /// Bumped by the watch, which holds it rather than the whole state so nothing is cyclic
    generation: Arc<AtomicU64>,
    /// Armed by the first [`SslConfig::generation`], [`None`] when there is nothing to watch
    watch: OnceLock<Option<FileWatch>>,
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
            reload: Arc::default(),
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
            reload: Arc::default(),
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
            reload: Arc::default(),
        }
    }

    /// Getter of the SSL timeout
    pub fn get_ssl_timeout(&self) -> Duration {
        Duration::from_millis(self.ssl_timeout)
    }

    /// Setter of the store certificate
    pub fn set_store(&mut self, store: Store) {
        self.store = Some(store);

        // Other files to watch, the clones keep watching the previous ones
        self.reload = Arc::default();
    }

    /// Setter of the ALPN list send by the client, or order of ALPN accepted by the server
    pub fn set_alpn(&mut self, alpn: Vec<String>) {
        self.alpn = alpn;
    }

    /// Method to get the files an SSL context is read from: the PKCS12, or the certificate and its
    /// key, and the store path, which can be a file, a directory or a glob pattern.
    ///
    /// A certificate without its key is left out: a server writes the certificate it signs there,
    /// it doesn't read it. So are the system store and inline certificates, which no file holds.
    ///
    /// ```
    /// use std::path::PathBuf;
    /// use prosa_utils::config::ssl::{SslConfig, Store};
    ///
    /// let mut config = SslConfig::new_cert_key("cert.pem".into(), "cert.key".into(), None);
    /// config.set_store(Store::File { path: "/etc/ssl/certs".into() });
    /// assert_eq!(
    ///     vec![PathBuf::from("cert.pem"), PathBuf::from("cert.key"), PathBuf::from("/etc/ssl/certs")],
    ///     config.watch_paths()
    /// );
    /// assert!(SslConfig::new_self_cert("cert.pem".into()).watch_paths().is_empty());
    /// ```
    pub fn watch_paths(&self) -> Vec<PathBuf> {
        let mut paths = Vec::new();
        if let Some(pkcs12) = &self.pkcs12 {
            paths.push(PathBuf::from(pkcs12));
        } else if let (Some(cert), Some(key)) = (&self.cert, &self.key) {
            paths.push(PathBuf::from(cert));
            paths.push(PathBuf::from(key));
        }

        if let Some(Store::File { path }) = &self.store {
            paths.push(PathBuf::from(path));
        }

        paths
    }

    /// Method to get the generation of the files of [`SslConfig::watch_paths`], which grows every
    /// time one of them changes. An SSL context built from the configuration is outdated once the
    /// generation differs from the one read before building it.
    ///
    /// The first call starts watching the files, for this configuration and its clones, until the
    /// last of them is dropped. The next calls only read an atomic, so it's cheap enough to call on
    /// every connection.
    ///
    /// ```
    /// use prosa_utils::config::ssl::SslConfig;
    ///
    /// let config = SslConfig::new_cert_key("cert.pem".into(), "cert.key".into(), None);
    /// let generation = config.generation();
    ///
    /// // Shared by the clones
    /// assert_eq!(generation, config.clone().generation());
    /// ```
    pub fn generation(&self) -> u64 {
        self.reload.watch.get_or_init(|| {
            let paths = self.watch_paths();
            if paths.is_empty() {
                return None;
            }

            let generation = self.reload.generation.clone();
            FileWatch::new(paths, move |_| {
                generation.fetch_add(1, Ordering::AcqRel);
            })
            .inspect_err(|err| log::warn!("Can't watch the certificates of {self:?}: {err}"))
            .ok()
        });

        self.reload.generation.load(Ordering::Acquire)
    }
}

impl PartialEq for SslConfig {
    fn eq(&self, other: &Self) -> bool {
        self.store == other.store
            && self.pkcs12 == other.pkcs12
            && self.cert == other.cert
            && self.key == other.key
            && self.passphrase == other.passphrase
            && self.alpn == other.alpn
            && self.modern_security == other.modern_security
            && self.ssl_timeout == other.ssl_timeout
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
            reload: Arc::default(),
        }
    }
}

impl fmt::Debug for SslConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SslConfig")
            .field("store", &self.store)
            .field("pkcs12", &self.pkcs12)
            .field("cert", &self.cert)
            .field("key", &self.key)
            .field("alpn", &self.alpn)
            .field("modern_security", &self.modern_security)
            .field("ssl_timeout", &self.ssl_timeout)
            .finish()
    }
}

/// SSL context that can be built from an [`SslConfig`], to be kept across connections
pub trait SslContextBuild: Clone + Send + Sync + 'static {
    /// Method to build the context from the files of the configuration. `host` is the name a
    /// server context is signed for when it has no certificate to serve
    fn build(config: &SslConfig, host: Option<&str>) -> io::Result<Self>;
}

#[cfg(feature = "config-openssl")]
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

        let config_store_le_x1_x2: Store = yaml_serde::from_str(
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
        .expect("SSL certificate configuration should be read");
        assert!(format!("{config_store_le_x1_x2}").contains("ISRG Root X"));

        let config_store_file: Store = yaml_serde::from_str("path: \"/opt\"")
            .expect("Certificate configuration path should be read");
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
        assert_eq!(
            "SslConfig { store: None, pkcs12: None, cert: None, key: None, alpn: [], modern_security: true, ssl_timeout: 3000 }",
            format!("{ssl_config:?}")
        );
        let ssl_acceptor = ssl_config
            .init_tls_server_context(None)
            .expect("The TLS server context should be init")
            .build();

        // Check for self signed certificate
        assert!(ssl_acceptor.context().private_key().is_some());
        assert!(ssl_acceptor.context().certificate().is_some());
    }

    #[test]
    fn ssl_config_debug_redacts_passphrase() {
        let ssl_config = SslConfig::new_cert_key(
            "cert.pem".into(),
            "key.pem".into(),
            Some("sensitive-passphrase".into()),
        );

        let debug = format!("{ssl_config:?}");
        assert!(debug.contains("cert.pem"));
        assert!(debug.contains("key.pem"));
        assert!(!debug.contains("sensitive-passphrase"));
        assert!(!debug.contains("passphrase"));
    }

    #[test]
    fn ssl_config_generation_follows_its_files() -> Result<(), Box<dyn std::error::Error>> {
        use std::{fs, time::Instant};

        let dir = std::env::temp_dir().join(format!(
            "prosa-ssl-generation-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        ));
        let store = dir.join("store");
        fs::create_dir_all(&store)?;
        let path = |path: &Path| path.to_string_lossy().into_owned();
        let (cert, key) = (dir.join("cert.pem"), dir.join("cert.key"));
        fs::write(&cert, "cert")?;
        fs::write(&key, "key")?;
        let mut config = SslConfig::new_cert_key(path(&cert), path(&key), None);
        config.set_store(Store::File { path: path(&store) });

        // Waits for the generation to move from `generation`, `None` if it doesn't
        let next = |config: &SslConfig, generation: u64| {
            let start = Instant::now();
            while start.elapsed() < Duration::from_secs(5) {
                if config.generation() != generation {
                    std::thread::sleep(Duration::from_millis(100));
                    return Some(config.generation());
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            None
        };

        let mut generation = config.generation();
        let clone = config.clone();
        for file in [&cert, &key, &store.join("ca.crt")] {
            fs::write(file, "rotated")?;
            generation = next(&config, generation)
                .ok_or_else(|| format!("{} wasn't seen changing", file.display()))?;
            assert_eq!(generation, clone.generation());
        }

        fs::write(dir.join("unrelated.log"), "not a certificate")?;
        assert_eq!(None, next(&config, generation));

        // Another store is other files, the clones keep the ones they had
        config.set_store(Store::System);
        let generation = config.generation();
        let clone_generation = clone.generation();
        fs::write(store.join("ca.crt"), "rotated again")?;
        assert!(next(&clone, clone_generation).is_some());
        assert_eq!(None, next(&config, generation));

        // A server writes the certificate it signs where a certificate without a key points, and
        // neither the system store nor inline certificates are files
        assert!(
            SslConfig::new_self_cert(path(&cert))
                .watch_paths()
                .is_empty()
        );
        let mut inline = SslConfig::default();
        inline.set_store(Store::Cert { certs: Vec::new() });
        assert!(inline.watch_paths().is_empty());

        fs::remove_dir_all(dir)?;
        Ok(())
    }
}
