use super::*;
use ajax_core::agent_watcher::{TaskFrame, WatcherState};
use std::{fs, path::PathBuf, process::Command, sync::atomic::AtomicUsize};

struct Fixture {
    root: PathBuf,
    judge: LayaJudge,
}

impl Fixture {
    fn new(body: &str) -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let root = std::env::temp_dir().join(format!(
            "ajax-laya-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
        ));
        fs::create_dir_all(&root).unwrap();
        let script = root.join("fake sidecar.py");
        fs::write(&script, format!("import json, sys, time\n{body}\n")).unwrap();
        let judge = LayaJudge::new(
            LayaCommand::Argv(vec![
                "python3".into(),
                "-u".into(),
                script.to_string_lossy().into_owned(),
            ]),
            Duration::from_millis(200),
        );
        Self { root, judge }
    }

    fn ready(&self) {
        wait_until(|| self.judge.ready.load(Ordering::Acquire));
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.judge.stop.store(true, Ordering::Release);
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn wait_until(mut predicate: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !predicate() {
        assert!(
            Instant::now() < deadline,
            "sidecar did not reach expected state"
        );
        thread::sleep(POLL);
    }
}

fn snapshot(objective: &str) -> WatcherSnapshot {
    WatcherState::new(
        &TaskFrame {
            objective: objective.into(),
        },
        "task",
        "run",
        "codex",
    )
    .snapshot(1000)
}

#[test]
fn ready_sidecar_maps_every_state_and_reuses_process() {
    let fixture = Fixture::new(
        r#"
print('{"type":"ready"}', flush=True)
for count, line in enumerate(sys.stdin, 1):
    request = json.loads(line)
    assert request['id'] == count
    print(json.dumps({'id': request['id'], 'state': request['snapshot']['objective'], 'confidence': 0.83}), flush=True)
"#,
    );
    fixture.ready();
    for (label, state) in [
        ("progressing", ProgressState::Progressing),
        ("stuck", ProgressState::Stuck),
        ("off_track", ProgressState::OffTrack),
        ("needs_user", ProgressState::NeedsUser),
        ("probably_done", ProgressState::ProbablyDone),
        ("uncertain", ProgressState::Uncertain),
    ] {
        assert_eq!(
            fixture.judge.evaluate(&snapshot(label)),
            Ok(WatcherVerdict {
                state,
                confidence: 0.83
            })
        );
    }
}

#[test]
fn low_confidence_is_uncertain() {
    let fixture = Fixture::new(
        r#"
print('{"type":"ready"}', flush=True)
for line in sys.stdin:
    request = json.loads(line)
    print(json.dumps({'id': request['id'], 'state': 'stuck', 'confidence': 0.59}), flush=True)
"#,
    );
    fixture.ready();
    assert_eq!(
        fixture.judge.evaluate(&snapshot("fix")),
        Ok(WatcherVerdict {
            state: ProgressState::Uncertain,
            confidence: 0.59
        })
    );
}

#[test]
fn malformed_replies_retire_child() {
    for reply in [
        "not json",
        "{}",
        r#"{"id":1,"state":"unknown","confidence":0.9}"#,
        r#"{"id":1,"state":"stuck","confidence":1.1}"#,
        r#"{"id":1,"state":"stuck","confidence":-0.1}"#,
        r#"{"id":1,"state":"stuck","confidence":NaN}"#,
        r#"{"id":1,"state":"stuck","confidence":"0.9"}"#,
    ] {
        let fixture = Fixture::new(&format!(
            "print('{{\"type\":\"ready\"}}', flush=True)\nsys.stdin.readline()\nprint({reply:?}, flush=True)\nprint({reply:?}, flush=True)\ntime.sleep(30)"
        ));
        fixture.ready();
        assert_eq!(
            fixture.judge.evaluate(&snapshot("fix")),
            Err(JudgeError::Malformed),
            "{reply}"
        );
        assert_eq!(
            fixture.judge.evaluate(&snapshot("fix")),
            Err(JudgeError::Unavailable)
        );
    }
}

#[test]
fn oversized_or_unterminated_garbage_is_bounded() {
    let fixture = Fixture::new("print('{\"type\":\"ready\"}', flush=True)\nsys.stdin.readline()\nsys.stdout.write(('x' * 10000 + chr(10)) * 2)\nsys.stdout.flush()\ntime.sleep(30)");
    fixture.ready();
    assert_eq!(
        fixture.judge.evaluate(&snapshot("fix")),
        Err(JudgeError::Malformed)
    );
}

#[test]
fn deadline_bounds_evaluation_and_next_call_during_backoff_does_not_hang() {
    let fixture = Fixture::new("print('{\"type\":\"ready\"}', flush=True)\ntime.sleep(30)");
    fixture.ready();
    let start = Instant::now();
    assert_eq!(
        fixture.judge.evaluate(&snapshot("fix")),
        Err(JudgeError::Timeout)
    );
    assert!(start.elapsed() < Duration::from_millis(500));
    let start = Instant::now();
    assert_eq!(
        fixture.judge.evaluate(&snapshot("fix")),
        Err(JudgeError::Unavailable)
    );
    assert!(start.elapsed() < Duration::from_millis(100));
}

#[test]
fn eof_missing_and_not_ready_are_unavailable() {
    let fixture = Fixture::new("print('{\"type\":\"ready\"}', flush=True)\nsys.stdin.readline()");
    fixture.ready();
    assert_eq!(
        fixture.judge.evaluate(&snapshot("fix")),
        Err(JudgeError::Unavailable)
    );
    for code in [
        "sys.exit(1)",
        "time.sleep(30)",
        "print('{\"type\":\"error\",\"message\":\"missing laya\"}', flush=True)",
    ] {
        let fixture = Fixture::new(code);
        assert_eq!(
            fixture.judge.evaluate(&snapshot("fix")),
            Err(JudgeError::Unavailable)
        );
        thread::sleep(Duration::from_millis(100));
        assert_eq!(
            fixture.judge.evaluate(&snapshot("fix")),
            Err(JudgeError::Unavailable)
        );
    }
    let judge = LayaJudge::new(
        LayaCommand::String("/missing/ajax-laya".into()),
        Duration::from_millis(200),
    );
    assert_eq!(
        judge.evaluate(&snapshot("fix")),
        Err(JudgeError::Unavailable)
    );
}

#[test]
fn failed_sidecar_restarts_after_backoff() {
    let fixture = Fixture::new(
        r#"
from pathlib import Path
marker = Path(__file__).with_suffix('.count')
count = int(marker.read_text()) + 1 if marker.exists() else 1
marker.write_text(str(count))
print('{"type":"ready"}', flush=True)
for line in sys.stdin:
    request = json.loads(line)
    print('garbage\ngarbage' if count == 1 else json.dumps({'id': request['id'], 'state': 'progressing', 'confidence': 1.0}), flush=True)
"#,
    );
    fixture.ready();
    assert_eq!(
        fixture.judge.evaluate(&snapshot("fix")),
        Err(JudgeError::Malformed)
    );
    thread::sleep(Duration::from_millis(200));
    assert_eq!(
        fs::read_to_string(fixture.root.join("fake sidecar.count")).unwrap(),
        "1"
    );
    fixture.ready();
    assert_eq!(
        fixture.judge.evaluate(&snapshot("fix")).unwrap().state,
        ProgressState::Progressing
    );
    assert_eq!(
        fs::read_to_string(fixture.root.join("fake sidecar.count")).unwrap(),
        "2"
    );
}

#[test]
fn snapshots_are_bounded_and_include_checkpoint_without_transcripts() {
    let mut source = snapshot(&"界".repeat(10000));
    source.recent_signatures = vec!["x".repeat(10000); 100];
    source.recent_events = vec!["turn_settled".into(); 100];
    let value = compact_snapshot(&source, Some(PendingCheckpoint::Settle));
    assert_eq!(value["checkpoint"], "settle");
    assert_eq!(value["objective"].as_str().unwrap().chars().count(), 512);
    assert_eq!(value["recent_signatures"].as_array().unwrap().len(), 8);
    assert!(serde_json::to_vec(&value).unwrap().len() < 4096);
    assert!(value.get("task_id").is_none());
    for (checkpoint, label) in [
        (PendingCheckpoint::Loop, "loop"),
        (PendingCheckpoint::GraceExpiry, "grace_expiry"),
    ] {
        // Recent events can still end at the old stop when a grace heartbeat fires.
        assert_eq!(
            compact_snapshot(&source, Some(checkpoint))["checkpoint"],
            label
        );
    }
}

#[test]
fn config_selects_disabled_null_and_laya() {
    let mut config = WatcherConfig::default();
    assert_eq!(
        configured_judge(&config).unwrap()(&snapshot("fix"), None),
        Err(JudgeError::Unavailable)
    );
    config.enabled = false;
    config.laya_command = Some(LayaCommand::String("/missing/laya".into()));
    assert!(configured_judge(&config).is_none());
    let fixture = Fixture::new("time.sleep(30)");
    fs::write(fixture.root.join("configured.py"), "import json, sys\nprint('{\"type\":\"ready\"}', flush=True)\nfor line in sys.stdin:\n r=json.loads(line)\n print(json.dumps({'id':r['id'], 'state':'stuck', 'confidence':0.9}), flush=True)\n").unwrap();
    config.enabled = true;
    config.laya_command = Some(LayaCommand::Argv(vec![
        "python3".into(),
        fixture
            .root
            .join("configured.py")
            .to_string_lossy()
            .into_owned(),
    ]));
    let judge = configured_judge(&config).unwrap();
    wait_until(|| {
        judge(&snapshot("fix"), Some(PendingCheckpoint::Settle))
            .is_ok_and(|v| v.state == ProgressState::Stuck)
    });
}

fn sidecar_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scripts/ajax-laya-sidecar")
}

#[test]
fn python_sidecar_parses_laya_defensively_and_asks_one_choice() {
    let output = Command::new("python3").args(["-c", r#"
import json, runpy, sys
module = runpy.run_path(sys.argv[1])
class Fake:
    def __init__(self, result): self.result = result
    def predict(self, state, questions):
        assert json.loads(state)['checkpoint'] == 'settle'
        assert list(questions) == ['progress']
        assert questions['progress']['type'] == 'choice'
        assert 'premature' in questions['progress']['instructions']
        assert len(questions['progress']['criteria']) == 5
        print('model diagnostic that must go to stderr')
        return self.result
for result in [None, {}, {'answers': {}}, {'answers': {'progress': {}}},
               {'answers': {'progress': {'choice': 'stuck', 'confidence': float('nan')}}},
               {'answers': {'progress': {'choice': 'stuck', 'confidence': True}}}]:
    assert module['predict'](Fake(result), {'checkpoint':'settle'}) == {'state':'uncertain', 'confidence':0}
result = {'answers': {'progress': {'choice': 'stuck', 'confidence': 0.83, 'probabilities': {'stuck':0.83}}}}
assert module['predict'](Fake(result), {'checkpoint':'settle'}) == {'state':'stuck', 'confidence':0.83}
"#]).arg(sidecar_path()).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.is_empty());
}

#[test]
fn python_sidecar_missing_dependency_emits_error_and_exits() {
    let output = Command::new("python3")
        .args(["-I", "-S"])
        .arg(sidecar_path())
        .output()
        .unwrap();
    assert!(!output.status.success());
    let reply: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(reply["type"], "error");
    assert!(reply["message"].as_str().unwrap().contains("laya"));
}

#[test]
fn real_sidecar_protocol_loads_model_once_with_fake_laya() {
    let fixture = Fixture::new("time.sleep(30)");
    fs::write(
        fixture.root.join("laya.py"),
        r#"
import json
loads = 0
def load(name):
    global loads
    loads += 1
    assert loads == 1
    assert name == 'convaiinnovations/laya'
    print('loading diagnostic')
    return Agent()
class Agent:
    def predict(self, state, questions):
        assert json.loads(state)['checkpoint'] == 'grace_expiry'
        assert 'resumed' in questions['progress']['instructions']
        print('inference diagnostic')
        return {'answers': {'progress': {'choice': 'progressing', 'confidence': 0.9}}}
"#,
    )
    .unwrap();
    let judge = LayaJudge::new(LayaCommand::Argv(vec![
        "python3".into(), "-c".into(),
        "import runpy,sys; sys.path.insert(0,sys.argv[1]); runpy.run_path(sys.argv[2],run_name='__main__')".into(),
        fixture.root.to_string_lossy().into_owned(), sidecar_path().to_string_lossy().into_owned(),
    ]), Duration::from_secs(1));
    wait_until(|| judge.ready.load(Ordering::Acquire));
    for _ in 0..2 {
        let verdict = judge
            .evaluate_checkpoint(&snapshot("fix"), Some(PendingCheckpoint::GraceExpiry))
            .unwrap();
        assert_eq!(verdict.state, ProgressState::Progressing);
        assert_eq!(verdict.confidence, 0.9);
    }
}

#[test]
fn late_reply_does_not_reload_or_poison_next_request() {
    let fixture = Fixture::new(
        r#"
from pathlib import Path
marker = Path(__file__).with_suffix('.count')
count = int(marker.read_text()) + 1 if marker.exists() else 1
marker.write_text(str(count))
print('{"type":"ready"}', flush=True)
for line in sys.stdin:
    request = json.loads(line)
    if request['id'] == 1: time.sleep(0.3)
    print(json.dumps({'id':request['id'], 'state':'progressing', 'confidence':0.9}), flush=True)
"#,
    );
    fixture.ready();
    assert_eq!(
        fixture.judge.evaluate(&snapshot("fix")),
        Err(JudgeError::Timeout)
    );
    fixture.ready();
    assert_eq!(
        fixture.judge.evaluate(&snapshot("fix")).unwrap().state,
        ProgressState::Progressing
    );
    assert_eq!(
        fs::read_to_string(fixture.root.join("fake sidecar.count")).unwrap(),
        "1"
    );
}

#[test]
fn startup_error_uses_exponential_backoff() {
    let fixture = Fixture::new(
        r#"
from pathlib import Path
marker = Path(__file__).with_suffix('.count')
with marker.open('a') as file: file.write('launch\n')
print('missing package stderr', file=sys.stderr, flush=True)
print('{"type":"error","message":"missing laya"}', flush=True)
sys.exit(1)
"#,
    );
    let marker = fixture.root.join("fake sidecar.count");
    wait_until(|| marker.exists());
    thread::sleep(Duration::from_millis(700));
    assert_eq!(fs::read_to_string(&marker).unwrap().lines().count(), 1);
    wait_until(|| fs::read_to_string(&marker).unwrap().lines().count() == 2);
    thread::sleep(Duration::from_millis(1300));
    assert_eq!(fs::read_to_string(&marker).unwrap().lines().count(), 2);
}

#[test]
fn single_malformed_and_wrong_id_replies_are_discarded() {
    let fixture = Fixture::new(
        r#"
print('{"type":"ready"}', flush=True)
for line in sys.stdin:
    request = json.loads(line)
    print('garbage', flush=True)
    print(json.dumps({'id':999, 'state':'stuck', 'confidence':1}), flush=True)
    print(json.dumps({'id':request['id'], 'state':'progressing', 'confidence':0.9}), flush=True)
"#,
    );
    fixture.ready();
    for _ in 0..2 {
        assert_eq!(
            fixture.judge.evaluate(&snapshot("fix")).unwrap().state,
            ProgressState::Progressing
        );
    }
}

#[test]
fn python_sidecar_warms_up_before_ready() {
    let output = Command::new("python3")
        .args([
            "-c",
            r#"
import io, runpy, sys, types
module = runpy.run_path(sys.argv[1])
calls = []
class Fake:
    def predict(self, state, questions):
        calls.append('predict')
        return {}
sys.modules['laya'] = types.SimpleNamespace(load=lambda name: Fake())
sys.stdin = io.StringIO('')
module['main'].__globals__['emit'] = lambda value: calls.append(value['type'])
assert module['main']() == 0
assert calls == ['predict', 'ready'], calls
"#,
        ])
        .arg(sidecar_path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn successful_verdict_resets_retry_backoff() {
    let fixture = Fixture::new(
        r#"
from pathlib import Path
marker = Path(__file__).with_suffix('.count')
count = int(marker.read_text()) + 1 if marker.exists() else 1
marker.write_text(str(count))
if count < 3: sys.exit(1)
print('{"type":"ready"}', flush=True)
for line in sys.stdin:
    request = json.loads(line)
    if request['snapshot']['objective'] == 'fail':
        print('garbage\ngarbage', flush=True)
    else:
        print(json.dumps({'id':request['id'], 'state':'progressing', 'confidence':0.9}), flush=True)
"#,
    );
    fixture.ready();
    assert!(fixture.judge.evaluate(&snapshot("fix")).is_ok());
    let start = Instant::now();
    assert_eq!(
        fixture.judge.evaluate(&snapshot("fail")),
        Err(JudgeError::Malformed)
    );
    fixture.ready();
    assert!(start.elapsed() < Duration::from_millis(1800));
    assert_eq!(
        fs::read_to_string(fixture.root.join("fake sidecar.count")).unwrap(),
        "4"
    );
}

#[test]
fn retry_backoff_caps_at_sixty_seconds() {
    let (sender, inbox) = mpsc::sync_channel(1);
    drop(sender); // Exercise the delay calculation without wall-clock waiting.
    let mut backoff = BACKOFF;
    for seconds in [2, 4, 8, 16, 32, 60, 60] {
        retry_delay(
            &inbox,
            &AtomicBool::new(false),
            &mut backoff,
            JudgeError::Unavailable,
            "test",
        );
        assert_eq!(backoff, Duration::from_secs(seconds));
    }
}

#[test]
fn spawn_failures_warn_with_cause_and_empty_commands_never_retry() {
    for command in [
        LayaCommand::String("/missing/ajax-laya-d3".into()),
        LayaCommand::String("  \t ".into()),
        LayaCommand::Argv(vec![]),
        LayaCommand::Argv(vec!["  ".into()]),
    ] {
        let missing = matches!(&command, LayaCommand::String(value) if value.starts_with('/'));
        let messages = Arc::new(std::sync::Mutex::new(Vec::new()));
        let captured = messages.clone();
        let (requests, inbox) = mpsc::sync_channel(1);
        let ready = Arc::new(AtomicBool::new(false));
        let stop = Arc::new(AtomicBool::new(false));
        let worker_ready = ready.clone();
        let worker_stop = stop.clone();
        let worker = thread::spawn(move || {
            transport::tests::capture_warnings(
                || supervise(command, inbox, worker_ready, worker_stop),
                captured,
            )
        });
        wait_until(|| !messages.lock().unwrap().is_empty());
        thread::sleep(BACKOFF + Duration::from_millis(100));
        let finished = worker.is_finished();
        stop.store(true, Ordering::Release);
        worker.join().unwrap();
        let judge = LayaJudge {
            requests,
            ready,
            stop,
            timeout: POLL,
        };
        assert_eq!(
            judge.evaluate(&snapshot("fix")),
            Err(JudgeError::Unavailable)
        );
        let messages = messages.lock().unwrap();
        if missing {
            let cause = Command::new("/missing/ajax-laya-d3")
                .spawn()
                .unwrap_err()
                .to_string();
            assert!(messages[0].contains(&cause), "{messages:?}");
            assert!(messages.len() <= 2, "{messages:?}");
        } else {
            assert!(messages[0].contains("empty laya_command"), "{messages:?}");
            assert_eq!(messages.len(), 1);
            assert!(finished, "empty command must terminate the spawn loop");
        }
    }
}
