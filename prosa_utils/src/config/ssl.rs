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
    x509::{extension::SubjectAlternativeName, X509NameBuilder, X509},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    ffi::OsStr,
    fmt, fs,
    ops::DerefMut,
    time::{self, Duration},
};

use super::{os_country, ConfigError};

/// SSL configuration object for store certificates
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct Store {
    path: String,
}

impl Store {
    /// Method to read a certificate from its path
    fn get_certificate(
        path: &std::path::PathBuf,
    ) -> Result<Option<openssl::x509::X509>, ConfigError> {
        if path.is_file() {
            match &path.extension().and_then(OsStr::to_str) {
                Some("pem") => match fs::read(path) {
                    Ok(pem_file) => Ok(Some(openssl::x509::X509::from_pem(&pem_file)?)),
                    Err(io) => Err(ConfigError::IoFile(
                        path.to_str().unwrap_or_default().into(),
                        io,
                    )),
                },
                Some("der") => match fs::read(path) {
                    Ok(der_file) => Ok(Some(openssl::x509::X509::from_der(&der_file)?)),
                    Err(io) => Err(ConfigError::IoFile(
                        path.to_str().unwrap_or_default().into(),
                        io,
                    )),
                },
                _ => Ok(None),
            }
        } else {
            Ok(None)
        }
    }

    /// Method to create an SSL Store configuration manually
    /// Should be use with config instead of building it manually
    pub fn new(path: String) -> Store {
        Store { path }
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
        match glob(&(self.path.clone() + "*")) {
            Ok(certs) => {
                let mut store = openssl::x509::store::X509StoreBuilder::new()?;
                for cert_path in certs.flatten() {
                    if let Some(cert) = Self::get_certificate(&cert_path)? {
                        store.add_cert(cert)?;
                    }
                }

                Ok(store.build())
            }
            Err(e) => Err(ConfigError::WrongPath(self.path.clone(), e)),
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
        match glob(&(self.path.clone() + "*")) {
            Ok(certs) => {
                let mut certs_map = HashMap::new();
                for cert_path in certs.flatten() {
                    if let Some(cert_path_name) = cert_path.to_str() {
                        if let Some(cert_name) = cert_path_name.strip_suffix(".pem") {
                            if let Some(cert) = Self::get_certificate(&cert_path)? {
                                certs_map.insert(cert_name.into(), cert);
                            }
                        } else if let Some(cert_name) = cert_path_name.strip_suffix(".der") {
                            if let Some(cert) = Self::get_certificate(&cert_path)? {
                                certs_map.insert(cert_name.into(), cert);
                            }
                        }
                    }
                }

                Ok(certs_map)
            }
            Err(e) => Err(ConfigError::WrongPath(self.path.clone(), e)),
        }
    }
}

#[cfg(target_family = "unix")]
impl Default for Store {
    fn default() -> Self {
        Store::new("/etc/ssl/certs/".into())
    }
}

#[cfg(target_family = "windows")]
impl Default for Store {
    fn default() -> Self {
        Store::new("HKLM:/Software/Microsoft/SystemCertificates/".into())
    }
}

impl fmt::Display for Store {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let certs = self.get_certs().unwrap_or_default();
        writeln!(f, "Store cert path [{}]:", self.path)?;
        for (name, cert) in certs {
            if f.alternate() {
                writeln!(f, "{}:\n{:#?}", name, cert)?;
            } else {
                writeln!(f, "{}", name)?;
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
///         let ssl_context = ssl_context_builder.build();
///         let ssl = Ssl::new(&ssl_context).unwrap();
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
///     if let Ok(mut ssl_context_builder) = server_config.init_tls_server_context() {
///         ssl_context_builder.set_verify(SslVerifyMode::NONE);
///         let ssl_context = ssl_context_builder.build();
///
///         loop {
///             let (stream, cli_addr) = listener.accept().await?;
///             let ssl = Ssl::new(&ssl_context).unwrap();
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
        domain: Option<&str>,
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

            // Add DNS subject alternative name if needed to check the certificate
            if let Some(dns) = domain {
                let mut subject_alternative_name = SubjectAlternativeName::new();
                let x509_extension = subject_alternative_name
                    .dns(dns)
                    .build(&cert.x509v3_context(None, None))?;
                cert.append_extension2(&x509_extension)?;
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
                            cli_alpn.insert(String::from_utf8(left[1..].to_vec()).unwrap(), left);
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
    /// if let Ok(mut ssl_context_builder) = server_config.init_tls_server_context() {
    ///     let ssl_context = ssl_context_builder.build();
    /// }
    /// ```
    pub fn init_tls_server_context(
        &self,
        domain: Option<&str>,
    ) -> Result<openssl::ssl::SslAcceptorBuilder, ConfigError> {
        let ssl_acceptor = if self.modern_security {
            openssl::ssl::SslAcceptor::mozilla_modern_v5(SslMethod::tls_server())
        } else {
            openssl::ssl::SslAcceptor::mozilla_intermediate_v5(SslMethod::tls_server())
        }?;
        self.init_tls_context(ssl_acceptor, true, domain)
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
    fn test_tls_server_context() {
        let ssl_config = SslConfig::default();
        let ssl_acceptor = ssl_config.init_tls_server_context(None).unwrap().build();

        // Check for self signed certificate
        assert!(ssl_acceptor.context().private_key().is_some());
        assert!(ssl_acceptor.context().certificate().is_some());
    }
}
