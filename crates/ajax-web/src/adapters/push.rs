//! Web Push subscription and delivery mechanisms.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::{adapters::tls::write_private, WebError};

const VAPID_FILE: &str = "web-push-vapid.pem";
const SUBSCRIPTIONS_FILE: &str = "web-push-subscriptions.json";

/// A browser push subscription, matching the JSON shape of the browser's
/// `PushSubscription.toJSON()`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PushSubscription {
    pub endpoint: String,
    pub keys: PushKeys,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PushKeys {
    pub p256dh: String,
    pub auth: String,
}

/// Loads stored push subscriptions from `dir`, treating a missing or
/// unreadable file as an empty set.
pub fn load_subscriptions(dir: &Path) -> Vec<PushSubscription> {
    match std::fs::read(dir.join(SUBSCRIPTIONS_FILE)) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

fn save_subscriptions(dir: &Path, subscriptions: &[PushSubscription]) -> Result<(), WebError> {
    std::fs::create_dir_all(dir).map_err(|error| {
        WebError::CommandFailed(format!("web push subscription dir create failed: {error}"))
    })?;
    let json = serde_json::to_vec_pretty(subscriptions)
        .map_err(|error| WebError::JsonSerialization(error.to_string()))?;
    std::fs::write(dir.join(SUBSCRIPTIONS_FILE), json).map_err(|error| {
        WebError::CommandFailed(format!("web push subscription write failed: {error}"))
    })
}

/// Stores a push subscription, replacing any existing entry with the same
/// endpoint so repeated subscribe calls stay idempotent.
pub fn add_subscription(dir: &Path, subscription: PushSubscription) -> Result<(), WebError> {
    let mut subscriptions = load_subscriptions(dir);
    subscriptions.retain(|existing| existing.endpoint != subscription.endpoint);
    subscriptions.push(subscription);
    save_subscriptions(dir, &subscriptions)
}

/// Removes the subscription with the given endpoint, if present.
pub fn remove_subscription(dir: &Path, endpoint: &str) -> Result<(), WebError> {
    let mut subscriptions = load_subscriptions(dir);
    subscriptions.retain(|existing| existing.endpoint != endpoint);
    save_subscriptions(dir, &subscriptions)
}

/// The server's VAPID identity. The PEM private key signs push requests; the
/// raw public key bytes are handed to the browser as the application server
/// key when it subscribes.
pub struct VapidKeys {
    pub private_pem: String,
    pub public_key: Vec<u8>,
}

/// Loads the persisted VAPID identity from `dir`, generating and persisting a
/// fresh P-256 keypair when the file is missing or empty.
pub fn load_or_create_vapid_keys(dir: &Path) -> Result<VapidKeys, WebError> {
    let path = dir.join(VAPID_FILE);

    let private_pem = match std::fs::read_to_string(&path) {
        Ok(pem) if !pem.trim().is_empty() => pem,
        _ => {
            let pem = generate_vapid_pem()?;
            std::fs::create_dir_all(dir).map_err(|error| {
                WebError::CommandFailed(format!("web push key dir create failed: {error}"))
            })?;
            write_private(&path, &pem)?;
            pem
        }
    };

    let public_key = derive_public_key(&private_pem)?;
    Ok(VapidKeys {
        private_pem,
        public_key,
    })
}

fn generate_vapid_pem() -> Result<String, WebError> {
    let key = rcgen::KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256).map_err(|error| {
        WebError::CommandFailed(format!("web push key generation failed: {error}"))
    })?;
    Ok(key.serialize_pem())
}

fn derive_public_key(private_pem: &str) -> Result<Vec<u8>, WebError> {
    let builder = web_push::VapidSignatureBuilder::from_pem_no_sub(private_pem.as_bytes())
        .map_err(|error| {
            WebError::CommandFailed(format!("web push key is not a valid VAPID key: {error}"))
        })?;
    Ok(builder.get_public_key())
}

/// A notification to deliver to subscribed browsers. The fields are serialized
/// into the encrypted push payload and read back by the service worker.
pub struct PushNotification {
    pub title: String,
    pub body: String,
    pub tag: String,
}

fn notification_payload(notification: &PushNotification) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "title": notification.title,
        "body": notification.body,
        "tag": notification.tag,
    }))
    .unwrap_or_default()
}

fn is_gone(error: &web_push::WebPushError) -> bool {
    matches!(
        error,
        web_push::WebPushError::EndpointNotValid(_) | web_push::WebPushError::EndpointNotFound(_)
    ) || is_bad_jwt_token(error)
}

fn is_bad_jwt_token(error: &web_push::WebPushError) -> bool {
    matches!(
        error,
        web_push::WebPushError::Other(info)
            if info.code == 403 && info.message.contains(r#""reason":"BadJwtToken""#)
    )
}

fn build_push_message(
    vapid_pem: &str,
    subscription: &PushSubscription,
    payload: &[u8],
) -> Result<web_push::WebPushMessage, WebError> {
    let subscription_info = web_push::SubscriptionInfo::new(
        subscription.endpoint.clone(),
        subscription.keys.p256dh.clone(),
        subscription.keys.auth.clone(),
    );
    let mut signature_builder =
        web_push::VapidSignatureBuilder::from_pem(vapid_pem.as_bytes(), &subscription_info)
            .map_err(|error| {
                WebError::CommandFailed(format!("web push signature setup failed: {error}"))
            })?;
    signature_builder.add_claim("sub", "mailto:ajax-cockpit@localhost");
    let signature = signature_builder.build().map_err(|error| {
        WebError::CommandFailed(format!("web push signature build failed: {error}"))
    })?;

    let mut message_builder = web_push::WebPushMessageBuilder::new(&subscription_info);
    message_builder.set_payload(web_push::ContentEncoding::Aes128Gcm, payload);
    message_builder.set_vapid_signature(signature);
    message_builder
        .build()
        .map_err(|error| WebError::CommandFailed(format!("web push message build failed: {error}")))
}

/// Sends `notification` to every stored subscription, pruning any the push
/// service reports as gone. Returns how many were delivered.
pub fn send_to_all(dir: &Path, notification: &PushNotification) -> Result<usize, WebError> {
    use web_push::WebPushClient;

    let subscriptions = load_subscriptions(dir);
    if subscriptions.is_empty() {
        return Ok(0);
    }
    let keys = load_or_create_vapid_keys(dir)?;
    let payload = notification_payload(notification);

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| WebError::CommandFailed(format!("web push runtime failed: {error}")))?;

    let (delivered, gone) = runtime.block_on(async {
        let client = web_push::HyperWebPushClient::new();
        let mut delivered = 0_usize;
        let mut gone: Vec<String> = Vec::new();
        for subscription in &subscriptions {
            if let Ok(message) = build_push_message(&keys.private_pem, subscription, &payload) {
                match client.send(message).await {
                    Ok(()) => delivered += 1,
                    Err(error) => {
                        if is_gone(&error) {
                            gone.push(subscription.endpoint.clone());
                        }
                    }
                }
            }
        }
        (delivered, gone)
    });

    if !gone.is_empty() {
        let live: Vec<PushSubscription> = subscriptions
            .into_iter()
            .filter(|subscription| !gone.contains(&subscription.endpoint))
            .collect();
        save_subscriptions(dir, &live)?;
    }
    Ok(delivered)
}

#[cfg(test)]
mod tests {
    use super::{
        add_subscription, build_push_message, is_gone, load_or_create_vapid_keys,
        load_subscriptions, notification_payload, remove_subscription, send_to_all, PushKeys,
        PushNotification, PushSubscription,
    };
    use std::time::{SystemTime, UNIX_EPOCH};

    fn subscription(endpoint: &str) -> PushSubscription {
        PushSubscription {
            endpoint: endpoint.to_string(),
            keys: PushKeys {
                p256dh: "p256dh-key".to_string(),
                auth: "auth-secret".to_string(),
            },
        }
    }

    fn scratch_dir(tag: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "ajax-web-push-{tag}-{}-{nanos}",
            std::process::id()
        ))
    }

    #[test]
    fn vapid_keys_are_generated_persisted_and_reused() {
        let dir = scratch_dir("vapid");

        let first = load_or_create_vapid_keys(&dir).unwrap();
        assert!(first.private_pem.contains("PRIVATE KEY"));
        assert_eq!(first.public_key.len(), 65, "uncompressed P-256 point");
        assert_eq!(first.public_key[0], 0x04, "uncompressed point marker");
        assert!(dir.join("web-push-vapid.pem").exists());

        let second = load_or_create_vapid_keys(&dir).unwrap();
        assert_eq!(first.private_pem, second.private_pem);
        assert_eq!(first.public_key, second.public_key);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn subscriptions_are_stored_deduplicated_and_removed() {
        let dir = scratch_dir("subs");

        add_subscription(&dir, subscription("https://push.example/a")).unwrap();
        add_subscription(&dir, subscription("https://push.example/b")).unwrap();
        add_subscription(&dir, subscription("https://push.example/a")).unwrap();
        assert_eq!(load_subscriptions(&dir).len(), 2);

        remove_subscription(&dir, "https://push.example/a").unwrap();
        let remaining = load_subscriptions(&dir);
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].endpoint, "https://push.example/b");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn transient_errors_are_not_classified_as_gone() {
        assert!(!is_gone(&web_push::WebPushError::Unspecified));
        assert!(!is_gone(&web_push::WebPushError::InvalidUri));
        assert!(!is_gone(&web_push::WebPushError::PayloadTooLarge));
        assert!(!is_gone(&web_push::WebPushError::InvalidResponse));
    }

    #[test]
    fn bad_jwt_token_errors_are_classified_as_gone() {
        let error = web_push::request_builder::parse_response(
            403.try_into().unwrap(),
            br#"{"reason":"BadJwtToken"}"#.to_vec(),
        )
        .unwrap_err();

        assert!(is_gone(&error));
    }

    #[test]
    fn notification_payload_is_json_with_title_and_body() {
        let payload = notification_payload(&PushNotification {
            title: "Task needs review".to_string(),
            body: "web/fix-login".to_string(),
            tag: "web/fix-login".to_string(),
        });
        let value: serde_json::Value = serde_json::from_slice(&payload).unwrap();
        assert_eq!(value["title"], "Task needs review");
        assert_eq!(value["body"], "web/fix-login");
        assert_eq!(value["tag"], "web/fix-login");
    }

    #[test]
    fn build_push_message_rejects_invalid_subscription_keys() {
        let dir = scratch_dir("badkeys");
        let keys = load_or_create_vapid_keys(&dir).unwrap();

        let result = build_push_message(
            &keys.private_pem,
            &subscription("https://push.example/bad"),
            b"payload",
        );
        assert!(result.is_err());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn send_to_all_with_no_subscriptions_delivers_nothing() {
        let dir = scratch_dir("empty-send");
        let delivered = send_to_all(
            &dir,
            &PushNotification {
                title: "ignored".to_string(),
                body: "ignored".to_string(),
                tag: "ignored".to_string(),
            },
        )
        .unwrap();
        assert_eq!(delivered, 0);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn web_push_does_not_write_delivery_failures_to_terminal() {
        let source = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/adapters/push.rs"),
        )
        .unwrap();
        let terminal_write = ["e", "println!"].concat();

        assert!(!source.contains(&terminal_write));
    }
}
