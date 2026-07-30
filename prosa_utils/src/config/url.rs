//! URL authentication and safe-formatting utilities.

use base64::{Engine as _, engine::general_purpose::STANDARD};
use percent_encoding::percent_decode_str;
use std::fmt;

use ::url::{Position, Url};

static CREDENTIAL_MASK: &str = "***";

/// Borrowed URL view for safe logging and display.
///
/// [`Display`](fmt::Display) and [`Debug`](fmt::Debug) mask URL credentials and omit the query
/// and fragment without cloning the underlying [`Url`]. [`SafeUrl::to_url`] returns an owned URL
/// with credentials removed, while [`SafeUrl::to_mask_url`] returns one with masked credentials.
///
/// The URL path is preserved and can still contain sensitive information. Callers must avoid
/// putting secrets in paths or apply additional application-specific redaction.
#[derive(Clone, Copy)]
pub struct SafeUrl<'a> {
    url: &'a Url,
}

impl<'a> SafeUrl<'a> {
    /// Create a borrowed URL view that masks credentials and omits query and fragment when
    /// formatted.
    pub fn new(url: &'a Url) -> Self {
        Self { url }
    }

    /// Return an owned URL without credentials, query, or fragment.
    pub fn to_url(&self) -> Url {
        let mut url = self.url.clone();
        url.set_query(None);
        url.set_fragment(None);
        if !url.username().is_empty() {
            let _ = url.set_username("");
        }
        if url.password().is_some() {
            let _ = url.set_password(None);
        }

        url
    }

    /// Return an owned URL with masked credentials and without query or fragment.
    pub fn to_mask_url(&self) -> Url {
        let mut url = self.url.clone();
        url.set_query(None);
        url.set_fragment(None);
        if !url.username().is_empty() {
            let _ = url.set_username(CREDENTIAL_MASK);
        }
        if url.password().is_some() {
            let _ = url.set_password(Some(CREDENTIAL_MASK));
        }

        url
    }

    #[cfg(feature = "config-observability")]
    pub(crate) fn without_credentials(self) -> UrlWithoutCredentials<'a> {
        UrlWithoutCredentials { url: self.url }
    }
}

#[cfg(feature = "config-observability")]
#[derive(Clone, Copy)]
pub(crate) struct UrlWithoutCredentials<'a> {
    url: &'a Url,
}

#[cfg(feature = "config-observability")]
impl fmt::Display for UrlWithoutCredentials<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.url[..Position::BeforeUsername])?;
        f.write_str(&self.url[Position::BeforeHost..Position::AfterPath])
    }
}

#[cfg(feature = "config-observability")]
impl fmt::Debug for UrlWithoutCredentials<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

impl fmt::Display for SafeUrl<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.url[..Position::BeforeUsername])?;

        let has_username = !self.url.username().is_empty();
        let has_password = self.url.password().is_some();

        if has_username {
            f.write_str(CREDENTIAL_MASK)?;
        }
        if has_password {
            f.write_str(":")?;
            f.write_str(CREDENTIAL_MASK)?;
        }
        if has_username || has_password {
            f.write_str("@")?;
        }

        f.write_str(&self.url[Position::BeforeHost..Position::AfterPath])
    }
}

impl fmt::Debug for SafeUrl<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let username = if self.url.username().is_empty() {
            ""
        } else {
            CREDENTIAL_MASK
        };
        let password = self.url.password().map(|_| CREDENTIAL_MASK);

        f.debug_struct("Url")
            .field("scheme", &self.url.scheme())
            .field("cannot_be_a_base", &self.url.cannot_be_a_base())
            .field("username", &username)
            .field("password", &password)
            .field("host", &self.url.host())
            .field("port", &self.url.port())
            .field("path", &self.url.path())
            .finish()
    }
}

/// Return a borrowed URL view with masked credentials and without query or fragment.
///
/// Formatting the returned view does not clone or reparse the URL. Use [`SafeUrl::to_url`] when an
/// owned URL without credentials is required, or [`SafeUrl::to_mask_url`] when the owned URL must
/// retain masked user information.
///
/// ```
/// use prosa_utils::config::url::get_safe_url;
/// use url::Url;
///
/// let url =
///     Url::parse("https://admin:secret@localhost:4443/v1?token=secret#access_token=secret")
///         .unwrap();
/// let safe_url = get_safe_url(&url);
///
/// assert_eq!(safe_url.to_string(), "https://***:***@localhost:4443/v1");
/// assert_eq!(safe_url.to_url().as_str(), "https://localhost:4443/v1");
/// assert_eq!(safe_url.to_mask_url().as_str(), "https://***:***@localhost:4443/v1");
/// ```
pub fn get_safe_url(url: &Url) -> SafeUrl<'_> {
    SafeUrl::new(url)
}

/// Build an HTTP authorization value from URL credentials.
///
/// URL percent-encoding is decoded before constructing the authentication value. Basic
/// authentication usernames containing a decoded `:` are rejected because the character
/// separates the username and password. Bearer credentials that are not valid ASCII HTTP header
/// values are rejected.
///
/// - A non-empty username and password produce Basic authentication.
/// - An empty username and password produce Bearer authentication using the password as the token.
/// - A URL without a password produces `None`.
///
/// ```
/// use url::Url;
/// use prosa_utils::config::url::url_authentication;
///
/// let basic_auth_target = Url::parse("http://user:pass@localhost:8080").unwrap();
/// assert_eq!(Some(String::from("Basic dXNlcjpwYXNz")), url_authentication(&basic_auth_target));
///
/// let bearer_auth_target = Url::parse("http://:token@localhost:8080").unwrap();
/// assert_eq!(Some(String::from("Bearer token")), url_authentication(&bearer_auth_target));
/// ```
pub fn url_authentication(url: &Url) -> Option<String> {
    let password = url.password()?;

    if url.username().is_empty() {
        let password = percent_decode_str(password).decode_utf8().ok()?;
        if !password
            .bytes()
            .all(|byte| byte == b'\t' || (b' '..=b'~').contains(&byte))
        {
            return None;
        }
        Some(format!("Bearer {password}"))
    } else {
        let username = percent_decode_str(url.username()).collect::<Vec<_>>();
        if username.contains(&b':') {
            return None;
        }

        let mut credentials = Vec::with_capacity(username.len() + password.len() + 1);
        credentials.extend(username);
        credentials.push(b':');
        credentials.extend(percent_decode_str(password));
        Some(format!("Basic {}", STANDARD.encode(credentials)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_safe_url_display() {
        let url =
            Url::parse("https://admin:secret@localhost:4443/v1?token=secret#access_token=secret")
                .expect("URL should be valid");

        assert_eq!(
            "https://***:***@localhost:4443/v1",
            get_safe_url(&url).to_string()
        );
    }

    #[test]
    fn test_safe_url_debug() {
        let url =
            Url::parse("https://admin:secret@localhost:4443/v1?token=secret#access_token=secret")
                .expect("URL should be valid");

        assert_eq!(
            "Url { scheme: \"https\", cannot_be_a_base: false, username: \"***\", password: Some(\"***\"), host: Some(Domain(\"localhost\")), port: Some(4443), path: \"/v1\" }",
            format!("{:?}", get_safe_url(&url))
        );
    }

    #[test]
    fn test_safe_url_to_url() {
        let url =
            Url::parse("https://admin:secret@localhost:4443/v1?token=secret#access_token=secret")
                .expect("URL should be valid");

        assert_eq!(
            "https://localhost:4443/v1",
            get_safe_url(&url).to_url().as_str()
        );
        assert_eq!(
            "https://admin:secret@localhost:4443/v1?token=secret#access_token=secret",
            url.as_str()
        );
    }

    #[test]
    fn test_safe_url_to_mask_url() {
        let url =
            Url::parse("https://admin:secret@localhost:4443/v1?token=secret#access_token=secret")
                .expect("URL should be valid");

        assert_eq!(
            "https://***:***@localhost:4443/v1",
            get_safe_url(&url).to_mask_url().as_str()
        );
        assert_eq!(
            "https://admin:secret@localhost:4443/v1?token=secret#access_token=secret",
            url.as_str()
        );
    }

    #[test]
    fn test_safe_url_bearer_authentication() {
        let url = Url::parse("https://:token@localhost:4443/v1")
            .expect("Bearer authentication URL should be valid");

        assert_eq!(
            "https://:***@localhost:4443/v1",
            get_safe_url(&url).to_string()
        );
        assert_eq!(
            "Url { scheme: \"https\", cannot_be_a_base: false, username: \"\", password: Some(\"***\"), host: Some(Domain(\"localhost\")), port: Some(4443), path: \"/v1\" }",
            format!("{:?}", get_safe_url(&url))
        );
    }

    #[test]
    fn test_url_authentication_basic() {
        let basic_auth_target = Url::parse("http://user:pass@localhost:8080")
            .expect("Basic auth target URL should be valid");
        assert_eq!(
            Some(String::from("Basic dXNlcjpwYXNz")),
            url_authentication(&basic_auth_target)
        );
    }

    #[test]
    fn test_url_encoded_authentication_basic() {
        let basic_auth_target = Url::parse("http://us%40er:p%25%3A%C3%A4ss@localhost:8080")
            .expect("Basic auth target URL should be valid");
        assert_eq!(
            Some(format!("Basic {}", STANDARD.encode("us@er:p%:äss"))),
            url_authentication(&basic_auth_target)
        );
    }

    #[test]
    fn test_url_authentication_rejects_colon_in_basic_username() {
        let basic_auth_target = Url::parse("http://us%3Aer:password@localhost:8080")
            .expect("Basic auth target URL should be valid");
        assert_eq!(None, url_authentication(&basic_auth_target));
    }

    #[test]
    fn test_url_authentication_bearer() {
        let bearer_auth_target = Url::parse("http://:token%25%40%3A@localhost:8080")
            .expect("Bearer auth target URL should be valid");
        assert_eq!(
            Some(String::from("Bearer token%@:")),
            url_authentication(&bearer_auth_target)
        );
    }

    #[test]
    fn test_url_authentication_rejects_control_characters_in_bearer() {
        let bearer_auth_target = Url::parse("http://:token%0D%0AX-Test%3Ayes@localhost:8080")
            .expect("Bearer auth target URL should be valid");
        assert_eq!(None, url_authentication(&bearer_auth_target));
    }

    #[test]
    fn test_url_authentication_rejects_non_ascii_bearer() {
        let bearer_auth_target = Url::parse("http://:token%E2%9C%93@localhost:8080")
            .expect("Bearer auth target URL should be valid");
        assert_eq!(None, url_authentication(&bearer_auth_target));
    }

    #[test]
    fn test_url_authentication_rejects_non_utf8_bearer() {
        let bearer_auth_target = Url::parse("http://:%FF@localhost:8080")
            .expect("Bearer auth target URL should be valid");
        assert_eq!(None, url_authentication(&bearer_auth_target));
    }
}
