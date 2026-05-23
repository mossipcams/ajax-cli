//! Local HTTPS identity mechanisms.

use std::path::Path;
use std::sync::Arc;

use crate::WebError;

const CERT_FILE: &str = "web-tls-cert.pem";
const KEY_FILE: &str = "web-tls-key.pem";

/// A persisted self-signed certificate and its private key, PEM-encoded.
pub struct TlsIdentity {
    pub cert_pem: String,
    pub key_pem: String,
}

/// Loads the persisted TLS identity from `dir`, generating and persisting a
/// fresh self-signed identity when either file is missing or empty.
pub fn load_or_create_identity(dir: &Path) -> Result<TlsIdentity, WebError> {
    let cert_path = dir.join(CERT_FILE);
    let key_path = dir.join(KEY_FILE);

    if let (Ok(cert_pem), Ok(key_pem)) = (
        std::fs::read_to_string(&cert_path),
        std::fs::read_to_string(&key_path),
    ) {
        if !cert_pem.trim().is_empty() && !key_pem.trim().is_empty() {
            return Ok(TlsIdentity { cert_pem, key_pem });
        }
    }

    let identity = generate_identity()?;
    std::fs::create_dir_all(dir)
        .map_err(|error| WebError::CommandFailed(format!("web tls dir create failed: {error}")))?;
    write_private(&key_path, &identity.key_pem)?;
    std::fs::write(&cert_path, &identity.cert_pem)
        .map_err(|error| WebError::CommandFailed(format!("web tls cert write failed: {error}")))?;
    Ok(identity)
}

fn generate_identity() -> Result<TlsIdentity, WebError> {
    let mut subject_alt_names = vec!["localhost".to_string()];
    if let Some(ip) = primary_lan_ip() {
        subject_alt_names.push(ip);
    }
    let certified = rcgen::generate_simple_self_signed(subject_alt_names).map_err(|error| {
        WebError::CommandFailed(format!("web tls cert generation failed: {error}"))
    })?;
    Ok(TlsIdentity {
        cert_pem: certified.cert.pem(),
        key_pem: certified.key_pair.serialize_pem(),
    })
}

fn primary_lan_ip() -> Option<String> {
    let socket = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    Some(socket.local_addr().ok()?.ip().to_string())
}

/// Builds a rustls server configuration from a PEM-encoded identity.
pub fn tls_server_config(identity: &TlsIdentity) -> Result<Arc<rustls::ServerConfig>, WebError> {
    use rustls::pki_types::pem::PemObject;
    use rustls::pki_types::{CertificateDer, PrivateKeyDer};

    let certs = CertificateDer::pem_slice_iter(identity.cert_pem.as_bytes())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| WebError::CommandFailed(format!("web tls cert parse failed: {error}")))?;
    if certs.is_empty() {
        return Err(WebError::CommandFailed(
            "web tls identity has no certificate".to_string(),
        ));
    }
    let key = PrivateKeyDer::from_pem_slice(identity.key_pem.as_bytes())
        .map_err(|error| WebError::CommandFailed(format!("web tls key parse failed: {error}")))?;

    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = rustls::ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|error| {
            WebError::CommandFailed(format!("web tls protocol setup failed: {error}"))
        })?
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(|error| WebError::CommandFailed(format!("web tls config failed: {error}")))?;
    Ok(Arc::new(config))
}

/// Writes a secret file, restricting it to the owner on Unix.
pub fn write_private(path: &Path, contents: &str) -> Result<(), WebError> {
    std::fs::write(path, contents)
        .map_err(|error| WebError::CommandFailed(format!("web tls key write failed: {error}")))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).map_err(
            |error| WebError::CommandFailed(format!("web tls key chmod failed: {error}")),
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{load_or_create_identity, tls_server_config};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn scratch_dir(tag: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("ajax-web-tls-{tag}-{}-{nanos}", std::process::id()))
    }

    #[test]
    fn identity_is_generated_persisted_and_reused() {
        let dir = scratch_dir("reuse");

        let first = load_or_create_identity(&dir).unwrap();
        assert!(first.cert_pem.contains("BEGIN CERTIFICATE"));
        assert!(first.key_pem.contains("PRIVATE KEY"));
        assert!(dir.join("web-tls-cert.pem").exists());
        assert!(dir.join("web-tls-key.pem").exists());

        let second = load_or_create_identity(&dir).unwrap();
        assert_eq!(first.cert_pem, second.cert_pem);
        assert_eq!(first.key_pem, second.key_pem);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn server_config_builds_from_a_generated_identity() {
        let dir = scratch_dir("config");
        let identity = load_or_create_identity(&dir).unwrap();

        let config = tls_server_config(&identity);
        assert!(config.is_ok(), "{:?}", config.err());

        std::fs::remove_dir_all(&dir).ok();
    }
}
