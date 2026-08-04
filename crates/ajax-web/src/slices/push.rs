//! Declarative Web Push attention delivery for Web Cockpit.
//!
//! Uses `window.pushManager` on the client (no service worker). Server loads
//! VAPID keys and subscriptions under `state_dir` at process start into
//! [`PushHub`], encrypts with `web-push-native`, and delivers via `curl`.
//! HTTP handlers mutate in-memory state only; a background flusher persists
//! (avoids CodeQL `rust/path-injection` on remote-reachable `state_dir` joins).

use ajax_core::attention::{take_attention_transition, AttentionTransition};
use ajax_core::commands::CommandContext;
use ajax_core::registry::{InMemoryRegistry, Registry};
use axum::http::{header, uri::Authority, HeaderMap, Request, Uri};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{io::AsyncWriteExt, process::Command, sync::Notify};
use web_push_native::{
    jwt_simple::algorithms::ES256KeyPair,
    p256::{elliptic_curve::sec1::ToEncodedPoint, PublicKey},
    Auth, WebPushBuilder,
};

const VAPID_KEY_FILE: &str = "web-push-vapid.key";
const SUBSCRIPTIONS_FILE: &str = "web-push-subscriptions.json";
pub(crate) const DEFAULT_PUSH_POLL_SECONDS: u64 = 30;

/// Process-local push persistence. Disk I/O happens in [`PushHub::load_or_create`]
/// and [`PushHub::flush_if_dirty`] (background only) — not from HTTP handlers.
pub struct PushHub {
    inner: Mutex<PushInner>,
    disk: Option<PushDiskPaths>,
    flush_notify: Notify,
}

struct PushDiskPaths {
    subscriptions_path: PathBuf,
}

struct PushInner {
    key_pair_bytes: Vec<u8>,
    store: SubscriptionStore,
    dirty: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct PushSubscription {
    pub(crate) endpoint: String,
    pub(crate) keys: PushSubscriptionKeys,
    /// Absolute https cockpit URL for declarative `navigate` (set server-side).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) navigate: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct PushSubscriptionKeys {
    pub(crate) p256dh: String,
    pub(crate) auth: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct UnsubscribeRequest {
    #[serde(default)]
    pub(crate) endpoint: Option<String>,
    /// Clear every stored subscription (Settings Disable with no local sub).
    #[serde(default)]
    pub(crate) all: bool,
}

#[derive(Debug, Deserialize)]
pub(crate) struct PushTestRequest {
    #[serde(flatten)]
    subscription: PushSubscription,
    /// Optional delay before delivery (Settings closed-app smoke test).
    #[serde(default)]
    delay_ms: u64,
}

const MAX_PUSH_TEST_DELAY_MS: u64 = 60_000;

#[derive(Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct SubscriptionStore {
    #[serde(default)]
    subscriptions: Vec<PushSubscription>,
}

impl PushHub {
    /// In-memory only (no disk). Used by `WebAppState::new` test harnesses.
    pub fn ephemeral() -> Arc<Self> {
        let key_pair = ES256KeyPair::generate();
        Arc::new(Self {
            inner: Mutex::new(PushInner {
                key_pair_bytes: key_pair.to_bytes(),
                store: SubscriptionStore::default(),
                dirty: false,
            }),
            disk: None,
            flush_notify: Notify::new(),
        })
    }

    /// Load or create VAPID + subscriptions under `state_dir`. Call at process
    /// start only — not from HTTP handlers.
    pub fn load_or_create(state_dir: &Path) -> Result<Arc<Self>, String> {
        fs::create_dir_all(state_dir).map_err(|error| format!("create state dir: {error}"))?;
        let vapid_path = state_dir.join(VAPID_KEY_FILE);
        let subscriptions_path = state_dir.join(SUBSCRIPTIONS_FILE);
        let key_pair_bytes = if vapid_path.is_file() {
            let bytes =
                fs::read(&vapid_path).map_err(|error| format!("read VAPID key: {error}"))?;
            ES256KeyPair::from_bytes(&bytes)
                .map_err(|error| format!("invalid VAPID key file: {error}"))?;
            bytes
        } else {
            let generated = ES256KeyPair::generate().to_bytes();
            write_private_file(&vapid_path, &generated)?;
            generated
        };
        let store = if subscriptions_path.is_file() {
            let raw = fs::read_to_string(&subscriptions_path)
                .map_err(|error| format!("read subscriptions: {error}"))?;
            serde_json::from_str(&raw).map_err(|error| format!("parse subscriptions: {error}"))?
        } else {
            SubscriptionStore::default()
        };
        Ok(Arc::new(Self {
            inner: Mutex::new(PushInner {
                key_pair_bytes,
                store,
                dirty: false,
            }),
            disk: Some(PushDiskPaths { subscriptions_path }),
            flush_notify: Notify::new(),
        }))
    }

    pub(crate) fn vapid_public_key_base64(&self) -> Result<String, String> {
        let key_pair = self.key_pair()?;
        Ok(URL_SAFE_NO_PAD.encode(vapid_public_key_bytes(&key_pair)))
    }

    pub(crate) fn has_subscriptions(&self) -> bool {
        self.inner
            .lock()
            .map(|guard| !guard.store.subscriptions.is_empty())
            .unwrap_or(false)
    }

    pub(crate) fn upsert_subscription(
        &self,
        mut subscription: PushSubscription,
        navigate: &str,
    ) -> Result<(), String> {
        validate_subscription(&subscription)?;
        validate_navigate_url(navigate)?;
        // Single-operator Cockpit: latest subscribe replaces the store so VAPID
        // rotation / re-enable cannot accumulate stale endpoints.
        subscription.navigate = Some(navigate.to_string());
        {
            let mut guard = self
                .inner
                .lock()
                .map_err(|_| "push hub lock poisoned".to_string())?;
            guard.store = SubscriptionStore {
                subscriptions: vec![subscription],
            };
            guard.dirty = true;
        }
        self.flush_notify.notify_one();
        Ok(())
    }

    pub(crate) fn apply_unsubscribe(&self, request: &UnsubscribeRequest) -> Result<(), String> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| "push hub lock poisoned".to_string())?;
        if request.all || request.endpoint.as_deref().is_none_or(str::is_empty) {
            guard.store = SubscriptionStore::default();
        } else {
            let endpoint = request.endpoint.as_deref().unwrap_or_default();
            guard
                .store
                .subscriptions
                .retain(|item| item.endpoint != endpoint);
        }
        guard.dirty = true;
        drop(guard);
        self.flush_notify.notify_one();
        Ok(())
    }

    fn key_pair(&self) -> Result<ES256KeyPair, String> {
        let guard = self
            .inner
            .lock()
            .map_err(|_| "push hub lock poisoned".to_string())?;
        ES256KeyPair::from_bytes(&guard.key_pair_bytes)
            .map_err(|error| format!("invalid VAPID key: {error}"))
    }

    fn prune_endpoints(&self, endpoints: &[String]) {
        let Ok(mut guard) = self.inner.lock() else {
            return;
        };
        guard
            .store
            .subscriptions
            .retain(|item| !endpoints.contains(&item.endpoint));
        guard.dirty = true;
        drop(guard);
        self.flush_notify.notify_one();
    }

    /// Persist dirty subscription state. Call from background tasks only.
    pub(crate) fn flush_if_dirty(&self) -> Result<(), String> {
        let Some(disk) = self.disk.as_ref() else {
            return Ok(());
        };
        let (dirty, store) = {
            let guard = self
                .inner
                .lock()
                .map_err(|_| "push hub lock poisoned".to_string())?;
            (guard.dirty, guard.store.clone())
        };
        if !dirty {
            return Ok(());
        }
        if let Some(parent) = disk.subscriptions_path.parent() {
            fs::create_dir_all(parent).map_err(|error| format!("create state dir: {error}"))?;
        }
        let raw = serde_json::to_string_pretty(&store)
            .map_err(|error| format!("serialize subscriptions: {error}"))?;
        write_private_file(&disk.subscriptions_path, raw.as_bytes())?;
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| "push hub lock poisoned".to_string())?;
        // Only clear dirty if no newer mutation landed during the write.
        if guard.store == store {
            guard.dirty = false;
        }
        Ok(())
    }
}

/// Background flusher so HTTP handlers never touch push disk paths.
pub(crate) fn spawn_push_flusher(hub: Arc<PushHub>) {
    tokio::spawn(async move {
        loop {
            hub.flush_notify.notified().await;
            // Coalesce bursts from enable/disable.
            tokio::time::sleep(Duration::from_millis(50)).await;
            if let Err(error) = hub.flush_if_dirty() {
                eprintln!("declarative push flush failed: {error}");
            }
        }
    });
}

/// Take attention transitions and fan-out declarative push. Returns true when
/// any task metadata stamp changed (caller should persist registry).
pub(crate) fn deliver_attention_pushes(
    context: &mut CommandContext<InMemoryRegistry>,
    hub: &PushHub,
) -> bool {
    let (subscriptions, key_pair) = {
        let Ok(guard) = hub.inner.lock() else {
            return false;
        };
        if guard.store.subscriptions.is_empty() {
            return false;
        }
        let Ok(key_pair) = ES256KeyPair::from_bytes(&guard.key_pair_bytes) else {
            return false;
        };
        (guard.store.subscriptions.clone(), key_pair)
    };
    let task_ids: Vec<_> = context
        .registry
        .list_tasks()
        .iter()
        .map(|task| task.id.clone())
        .collect();
    let mut fired = false;
    let mut dead_endpoints = Vec::new();
    for task_id in task_ids {
        let Some(task) = context.registry.get_task_mut(&task_id) else {
            continue;
        };
        let Some(transition) = take_attention_transition(task) else {
            continue;
        };
        fired = true;
        for subscription in &subscriptions {
            let navigate = subscription
                .navigate
                .as_deref()
                .unwrap_or("https://localhost/");
            let vapid_subject = navigate.trim_end_matches('/').to_string();
            let payload = attention_payload(&transition, navigate);
            match build_push_request(subscription.clone(), payload, &key_pair, &vapid_subject) {
                Ok(request) => match deliver_with_curl_blocking(request) {
                    Ok(()) => {}
                    Err(error) if is_gone_endpoint(&error) => {
                        dead_endpoints.push(subscription.endpoint.clone());
                    }
                    Err(error) => {
                        eprintln!("declarative push delivery failed: {error}");
                    }
                },
                Err(error) => {
                    eprintln!("declarative push build failed: {error}");
                }
            }
        }
    }
    if !dead_endpoints.is_empty() {
        hub.prune_endpoints(&dead_endpoints);
    }
    fired
}

pub(crate) fn schedule_test_push(
    hub: &PushHub,
    headers: &HeaderMap,
    request: PushTestRequest,
) -> Result<(), String> {
    validate_subscription(&request.subscription)?;
    let delay_ms = request.delay_ms.min(MAX_PUSH_TEST_DELAY_MS);
    let key_pair = hub.key_pair()?;
    let navigate = navigation_url(headers)?;
    let vapid_subject = navigate.trim_end_matches('/').to_string();
    let payload = test_payload(&navigate);
    let http_request =
        build_push_request(request.subscription, payload, &key_pair, &vapid_subject)?;
    tokio::spawn(async move {
        if delay_ms > 0 {
            tokio::time::sleep(Duration::from_millis(delay_ms)).await;
        }
        if let Err(error) = deliver_with_curl(http_request).await {
            eprintln!("declarative push test delivery failed: {error}");
        }
    });
    Ok(())
}

/// Build navigate URL from Host; when Origin is present it must match Host.
pub(crate) fn navigation_url(headers: &HeaderMap) -> Result<String, String> {
    let host = headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .and_then(|host| host.parse::<Authority>().ok())
        .ok_or_else(|| "missing or invalid Host header for navigate URL".to_string())?;
    if let Some(origin_authority) = headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
        .and_then(|origin| origin.parse::<Uri>().ok())
        .and_then(|uri| uri.authority().cloned())
    {
        if origin_authority.host() != host.host() || origin_authority.port() != host.port() {
            return Err("Origin does not match Host".to_string());
        }
    }
    Ok(format!("https://{host}/"))
}

fn write_private_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    fs::write(path, bytes).map_err(|error| format!("write {}: {error}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

fn vapid_public_key_bytes(key_pair: &ES256KeyPair) -> Vec<u8> {
    PublicKey::from_sec1_bytes(&key_pair.public_key().to_bytes())
        .expect("ES256 key pair always has a valid public key")
        .to_encoded_point(false)
        .as_bytes()
        .to_vec()
}

fn validate_subscription(subscription: &PushSubscription) -> Result<(), String> {
    let endpoint = subscription
        .endpoint
        .parse::<Uri>()
        .map_err(|error| format!("invalid push endpoint: {error}"))?;
    if endpoint.scheme_str() != Some("https") || endpoint.authority().is_none() {
        return Err("push endpoint must be an absolute https URL".to_string());
    }
    if !push_endpoint_host_allowed(&endpoint) {
        return Err("push endpoint host is not an allowed Web Push service".to_string());
    }
    let _ = URL_SAFE_NO_PAD
        .decode(&subscription.keys.p256dh)
        .map_err(|error| format!("invalid subscription p256dh key: {error}"))?;
    let auth_bytes = URL_SAFE_NO_PAD
        .decode(&subscription.keys.auth)
        .map_err(|error| format!("invalid subscription auth key: {error}"))?;
    if auth_bytes.len() != 16 {
        return Err("subscription auth key must decode to 16 bytes".to_string());
    }
    Ok(())
}

fn validate_navigate_url(navigate: &str) -> Result<(), String> {
    let uri = navigate
        .parse::<Uri>()
        .map_err(|error| format!("invalid navigate URL: {error}"))?;
    if uri.scheme_str() != Some("https") || uri.authority().is_none() {
        return Err("navigate must be an absolute https URL".to_string());
    }
    Ok(())
}

/// Allow only known browser push services — blocks SSRF to RFC1918/metadata.
fn push_endpoint_host_allowed(endpoint: &Uri) -> bool {
    let Some(authority) = endpoint.authority() else {
        return false;
    };
    let host = authority.host();
    if host.parse::<std::net::IpAddr>().is_ok() {
        return false;
    }
    let host = host.to_ascii_lowercase();
    const EXACT: &[&str] = &[
        "web.push.apple.com",
        "fcm.googleapis.com",
        "fcmregistrations.googleapis.com",
        "android.googleapis.com",
        "updates.push.services.mozilla.com",
        "push.services.mozilla.com",
    ];
    if EXACT.iter().any(|allowed| host == *allowed) {
        return true;
    }
    host.ends_with(".push.apple.com")
        || host.ends_with(".notify.windows.com")
        || host.ends_with(".push.services.mozilla.com")
}

fn attention_payload(transition: &AttentionTransition, navigate: &str) -> Vec<u8> {
    let mut body = format!(
        "{}/{}: {} ({})",
        transition.repo,
        transition.handle,
        transition.status.as_str(),
        transition.client
    );
    if let Some(explanation) = transition.explanation.as_deref() {
        body.push_str(" — ");
        body.push_str(explanation);
    }
    serde_json::to_vec(&json!({
        "web_push": 8030,
        "notification": {
            "title": "Ajax Cockpit",
            "lang": "en-US",
            "dir": "ltr",
            "body": body,
            "navigate": navigate,
            "silent": false
        }
    }))
    .expect("attention declarative push payload is serializable")
}

fn test_payload(navigate: &str) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "web_push": 8030,
        "notification": {
            "title": "Ajax Cockpit",
            "lang": "en-US",
            "dir": "ltr",
            "body": "Push notification test",
            "navigate": navigate,
            "silent": false
        }
    }))
    .expect("test declarative push payload is serializable")
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

fn is_gone_endpoint(error: &str) -> bool {
    error.contains("404")
        || error.contains("410")
        || error.contains("HTTP/2 404")
        || error.contains("HTTP/2 410")
}

fn deliver_with_curl_blocking(request: Request<Vec<u8>>) -> Result<(), String> {
    let (parts, body) = request.into_parts();
    let mut command = std::process::Command::new("curl");
    command
        .args(["-sS", "--fail", "--max-time", "10", "-X"])
        .arg(parts.method.as_str());
    for (name, value) in &parts.headers {
        command.arg("-H").arg(format!(
            "{name}: {}",
            value
                .to_str()
                .map_err(|error| format!("invalid push request header: {error}"))?
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
    {
        use std::io::Write;
        child
            .stdin
            .as_mut()
            .ok_or_else(|| "open curl stdin for push delivery".to_string())?
            .write_all(&body)
            .map_err(|error| format!("write encrypted push payload to curl: {error}"))?;
    }
    let output = child
        .wait_with_output()
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
                .map_err(|error| format!("invalid push request header: {error}"))?
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
    use ajax_core::lifecycle::mark_active;
    use ajax_core::models::{AgentClient, SideFlag, Task, TaskId};
    use serde_json::Value;
    use std::time::{SystemTime, UNIX_EPOCH};

    const P256DH: &str =
        "BLn9b-VR0ca83knDNZ32dCHGyjJp-1riX9ZTN40MqV8K_LpQmLqxC_DoHvqvFXO_nGdAB4W9dogZb_sM-uV4JbY";
    const AUTH: &str = "_ordMnz7uTCmrpBTeUV4Bw";

    fn scratch_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "ajax-push-{label}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn sample_subscription() -> PushSubscription {
        PushSubscription {
            endpoint: "https://web.push.apple.com/messages/1".to_string(),
            keys: PushSubscriptionKeys {
                p256dh: P256DH.to_string(),
                auth: AUTH.to_string(),
            },
            navigate: None,
        }
    }

    #[test]
    fn vapid_persists_across_load() {
        let dir = scratch_dir("vapid");
        let first_hub = PushHub::load_or_create(&dir).unwrap();
        let first = first_hub.vapid_public_key_base64().unwrap();
        let path = dir.join(VAPID_KEY_FILE);
        assert!(path.is_file());
        let bytes = fs::read(&path).unwrap();
        let key = ES256KeyPair::from_bytes(&bytes).unwrap();
        assert_eq!(first, URL_SAFE_NO_PAD.encode(vapid_public_key_bytes(&key)));
        let second_hub = PushHub::load_or_create(&dir).unwrap();
        assert_eq!(first, second_hub.vapid_public_key_base64().unwrap());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn subscription_store_replace_and_clear() {
        let dir = scratch_dir("store");
        let hub = PushHub::load_or_create(&dir).unwrap();
        assert!(!hub.has_subscriptions());
        hub.upsert_subscription(sample_subscription(), "https://cockpit.example/")
            .unwrap();
        hub.flush_if_dirty().unwrap();
        assert!(hub.has_subscriptions());
        {
            let guard = hub.inner.lock().unwrap();
            assert_eq!(
                guard.store.subscriptions[0].navigate.as_deref(),
                Some("https://cockpit.example/")
            );
        }
        let mut second = sample_subscription();
        second.endpoint = "https://fcm.googleapis.com/fcm/send/abc".to_string();
        hub.upsert_subscription(second, "https://cockpit.example/")
            .unwrap();
        hub.flush_if_dirty().unwrap();
        {
            let guard = hub.inner.lock().unwrap();
            assert_eq!(guard.store.subscriptions.len(), 1);
            assert!(guard.store.subscriptions[0]
                .endpoint
                .contains("fcm.googleapis.com"));
        }
        hub.apply_unsubscribe(&UnsubscribeRequest {
            endpoint: None,
            all: true,
        })
        .unwrap();
        hub.flush_if_dirty().unwrap();
        assert!(!hub.has_subscriptions());
        let reloaded = PushHub::load_or_create(&dir).unwrap();
        assert!(!reloaded.has_subscriptions());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn rejects_non_push_service_endpoints() {
        let mut bad = sample_subscription();
        bad.endpoint = "https://127.0.0.1/steal".to_string();
        assert!(validate_subscription(&bad).is_err());
        bad.endpoint = "https://evil.example/push".to_string();
        assert!(validate_subscription(&bad).is_err());
    }

    #[test]
    fn navigation_url_requires_matching_origin_and_host() {
        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, "cockpit.example".parse().unwrap());
        headers.insert(header::ORIGIN, "https://attacker.example".parse().unwrap());
        assert!(navigation_url(&headers).is_err());
        headers.insert(header::ORIGIN, "https://cockpit.example".parse().unwrap());
        assert_eq!(
            navigation_url(&headers).unwrap(),
            "https://cockpit.example/"
        );
    }

    #[test]
    fn payload_has_declarative_shape() {
        let transition = AttentionTransition {
            repo: "web".to_string(),
            handle: "fix-login".to_string(),
            status: ajax_core::ui_state::TaskStatus::Waiting,
            explanation: Some("Waiting for input".to_string()),
            client: "codex".to_string(),
        };
        let payload: Value =
            serde_json::from_slice(&attention_payload(&transition, "https://cockpit.example/"))
                .unwrap();
        assert_eq!(payload["web_push"], 8030);
        assert_eq!(
            payload["notification"]["body"],
            "web/fix-login: Waiting (codex) — Waiting for input"
        );
        assert_eq!(
            payload["notification"]["navigate"],
            "https://cockpit.example/"
        );
    }

    #[test]
    fn valid_subscription_builds_encrypted_request() {
        let vapid = ES256KeyPair::generate();
        let request = build_push_request(
            sample_subscription(),
            test_payload("https://localhost/"),
            &vapid,
            "https://cockpit.example",
        )
        .unwrap();
        assert_eq!(request.method(), "POST");
        assert_eq!(request.uri(), "https://web.push.apple.com/messages/1");
        assert_eq!(request.headers()[header::CONTENT_ENCODING], "aes128gcm");
    }

    #[test]
    fn deliver_attention_pushes_without_subscriptions_is_noop() {
        let hub = PushHub::ephemeral();
        let mut task = Task::new(
            TaskId::new("web/wait"),
            "web",
            "wait",
            "Wait",
            "ajax/wait",
            "main",
            "/tmp/w",
            "ajax-web-wait",
            "task",
            AgentClient::Codex,
        );
        mark_active(&mut task).unwrap();
        task.add_side_flag(SideFlag::NeedsInput);
        let now_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        task.metadata.insert(
            ajax_core::attention::NOTIFY_CANDIDATE_SINCE_KEY.to_string(),
            (now_secs.saturating_sub(20)).to_string(),
        );
        let mut registry = InMemoryRegistry::default();
        registry.create_task(task).unwrap();
        let mut context = CommandContext::new(ajax_core::config::Config::default(), registry);
        assert!(!deliver_attention_pushes(&mut context, &hub));
    }
}
