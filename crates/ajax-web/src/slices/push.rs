//! Prototype declarative Web Push delivery for the Settings test action.

use axum::http::{header, uri::Authority, HeaderMap, Request, Uri};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use serde::Deserialize;
use serde_json::json;
use std::{process::Stdio, sync::OnceLock};
use tokio::{io::AsyncWriteExt, process::Command};
use web_push_native::{
    jwt_simple::algorithms::ES256KeyPair,
    p256::{elliptic_curve::sec1::ToEncodedPoint, PublicKey},
    Auth, WebPushBuilder,
};

/// Process-local VAPID private key bytes. Settings test re-subscribes on each
/// click, so disk persistence is unnecessary and would trip CodeQL path-injection
/// on the operator-owned `state_dir` the same way a request-tainted path would.
static VAPID_PRIVATE_KEY: OnceLock<Vec<u8>> = OnceLock::new();

#[derive(Debug, Deserialize)]
pub(crate) struct PushSubscription {
    endpoint: String,
    keys: PushSubscriptionKeys,
}

#[derive(Debug, Deserialize)]
struct PushSubscriptionKeys {
    p256dh: String,
    auth: String,
}

pub(crate) fn vapid_public_key_base64() -> Result<String, String> {
    let key_pair = vapid_key_pair()?;
    Ok(URL_SAFE_NO_PAD.encode(vapid_public_key(&key_pair)))
}

pub(crate) async fn send_declarative_test_push(
    headers: &HeaderMap,
    subscription: PushSubscription,
) -> Result<(), String> {
    let key_pair = vapid_key_pair()?;
    let navigate = navigation_url(headers);
    // Apple rejects VAPID JWT `sub` values like mailto:…@localhost (403 BadJwtToken).
    // An https origin URL is a valid subject claim.
    let vapid_subject = navigate.trim_end_matches('/').to_string();
    let request = build_push_request(
        subscription,
        declarative_payload(&navigate),
        &key_pair,
        &vapid_subject,
    )?;
    deliver_with_curl(request).await
}

fn vapid_key_pair() -> Result<ES256KeyPair, String> {
    let bytes = VAPID_PRIVATE_KEY.get_or_init(|| ES256KeyPair::generate().to_bytes());
    ES256KeyPair::from_bytes(bytes).map_err(|error| format!("invalid VAPID key: {error}"))
}

fn vapid_public_key(key_pair: &ES256KeyPair) -> Vec<u8> {
    PublicKey::from_sec1_bytes(&key_pair.public_key().to_bytes())
        .expect("ES256 key pair always has a valid public key")
        .to_encoded_point(false)
        .as_bytes()
        .to_vec()
}

fn declarative_payload(navigate: &str) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "web_push": 8030,
        "notification": {
            "title": "Ajax Cockpit",
            "lang": "en-US",
            "dir": "ltr",
            "body": "Declarative push prototype",
            "navigate": navigate,
            "silent": false
        }
    }))
    .expect("fixed declarative push payload is serializable")
}

fn navigation_url(headers: &HeaderMap) -> String {
    if let Some(authority) = headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
        .and_then(|origin| origin.parse::<Uri>().ok())
        .and_then(|uri| uri.authority().cloned())
    {
        return format!("https://{authority}/");
    }
    if let Some(authority) = headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .and_then(|host| host.parse::<Authority>().ok())
    {
        return format!("https://{authority}/");
    }
    "https://localhost/".to_string()
}

fn build_push_request(
    subscription: PushSubscription,
    payload: Vec<u8>,
    vapid: &ES256KeyPair,
    vapid_subject: &str,
) -> Result<Request<Vec<u8>>, String> {
    let endpoint = subscription
        .endpoint
        .parse::<Uri>()
        .map_err(|error| format!("invalid push endpoint: {error}"))?;
    if endpoint.scheme_str() != Some("https") || endpoint.authority().is_none() {
        return Err("push endpoint must be an absolute https URL".to_string());
    }
    let public_bytes = URL_SAFE_NO_PAD
        .decode(subscription.keys.p256dh)
        .map_err(|error| format!("invalid subscription p256dh key: {error}"))?;
    let public_key = PublicKey::from_sec1_bytes(&public_bytes)
        .map_err(|error| format!("invalid subscription p256dh key: {error}"))?;
    let auth_bytes = URL_SAFE_NO_PAD
        .decode(subscription.keys.auth)
        .map_err(|error| format!("invalid subscription auth key: {error}"))?;
    let auth: [u8; 16] = auth_bytes
        .try_into()
        .map_err(|_| "subscription auth key must decode to 16 bytes".to_string())?;

    WebPushBuilder::new(endpoint, public_key, Auth::from(auth))
        .with_vapid(vapid, vapid_subject)
        .build(payload)
        .map_err(|error| format!("build encrypted push request: {error}"))
}

async fn deliver_with_curl(request: Request<Vec<u8>>) -> Result<(), String> {
    let (parts, body) = request.into_parts();
    let mut command = Command::new("curl");
    command
        .args(["-sS", "--fail", "--max-time", "10", "-X"])
        .arg(parts.method.as_str());
    for (name, value) in &parts.headers {
        command.arg("-H").arg(format!(
            "{name}: {}",
            value
                .to_str()
                .map_err(|error| { format!("invalid push request header: {error}") })?
        ));
    }
    let mut child = command
        .args(["--data-binary", "@-"])
        .arg(parts.uri.to_string())
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("start curl push delivery: {error}"))?;
    child
        .stdin
        .take()
        .ok_or_else(|| "open curl stdin for push delivery".to_string())?
        .write_all(&body)
        .await
        .map_err(|error| format!("write encrypted push payload to curl: {error}"))?;
    let output = child
        .wait_with_output()
        .await
        .map_err(|error| format!("wait for curl push delivery: {error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        let detail = String::from_utf8_lossy(&output.stderr);
        Err(format!(
            "push delivery failed with {}: {}",
            output.status,
            detail.trim()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use web_push_native::jwt_simple::algorithms::ES256KeyPair;

    const P256DH: &str =
        "BLn9b-VR0ca83knDNZ32dCHGyjJp-1riX9ZTN40MqV8K_LpQmLqxC_DoHvqvFXO_nGdAB4W9dogZb_sM-uV4JbY";
    const AUTH: &str = "_ordMnz7uTCmrpBTeUV4Bw";

    fn subscription(auth: &str) -> PushSubscription {
        PushSubscription {
            endpoint: "https://push.example/messages/1".to_string(),
            keys: PushSubscriptionKeys {
                p256dh: P256DH.to_string(),
                auth: auth.to_string(),
            },
        }
    }

    #[test]
    fn vapid_key_and_public_key_are_stable_in_process() {
        let first = vapid_key_pair().unwrap();
        let second = vapid_key_pair().unwrap();

        assert_eq!(first.to_bytes(), second.to_bytes());
        assert_eq!(vapid_public_key(&first).len(), 65);
        assert_eq!(
            vapid_public_key_base64().unwrap(),
            URL_SAFE_NO_PAD.encode(vapid_public_key(&first))
        );
    }

    #[test]
    fn payload_has_declarative_shape_and_https_navigation() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::ORIGIN,
            "http://cockpit.example:8443".parse().unwrap(),
        );
        let payload: Value =
            serde_json::from_slice(&declarative_payload(&navigation_url(&headers))).unwrap();

        assert_eq!(
            payload,
            json!({
                "web_push": 8030,
                "notification": {
                    "title": "Ajax Cockpit",
                    "lang": "en-US",
                    "dir": "ltr",
                    "body": "Declarative push prototype",
                    "navigate": "https://cockpit.example:8443/",
                    "silent": false
                }
            })
        );
        assert_eq!(navigation_url(&HeaderMap::new()), "https://localhost/");
    }

    #[test]
    fn valid_subscription_builds_encrypted_request_and_invalid_auth_is_rejected() {
        let vapid = ES256KeyPair::generate();
        let request = build_push_request(
            subscription(AUTH),
            declarative_payload("https://localhost/"),
            &vapid,
            "https://cockpit.example",
        )
        .unwrap();

        assert_eq!(request.method(), "POST");
        assert_eq!(request.uri(), "https://push.example/messages/1");
        assert_eq!(request.headers()[header::CONTENT_ENCODING], "aes128gcm");
        assert!(!request.body().is_empty());
        assert!(build_push_request(
            subscription("bad"),
            Vec::new(),
            &vapid,
            "https://cockpit.example"
        )
        .is_err());
    }
}
