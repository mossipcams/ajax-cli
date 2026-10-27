use std::path::PathBuf;
use std::time::{Duration, Instant};

use super::{with_test_cursor_sdk_module, CursorCredentialStatus, CursorLogin, CursorLoginPhase};

const FAKE_KEY: &str = "sk-RUST-FAKE-SECRET";
const LOGIN_URL: &str = "https://cursor.test/login?x=1";
const EXPIRES_AT_MS: u64 = 4_102_444_800_000;

const FAKE_SDK: &str = r#"
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const releasePath = join(dirname(fileURLToPath(import.meta.url)), "release");

export class FileCredentialStore {
  constructor(path) { this.path = path; }
  async load() {
    try { return JSON.parse(readFileSync(this.path, "utf8")); } catch { return undefined; }
  }
  async save(creds) { writeFileSync(this.path, JSON.stringify(creds)); }
}

export class InMemoryCredentialStore {
  constructor() { this.creds = null; }
  async load() { return this.creds; }
  async save(creds) { this.creds = creds; }
}

const FAKE_KEY = "sk-RUST-FAKE-SECRET";

export const Cursor = {
  auth: {
    async login(options) {
      options.onLoginUrl("https://cursor.test/login?x=1");
      if (process.argv.join(" ").includes("leak-exit")) {
        process.stderr.write('leak sk-RUST-FAKE-SECRET\n');
        process.exit(3);
      }
      await new Promise((resolve, reject) => {
        const poll = setInterval(() => {
          if (options.signal?.aborted) {
            clearInterval(poll);
            reject(new Error("aborted"));
            return;
          }
          try {
            readFileSync(releasePath);
            clearInterval(poll);
            resolve();
          } catch {}
        }, 20);
      });
      const creds = {
        version: 1,
        backendUrl: "x",
        apiKey: FAKE_KEY,
        apiKeyExpiresAtMs: 4102444800000,
        email: "op@example.com",
        createdAtMs: 1,
      };
      await options.store.save(creds);
      return creds;
    },
  },
  async me(creds) {
    if (creds?.apiKey !== FAKE_KEY) throw new Error("invalid api key");
    return { userEmail: "op@example.com" };
  },
};

export function sdkVersion() { return "0.0.0-fake"; }
"#;

const FAKE_SDK_HANG: &str = r#"
import { writeFileSync } from "node:fs";

setInterval(() => {}, 1000);

export class FileCredentialStore {
  constructor(path) { this.path = path; }
  async load() { return undefined; }
  async save(creds) { writeFileSync(this.path, JSON.stringify(creds)); }
}

export class InMemoryCredentialStore {
  constructor() { this.creds = null; }
  async load() { return this.creds; }
  async save(creds) { this.creds = creds; }
}

export const Cursor = {
  auth: {
    async login(options) {
      options.onLoginUrl("https://cursor.test/login?x=1");
      // Never resolves and ignores options.signal on purpose.
      return new Promise(() => {});
    },
  },
  async me(creds) {
    throw new Error("invalid api key");
  },
};

export function sdkVersion() { return "0.0.0-fake-hang"; }
"#;

const FAKE_SDK_LEAK_EXIT: &str = FAKE_SDK;

fn fresh_dir(label: &str) -> (PathBuf, PathBuf) {
    let dir = std::env::temp_dir().join(format!(
        "ajax-cursor-login-test-{}-{}",
        std::process::id(),
        label
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp dir");
    (dir.clone(), dir.join("fake_cursor_sdk.mjs"))
}

fn poll_until(
    login: &CursorLogin,
    matches: impl Fn(&CursorLoginPhase) -> bool,
    deadline: Duration,
    observed: &mut Vec<serde_json::Value>,
) -> CursorLoginPhase {
    let start = Instant::now();
    loop {
        let phase = login.phase();
        observed.push(serde_json::to_value(&phase).expect("phase serializes"));
        if matches(&phase) {
            return phase;
        }
        assert!(
            start.elapsed() < deadline,
            "timed out polling; last phase: {phase:?}"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn poll_status(
    login: &CursorLogin,
    observed: &mut Vec<serde_json::Value>,
) -> CursorCredentialStatus {
    let status = login.status().expect("status() succeeds");
    observed.push(serde_json::to_value(&status).expect("status serializes"));
    status
}

#[test]
fn start_returns_fast_and_reaches_succeeded_after_release() {
    let (dir, sdk) = fresh_dir("success");
    std::fs::write(&sdk, FAKE_SDK).expect("write fake SDK");
    with_test_cursor_sdk_module(&sdk, || {
        let store = dir.join("auth.json");
        let release = dir.join("release");
        let login = CursorLogin::new(Some(store.clone()), Duration::from_secs(30));
        let mut observed: Vec<serde_json::Value> = Vec::new();

        let started = Instant::now();
        login.start();
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "start() blocked"
        );

        let phase = poll_until(
            &login,
            |p| matches!(p, CursorLoginPhase::Waiting { .. }),
            Duration::from_secs(10),
            &mut observed,
        );
        match phase {
            CursorLoginPhase::Waiting { url } => assert_eq!(url, LOGIN_URL),
            other => panic!("expected Waiting, got {other:?}"),
        }
        assert!(
            !release.exists(),
            "release file must not predate the Waiting assertion"
        );

        std::fs::write(&release, "go").expect("create release file");
        let phase = poll_until(
            &login,
            |p| matches!(p, CursorLoginPhase::Succeeded { .. }),
            Duration::from_secs(10),
            &mut observed,
        );
        match phase {
            CursorLoginPhase::Succeeded {
                email,
                expires_at_ms,
            } => {
                assert_eq!(email.as_deref(), Some("op@example.com"));
                assert_eq!(expires_at_ms, Some(EXPIRES_AT_MS));
            }
            other => panic!("expected Succeeded, got {other:?}"),
        }
        assert!(store.exists(), "store file missing after success");

        for value in &observed {
            let text = value.to_string();
            assert!(
                !text.contains(FAKE_KEY),
                "serialized phase leaks the key: {text}"
            );
        }
    });
}

#[test]
fn second_start_while_waiting_returns_same_phase() {
    let (dir, sdk) = fresh_dir("second-start");
    std::fs::write(&sdk, FAKE_SDK).expect("write fake SDK");
    with_test_cursor_sdk_module(&sdk, || {
        let login = CursorLogin::new(Some(dir.join("auth.json")), Duration::from_secs(30));
        let mut observed: Vec<serde_json::Value> = Vec::new();

        login.start();
        let first = poll_until(
            &login,
            |p| matches!(p, CursorLoginPhase::Waiting { .. }),
            Duration::from_secs(10),
            &mut observed,
        );
        let again = login.start();
        assert_eq!(
            again, first,
            "second start must return the live Waiting phase"
        );

        for value in &observed {
            assert!(!value.to_string().contains(FAKE_KEY));
        }
    });
}

#[test]
fn cancel_while_waiting_fails_and_never_writes_store() {
    let (dir, sdk) = fresh_dir("cancel");
    std::fs::write(&sdk, FAKE_SDK).expect("write fake SDK");
    with_test_cursor_sdk_module(&sdk, || {
        let store = dir.join("auth.json");
        let login = CursorLogin::new(Some(store.clone()), Duration::from_secs(30));
        let mut observed: Vec<serde_json::Value> = Vec::new();

        login.start();
        poll_until(
            &login,
            |p| matches!(p, CursorLoginPhase::Waiting { .. }),
            Duration::from_secs(10),
            &mut observed,
        );

        login.cancel();
        let phase = poll_until(
            &login,
            |p| matches!(p, CursorLoginPhase::Failed { .. }),
            Duration::from_secs(10),
            &mut observed,
        );
        match phase {
            CursorLoginPhase::Failed { message } => assert_eq!(message, "cancelled"),
            other => panic!("expected Failed, got {other:?}"),
        }
        assert!(
            !store.exists(),
            "cancelled login must not create the store file"
        );

        for value in &observed {
            assert!(!value.to_string().contains(FAKE_KEY));
        }
    });
}

#[test]
fn timeout_without_release_fails_with_timed_out() {
    let (dir, sdk) = fresh_dir("timeout");
    std::fs::write(&sdk, FAKE_SDK).expect("write fake SDK");
    with_test_cursor_sdk_module(&sdk, || {
        let login = CursorLogin::new(Some(dir.join("auth.json")), Duration::from_millis(500));
        let mut observed: Vec<serde_json::Value> = Vec::new();

        login.start();
        let phase = poll_until(
            &login,
            |p| matches!(p, CursorLoginPhase::Failed { .. }),
            Duration::from_secs(10),
            &mut observed,
        );
        match phase {
            CursorLoginPhase::Failed { message } => {
                assert!(
                    message.contains("timed out"),
                    "unexpected message: {message}"
                )
            }
            other => panic!("expected Failed, got {other:?}"),
        }

        for value in &observed {
            assert!(!value.to_string().contains(FAKE_KEY));
        }
    });
}

#[test]
fn status_reports_absent_then_valid_creds() {
    let (dir, sdk) = fresh_dir("status");
    std::fs::write(&sdk, FAKE_SDK).expect("write fake SDK");
    with_test_cursor_sdk_module(&sdk, || {
        let store = dir.join("auth.json");
        assert!(!store.exists());
        let login = CursorLogin::new(Some(store.clone()), Duration::from_secs(30));
        let mut observed: Vec<serde_json::Value> = Vec::new();

        let absent = poll_status(&login, &mut observed);
        assert!(!absent.present, "no store file means present:false");

        let release = dir.join("release");
        login.start();
        poll_until(
            &login,
            |p| matches!(p, CursorLoginPhase::Waiting { .. }),
            Duration::from_secs(10),
            &mut observed,
        );
        std::fs::write(&release, "go").expect("create release file");
        poll_until(
            &login,
            |p| matches!(p, CursorLoginPhase::Succeeded { .. }),
            Duration::from_secs(10),
            &mut observed,
        );

        let present = poll_status(&login, &mut observed);
        assert!(present.present);
        assert_eq!(present.valid, Some(true));
        assert_eq!(present.error.as_deref(), None);

        for value in &observed {
            assert!(!value.to_string().contains(FAKE_KEY));
        }
    });
}

#[test]
fn serialized_phases_and_status_never_contain_api_key() {
    let (dir, sdk) = fresh_dir("no-leak");
    std::fs::write(&sdk, FAKE_SDK).expect("write fake SDK");
    with_test_cursor_sdk_module(&sdk, || {
        let store = dir.join("auth.json");
        let login = CursorLogin::new(Some(store.clone()), Duration::from_secs(30));
        let mut observed: Vec<serde_json::Value> = Vec::new();

        assert!(!serde_json::to_string(&login.phase())
            .unwrap()
            .contains(FAKE_KEY));
        login.start();
        poll_until(
            &login,
            |p| matches!(p, CursorLoginPhase::Waiting { .. }),
            Duration::from_secs(10),
            &mut observed,
        );

        let release = dir.join("release");
        std::fs::write(&release, "go").expect("create release file");
        poll_until(
            &login,
            |p| matches!(p, CursorLoginPhase::Succeeded { .. }),
            Duration::from_secs(10),
            &mut observed,
        );
        let status = poll_status(&login, &mut observed);

        assert!(std::fs::read_to_string(&store)
            .expect("store exists")
            .contains(FAKE_KEY));
        for value in &observed {
            let text = serde_json::to_string(value).expect("value serializes");
            assert!(
                !text.contains(FAKE_KEY),
                "serialization leaks the key: {text}"
            );
        }
        assert_eq!(status.valid, Some(true));
    });
}

#[test]
fn cancel_with_unresponsive_sidecar_times_out_and_allows_restart() {
    let (dir, sdk) = fresh_dir("hang-cancel");
    std::fs::write(&sdk, FAKE_SDK_HANG).expect("write hanging fake SDK");
    with_test_cursor_sdk_module(&sdk, || {
        let login = CursorLogin::new(Some(dir.join("auth.json")), Duration::from_millis(800));
        let mut observed: Vec<serde_json::Value> = Vec::new();

        login.start();
        poll_until(
            &login,
            |p| matches!(p, CursorLoginPhase::Waiting { .. }),
            Duration::from_secs(10),
            &mut observed,
        );

        login.cancel();
        let phase = poll_until(
            &login,
            |p| matches!(p, CursorLoginPhase::Failed { .. }),
            Duration::from_secs(10),
            &mut observed,
        );
        match phase {
            CursorLoginPhase::Failed { message } => assert!(
                message.contains("timed out"),
                "expected a timeout failure, got: {message}"
            ),
            other => panic!("expected Failed after cancel, got {other:?}"),
        }

        let restarted = login.start();
        assert!(
            matches!(
                restarted,
                CursorLoginPhase::Idle
                    | CursorLoginPhase::Waiting { .. }
                    | CursorLoginPhase::Failed { .. }
            ),
            "start() after a timed-out flow returned an unexpected phase: {restarted:?}"
        );

        for value in &observed {
            assert!(!value.to_string().contains(FAKE_KEY));
        }
    });
}

#[test]
fn leaked_stderr_on_exit_never_reaches_failed_phase_message() {
    let (dir, sdk) = fresh_dir("leak-exit");
    std::fs::write(&sdk, FAKE_SDK_LEAK_EXIT).expect("write leaking fake SDK");
    with_test_cursor_sdk_module(&sdk, || {
        let login = CursorLogin::new(Some(dir.join("auth.json")), Duration::from_secs(30));
        let mut observed: Vec<serde_json::Value> = Vec::new();

        login.start();
        let phase = poll_until(
            &login,
            |p| matches!(p, CursorLoginPhase::Failed { .. }),
            Duration::from_secs(10),
            &mut observed,
        );
        match phase {
            CursorLoginPhase::Failed { ref message } => {
                assert_eq!(message.as_str(), "cursor login exited unexpectedly")
            }
            other => panic!("expected Failed, got {other:?}"),
        }

        let text = serde_json::to_string(&phase).expect("phase serializes");
        assert!(
            !text.contains(FAKE_KEY),
            "serialized phase leaks the key: {text}"
        );
        for value in &observed {
            assert!(!value.to_string().contains(FAKE_KEY));
        }
    });
}
