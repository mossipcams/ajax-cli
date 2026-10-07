//! Background delivery of declarative push with a bounded retry.

use super::{build_push_request, PushHub, PushSubscription};
use axum::http::Request;
use std::{process::Stdio, sync::Arc, thread::JoinHandle, time::Duration};
use web_push_native::jwt_simple::algorithms::ES256KeyPair;

// ponytail: in-process retry only; a restart drops pending pushes. Move to a
// durable outbox if missed notifications across restarts matter.
const RETRY_DELAYS: [Duration; 2] = [Duration::from_secs(5), Duration::from_secs(30)];

pub(super) struct PushJob {
    pub(super) subscription: PushSubscription,
    pub(super) payload: Vec<u8>,
    pub(super) vapid_subject: String,
}

/// Deliver off the caller's thread: the caller holds the Cockpit control
/// lane, and a slow push service must not stall it (#1230).
pub(super) fn spawn(hub: Arc<PushHub>, key_pair: ES256KeyPair, jobs: Vec<PushJob>) {
    spawn_with(
        hub,
        key_pair,
        jobs,
        &RETRY_DELAYS,
        deliver_with_curl_blocking,
    );
}

fn spawn_with(
    hub: Arc<PushHub>,
    key_pair: ES256KeyPair,
    jobs: Vec<PushJob>,
    retry_delays: &'static [Duration],
    deliver: impl Fn(Request<Vec<u8>>) -> Result<(), String> + Send + 'static,
) -> JoinHandle<()> {
    std::thread::spawn(move || {
        let dead = deliver_jobs(&key_pair, jobs, retry_delays, deliver, std::thread::sleep);
        if !dead.is_empty() {
            hub.prune_endpoints(&dead);
        }
    })
}

/// Deliver every job, retrying transient failures (#1231). Returns the
/// endpoints the push service reported gone.
fn deliver_jobs(
    key_pair: &ES256KeyPair,
    jobs: Vec<PushJob>,
    retry_delays: &[Duration],
    deliver: impl Fn(Request<Vec<u8>>) -> Result<(), String>,
    sleep: impl Fn(Duration),
) -> Vec<String> {
    let mut dead = Vec::new();
    let mut pending = jobs;
    for attempt in 0..=retry_delays.len() {
        if attempt > 0 {
            sleep(retry_delays[attempt - 1]);
        }
        let mut failed = Vec::new();
        for job in pending {
            // Rebuilt per attempt: the VAPID token is short-lived.
            let result = build_push_request(
                job.subscription.clone(),
                job.payload.clone(),
                key_pair,
                &job.vapid_subject,
            )
            .and_then(&deliver);
            match result {
                Ok(()) => {}
                Err(error) if is_gone_endpoint(&error) => {
                    dead.push(job.subscription.endpoint.clone());
                }
                Err(error) => {
                    eprintln!("declarative push delivery failed: {error}");
                    failed.push(job);
                }
            }
        }
        if failed.is_empty() {
            break;
        }
        pending = failed;
    }
    dead
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::slices::push::{PushSubscriptionKeys, UnsubscribeRequest};
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        mpsc, Mutex,
    };

    fn job(endpoint: &str) -> PushJob {
        PushJob {
            subscription: PushSubscription {
                endpoint: endpoint.to_string(),
                keys: PushSubscriptionKeys {
                    p256dh: "BLn9b-VR0ca83knDNZ32dCHGyjJp-1riX9ZTN40MqV8K_LpQmLqxC_DoHvqvFXO_nGdAB4W9dogZb_sM-uV4JbY".to_string(),
                    auth: "_ordMnz7uTCmrpBTeUV4Bw".to_string(),
                },
                navigate: None,
            },
            payload: b"{}".to_vec(),
            vapid_subject: "https://cockpit.example".to_string(),
        }
    }

    const APPLE: &str = "https://web.push.apple.com/messages/1";
    const DELAYS: [Duration; 2] = [Duration::from_secs(5), Duration::from_secs(30)];

    #[test]
    fn issue_1231_transient_delivery_failure_is_retried() {
        let calls = AtomicUsize::new(0);
        let slept = Mutex::new(Vec::new());
        let dead = deliver_jobs(
            &ES256KeyPair::generate(),
            vec![job(APPLE)],
            &DELAYS,
            |_| match calls.fetch_add(1, Ordering::SeqCst) {
                0 => Err("push delivery failed with exit status: 28: timed out".to_string()),
                _ => Ok(()),
            },
            |delay| slept.lock().unwrap().push(delay),
        );
        assert!(dead.is_empty());
        assert_eq!(
            calls.load(Ordering::SeqCst),
            2,
            "failed push must be retried"
        );
        assert_eq!(*slept.lock().unwrap(), vec![DELAYS[0]]);
    }

    #[test]
    fn retries_are_bounded_and_gone_endpoints_are_not_retried() {
        let calls = AtomicUsize::new(0);
        let dead = deliver_jobs(
            &ES256KeyPair::generate(),
            vec![job(APPLE), job("https://fcm.googleapis.com/fcm/send/gone")],
            &DELAYS,
            |request| {
                calls.fetch_add(1, Ordering::SeqCst);
                if request.uri().to_string().contains("gone") {
                    Err("The requested URL returned error: 410".to_string())
                } else {
                    Err("could not resolve host".to_string())
                }
            },
            |_| {},
        );
        assert_eq!(dead, vec!["https://fcm.googleapis.com/fcm/send/gone"]);
        assert_eq!(calls.load(Ordering::SeqCst), 1 + DELAYS.len() + 1);
    }

    #[test]
    fn issue_1230_delivery_does_not_block_the_caller() {
        let hub = PushHub::ephemeral();
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel::<()>();
        let release_rx = Mutex::new(release_rx);
        // Returning at all while the endpoint hangs is the regression check.
        let worker = spawn_with(
            Arc::clone(&hub),
            ES256KeyPair::generate(),
            vec![job(APPLE)],
            &[],
            move |_| {
                started_tx.send(()).unwrap();
                release_rx.lock().unwrap().recv().unwrap();
                Ok(())
            },
        );
        started_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("delivery runs in the background");
        // The hub stays usable while delivery is still in flight.
        hub.apply_unsubscribe(&UnsubscribeRequest {
            endpoint: None,
            all: true,
        })
        .unwrap();
        release_tx.send(()).unwrap();
        worker.join().unwrap();
    }
}
