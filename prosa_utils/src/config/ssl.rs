//! Definition of SSL configuration

use bytes::{BufMut, BytesMut};
use glob::glob;
use openssl::{
    asn1::{Asn1Integer, Asn1Time},
    bn::{BigNum, MsbOption},
    ec::{Asn1Flag, EcGroup, EcKey},
    hash::MessageDigest,
    nid::Nid,
    pkey::PKey,
    ssl::{AlpnError, SslContextBuilder, SslFiletype, SslMethod, SslVerifyMode},
    x509::{X509, X509NameBuilder, extension::SubjectAlternativeName},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    ffi::OsStr,
    fmt, fs,
    net::IpAddr,
    ops::DerefMut,
    time::{self, Duration},
};

use super::{ConfigError, os_country};

/// SSL configuration object for store certificates
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(untagged)]
pub enum Store {
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

impl Store {
    /// Method to read certificates from its path. Get all certificates in subfolders
    fn get_file_certificates(
        path: &std::path::PathBuf,
    ) -> Result<Vec<openssl::x509::X509>, ConfigError> {
        if path.is_file() {
            match &path.extension().and_then(OsStr::to_str) {
                Some("pem") => match fs::read(path) {
                    Ok(pem_file) => Ok(vec![openssl::x509::X509::from_pem(&pem_file)?]),
                    Err(io) => Err(ConfigError::IoFile(
                        path.to_str().unwrap_or_default().into(),
                        io,
                    )),
                },
                Some("der") => match fs::read(path) {
                    Ok(der_file) => Ok(vec![openssl::x509::X509::from_der(&der_file)?]),
                    Err(io) => Err(ConfigError::IoFile(
                        path.to_str().unwrap_or_default().into(),
                        io,
                    )),
                },
                _ => Ok(Vec::new()),
            }
        } else if path.is_symlink() {
            if let Ok(link) = path.read_link() {
                Self::get_file_certificates(&link)
            } else {
                Ok(Vec::new())
            }
        } else if let Ok(path_dir) = path.read_dir() {
            let mut cert_list = Vec::new();
            for dir_entry in path_dir.flatten() {
                cert_list.append(&mut Self::get_file_certificates(&dir_entry.path())?);
            }

            Ok(cert_list)
        } else {
            Ok(Vec::new())
        }
    }

    /// Method to get an OpenSSL cert store
    ///
    /// ```
    /// use prosa_utils::config::ssl::Store;
    ///
    /// let store = Store::new("./target".into());
    /// let openssl_store: openssl::x509::store::X509Store = store.get_store().unwrap();
    /// ```
    pub fn get_store(&self) -> Result<openssl::x509::store::X509Store, ConfigError> {
        let mut store = openssl::x509::store::X509StoreBuilder::new()?;
        match self {
            Store::File { path } => match glob(path) {
                Ok(certs) => {
                    for cert_path in certs.flatten() {
                        for cert in Self::get_file_certificates(&cert_path)? {
                            store.add_cert(cert)?;
                        }
                    }

                    Ok(store.build())
                }
                Err(e) => Err(ConfigError::WrongPath(path.clone(), e)),
            },
            Store::Cert { certs } => {
                for cert in certs {
                    store.add_cert(openssl::x509::X509::from_pem(cert.as_bytes())?)?;
                }

                Ok(store.build())
            }
        }
    }

    /// Method to get all OpenSSL certificate with their names as key
    ///
    /// ```
    /// use prosa_utils::config::ssl::Store;
    ///
    /// let store = Store::new("./target".into());
    /// let certs_map = store.get_certs().unwrap();
    ///
    /// // No cert in target
    /// assert!(certs_map.is_empty());
    /// ```
    pub fn get_certs(&self) -> Result<HashMap<String, openssl::x509::X509>, ConfigError> {
        match self {
            Store::File { path } => match glob(path) {
                Ok(certs) => {
                    let mut certs_map = HashMap::new();
                    for cert_path in certs.flatten() {
                        for cert in Self::get_file_certificates(&cert_path)? {
                            if let Some(name) = cert
                                .subject_name()
                                .entries_by_nid(Nid::COMMONNAME)
                                .last()
                                .and_then(|cn| cn.data().as_utf8().map(|cn| cn.to_string()).ok())
                            {
                                certs_map.insert(name, cert);
                            } else if let Some(cert_path_name) = cert_path.to_str() {
                                if let Some(cert_name) = cert_path_name
                                    .strip_suffix(".pem")
                                    .or(cert_path_name.strip_suffix(".der"))
                                {
                                    certs_map.insert(cert_name.into(), cert);
                                }
                            }
                        }
                    }

                    Ok(certs_map)
                }
                Err(e) => Err(ConfigError::WrongPath(path.clone(), e)),
            },
            Store::Cert { certs } => {
                let mut certs_map = HashMap::new();
                for cert_pem in certs {
                    let cert = openssl::x509::X509::from_pem(cert_pem.as_bytes())?;
                    if let Some(name) = cert
                        .subject_name()
                        .entries_by_nid(Nid::COMMONNAME)
                        .last()
                        .and_then(|cn| cn.data().as_utf8().map(|cn| cn.to_string()).ok())
                    {
                        certs_map.insert(name, cert);
                    }
                }

                Ok(certs_map)
            }
        }
    }
}

#[cfg(target_family = "unix")]
impl Default for Store {
    fn default() -> Self {
        Store::File {
            path: "/etc/ssl/certs/".into(),
        }
    }
}

#[cfg(target_family = "windows")]
impl Default for Store {
    fn default() -> Self {
        Store::File {
            path: "HKLM:/Software/Microsoft/SystemCertificates/".into(),
        }
    }
}

impl fmt::Display for Store {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let certs = self.get_certs().unwrap_or_default();
        if let Self::File { path } = &self {
            writeln!(f, "Store cert path [{}]:", path)?;
        }
        for (name, cert) in certs {
            if f.alternate() {
                writeln!(f, "{name}:\n{cert:#?}")?;
            } else {
                writeln!(f, "{name}")?;
            }
        }

        Ok(())
    }
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
/// use prosa_utils::config::ssl::SslConfig;
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
/// use prosa_utils::config::ssl::SslConfig;
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
    modern_security: bool,
    #[serde(skip_serializing)]
    #[serde(default = "SslConfig::default_ssl_timeout")]
    /// SSL operation timeout
    ssl_timeout: u64,
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

    /// Method to init an SSL context for a socket
    pub(crate) fn init_tls_context<B>(
        &self,
        mut context_builder: B,
        is_server: bool,
        host: Option<&str>,
    ) -> Result<B, ConfigError>
    where
        B: DerefMut<Target = SslContextBuilder>,
    {
        if let Some(pkcs12_path) = &self.pkcs12 {
            match fs::read(pkcs12_path) {
                Ok(pkcs12_file) => {
                    let pkcs12 = openssl::pkcs12::Pkcs12::from_der(pkcs12_file.as_ref())?
                        .parse2(self.passphrase.as_ref().unwrap_or(&String::from("")))?;

                    if let Some(pkey) = pkcs12.pkey {
                        context_builder.set_private_key(&pkey)?;
                    }

                    if let Some(cert) = pkcs12.cert {
                        context_builder.set_certificate(&cert)?;
                    }

                    if let Some(ca) = pkcs12.ca {
                        for cert in ca {
                            context_builder.add_extra_chain_cert(cert)?;
                        }
                    }
                }
                Err(io) => return Err(ConfigError::IoFile(pkcs12_path.to_string(), io)),
            }
        } else if let (Some(cert_path), Some(key_path)) = (&self.cert, &self.key) {
            context_builder.set_certificate_file(cert_path, SslFiletype::PEM)?;

            match fs::read(key_path) {
                Ok(key_file) => {
                    let pkey = if key_path.ends_with(".der") {
                        PKey::private_key_from_der(key_file.as_slice())?
                    } else if let Some(passphrase) = &self.passphrase {
                        PKey::private_key_from_pem_passphrase(
                            key_file.as_slice(),
                            passphrase.as_bytes(),
                        )?
                    } else {
                        PKey::private_key_from_pem(key_file.as_slice())?
                    };

                    context_builder.set_private_key(&pkey)?;
                }
                Err(io) => return Err(ConfigError::IoFile(key_path.to_string(), io)),
            }
        } else if is_server {
            let mut group = EcGroup::from_curve_name(Nid::X9_62_PRIME256V1)?;
            group.set_asn1_flag(Asn1Flag::NAMED_CURVE);
            let pkey = PKey::from_ec_key(EcKey::generate(&group)?)?;
            context_builder.set_private_key(&pkey)?;

            let mut cert = X509::builder()?;
            cert.set_version(2)?;
            cert.set_pubkey(&pkey)?;

            let mut serial_bn = BigNum::new()?;
            serial_bn.pseudo_rand(64, MsbOption::MAYBE_ZERO, true)?;
            let serial_number = Asn1Integer::from_bn(&serial_bn)?;
            cert.set_serial_number(&serial_number)?;

            let begin_valid_time =
                Asn1Time::from_unix(time::UNIX_EPOCH.elapsed().unwrap().as_secs() as i64 - 360)?;
            cert.set_not_before(&begin_valid_time)?;
            let end_valid_time = Asn1Time::days_from_now(1461)?; // 4 years from now
            cert.set_not_after(&end_valid_time)?;

            let mut x509_name = X509NameBuilder::new()?;
            if let Some(cn) = os_country() {
                x509_name.append_entry_by_text("C", cn.as_str())?;
            }
            x509_name.append_entry_by_text("CN", "ProSA")?;
            let x509_name = x509_name.build();
            cert.set_subject_name(&x509_name)?;
            cert.set_issuer_name(&x509_name)?;

            // Add DNS or IP subject alternative name if needed to check the certificate
            if let Some(host) = host {
                if let Ok(ip) = host.parse::<IpAddr>() {
                    if !ip.is_unspecified() && !ip.is_loopback() {
                        let mut subject_alternative_name = SubjectAlternativeName::new();
                        let x509_extension = subject_alternative_name
                            .ip(host)
                            .build(&cert.x509v3_context(None, None))?;
                        cert.append_extension2(&x509_extension)?;
                    }
                } else {
                    let mut subject_alternative_name = SubjectAlternativeName::new();
                    let x509_extension = subject_alternative_name
                        .dns(host)
                        .build(&cert.x509v3_context(None, None))?;
                    cert.append_extension2(&x509_extension)?;
                }
            }

            cert.sign(&pkey, MessageDigest::sha256())?;

            context_builder.set_certificate(&cert.build())?;
        }

        if let Some(store) = &self.store {
            context_builder.set_cert_store(store.get_store()?);
            if is_server {
                context_builder.set_verify(SslVerifyMode::PEER);
            }
        } else if !is_server {
            context_builder.set_cert_store(Store::default().get_store()?);
        } else {
            context_builder.set_verify(SslVerifyMode::NONE);
        }

        if !self.alpn.is_empty() {
            if is_server {
                let alpn_list = self.alpn.clone();
                context_builder.set_alpn_select_callback(move |_ssl, alpn| {
                    let mut cli_alpn = HashMap::new();

                    let mut current_split = alpn;
                    while let Some(length) = current_split.first() {
                        if current_split.len() > *length as usize {
                            let (left, right) = current_split.split_at(*length as usize + 1);
                            cli_alpn
                                .insert(String::from_utf8(left[1..].to_vec()).unwrap(), &left[1..]);
                            current_split = right;
                        } else {
                            return Err(AlpnError::ALERT_FATAL);
                        }
                    }

                    for alpn_name in &alpn_list {
                        if let Some(alpn) = cli_alpn.get(alpn_name) {
                            return Ok(alpn);
                        }
                    }

                    Err(AlpnError::NOACK)
                });
            } else {
                let mut alpn_bytes = BytesMut::new();
                for alpn in &self.alpn {
                    alpn_bytes.put_u8(alpn.len() as u8);
                    alpn_bytes.put(alpn.as_bytes());
                }

                context_builder.set_alpn_protos(&alpn_bytes)?;
            }
        }

        Ok(context_builder)
    }

    /// Method to init an SSL context for a client socket
    ///
    /// ```
    /// use prosa_utils::config::ssl::{Store, SslConfig};
    ///
    /// let mut client_config = SslConfig::default();
    /// client_config.set_store(Store::new("./target".into()));
    /// if let Ok(mut ssl_context_builder) = client_config.init_tls_client_context() {
    ///     let ssl_context = ssl_context_builder.build();
    /// }
    /// ```
    pub fn init_tls_client_context(
        &self,
    ) -> Result<openssl::ssl::SslConnectorBuilder, ConfigError> {
        self.init_tls_context(
            openssl::ssl::SslConnector::builder(SslMethod::tls_client())?,
            false,
            None,
        )
    }

    /// Method to init an SSL context for a server socket
    ///
    /// ```
    /// use prosa_utils::config::ssl::SslConfig;
    ///
    /// let server_config = SslConfig::new_pkcs12("server.pkcs12".into());
    /// if let Ok(mut ssl_context_builder) = server_config.init_tls_server_context(None) {
    ///     let ssl_context = ssl_context_builder.build();
    /// }
    /// ```
    pub fn init_tls_server_context(
        &self,
        host: Option<&str>,
    ) -> Result<openssl::ssl::SslAcceptorBuilder, ConfigError> {
        let ssl_acceptor = if self.modern_security {
            openssl::ssl::SslAcceptor::mozilla_modern_v5(SslMethod::tls_server())
        } else {
            openssl::ssl::SslAcceptor::mozilla_intermediate_v5(SslMethod::tls_server())
        }?;
        self.init_tls_context(ssl_acceptor, true, host)
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
