use axum::http::{header, HeaderMap, HeaderValue};
use std::{fs, path::Path};

use crate::WebError;

const BROWSER_SESSION_COOKIE_NAME: &str = "ajax_browser_session";
const BROWSER_SESSION_TOKEN_FILE: &str = "web-browser-session-token";

#[derive(Clone)]
pub(crate) struct BrowserSession {
    token: String,
}

impl BrowserSession {
    fn new(token: impl Into<String>) -> Self {
        Self {
            token: token.into(),
        }
    }

    #[cfg(test)]
    fn new_for_test(token: impl Into<String>) -> Self {
        Self::new(token)
    }

    pub(crate) fn test_default() -> Self {
        Self::new("ajax-test-browser-session")
    }

    pub(crate) fn load_or_create(dir: &Path) -> Result<Self, WebError> {
        let token_path = dir.join(BROWSER_SESSION_TOKEN_FILE);
        if let Ok(token) = fs::read_to_string(&token_path) {
            let token = token.trim();
            if is_valid_session_token(token) {
                return Ok(Self::new(token.to_string()));
            }
        }

        fs::create_dir_all(dir).map_err(|error| {
            WebError::CommandFailed(format!("web session dir create failed: {error}"))
        })?;
        let token = generate_session_token()?;
        write_private_session_token(&token_path, &token)?;
        Ok(Self::new(token))
    }

    #[cfg(test)]
    fn token(&self) -> &str {
        &self.token
    }

    pub(crate) fn cookie_pair(&self) -> String {
        format!("{BROWSER_SESSION_COOKIE_NAME}={}", self.token)
    }

    fn set_cookie_value(&self) -> String {
        format!(
            "{}; Path=/; HttpOnly; Secure; SameSite=Strict",
            self.cookie_pair()
        )
    }

    pub(crate) fn apply_set_cookie(&self, headers: &mut HeaderMap) {
        if let Ok(value) = HeaderValue::from_str(&self.set_cookie_value()) {
            headers.insert(header::SET_COOKIE, value);
        }
    }

    pub(crate) fn is_present(&self, headers: &HeaderMap) -> bool {
        let expected = self.cookie_pair();
        headers.get_all(header::COOKIE).iter().any(|value| {
            value.to_str().ok().is_some_and(|cookies| {
                cookies
                    .split(';')
                    .map(str::trim)
                    .any(|cookie| cookie == expected)
            })
        })
    }
}

fn generate_session_token() -> Result<String, WebError> {
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes).map_err(|error| {
        WebError::CommandFailed(format!("web session token generation failed: {error}"))
    })?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn is_valid_session_token(token: &str) -> bool {
    token.len() == 64 && token.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn write_private_session_token(path: &Path, token: &str) -> Result<(), WebError> {
    fs::write(path, format!("{token}\n")).map_err(|error| {
        WebError::CommandFailed(format!("web session token write failed: {error}"))
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).map_err(|error| {
            WebError::CommandFailed(format!("web session token chmod failed: {error}"))
        })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use axum::http::{header, HeaderMap, HeaderValue};

    use super::{BrowserSession, BROWSER_SESSION_TOKEN_FILE};

    fn scratch_dir(tag: &str) -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "ajax-web-browser-session-{tag}-{}-{nanos}",
            std::process::id()
        ))
    }

    #[test]
    fn browser_session_persists_private_token_and_reuses_it() {
        let dir = scratch_dir("persist-reuse");
        let token_path = dir.join(BROWSER_SESSION_TOKEN_FILE);

        let first = BrowserSession::load_or_create(&dir).unwrap();
        let saved = std::fs::read_to_string(&token_path).unwrap();
        let second = BrowserSession::load_or_create(&dir).unwrap();

        assert_eq!(saved.trim(), first.token());
        assert_eq!(second.cookie_pair(), first.cookie_pair());
        assert_eq!(first.token().len(), 64);
        assert!(first.token().bytes().all(|byte| byte.is_ascii_hexdigit()));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&token_path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn browser_session_replaces_invalid_saved_token() {
        let dir = scratch_dir("invalid-token");
        std::fs::create_dir_all(&dir).unwrap();
        let token_path = dir.join(BROWSER_SESSION_TOKEN_FILE);
        std::fs::write(&token_path, "not-a-valid-token\n").unwrap();

        let session = BrowserSession::load_or_create(&dir).unwrap();
        let saved = std::fs::read_to_string(&token_path).unwrap();

        assert_ne!(saved.trim(), "not-a-valid-token");
        assert_eq!(saved.trim(), session.token());
        assert_eq!(session.token().len(), 64);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn browser_session_cookie_contract_is_strict_and_secure() {
        let session = BrowserSession::new_for_test("0123456789abcdef");
        let mut headers = HeaderMap::new();

        session.apply_set_cookie(&mut headers);

        let cookie = headers.get(header::SET_COOKIE).unwrap().to_str().unwrap();
        assert_eq!(
            cookie,
            "ajax_browser_session=0123456789abcdef; Path=/; HttpOnly; Secure; SameSite=Strict"
        );

        let mut request_headers = HeaderMap::new();
        request_headers.insert(
            header::COOKIE,
            HeaderValue::from_static(
                "other=value; ajax_browser_session=0123456789abcdef; another=value",
            ),
        );
        assert!(session.is_present(&request_headers));

        request_headers.insert(
            header::COOKIE,
            HeaderValue::from_static("other=value; ajax_browser_session=wrong"),
        );
        assert!(!session.is_present(&request_headers));
    }
}
