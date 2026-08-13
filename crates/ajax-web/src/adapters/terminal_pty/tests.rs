use super::*;
use axum::extract::ws::Message;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::time::{Duration, Instant};

fn attach_plan(handle: &str) -> TerminalAttachPlan {
    TerminalAttachPlan {
        qualified_handle: handle.to_string(),
        tmux_session: "ajax-web-fix-login".to_string(),
        task_window: "task".to_string(),
    }
}

/// One configurable stand-in for a spawned PTY child: records kill/wait
/// calls and, when gated, blocks `wait_child` until the returned sender is
/// dropped or signalled.
#[derive(Clone)]
pub(crate) struct MockChild {
    killed: Arc<Mutex<bool>>,
    waited: Arc<Mutex<bool>>,
    wait_gate: Option<Arc<Mutex<std::sync::mpsc::Receiver<()>>>>,
}

impl MockChild {
    fn instant() -> Self {
        Self {
            killed: Arc::new(Mutex::new(false)),
            waited: Arc::new(Mutex::new(false)),
            wait_gate: None,
        }
    }

    pub(crate) fn gated() -> (Self, std::sync::mpsc::Sender<()>) {
        let (release, receiver) = std::sync::mpsc::channel();
        let mut child = Self::instant();
        child.wait_gate = Some(Arc::new(Mutex::new(receiver)));
        (child, release)
    }
}

impl TerminalChild for MockChild {
    fn kill_child(&mut self) -> std::io::Result<()> {
        *self.killed.lock().unwrap() = true;
        Ok(())
    }

    fn wait_child(&mut self) -> std::io::Result<()> {
        if let Some(gate) = &self.wait_gate {
            let receiver = gate.lock().unwrap();
            let _ = receiver.recv();
        }
        *self.waited.lock().unwrap() = true;
        Ok(())
    }
}

#[test]
fn tmux_attach_command_plan_uses_registered_session_and_task_target() {
    let plan = attach_plan("web/fix-login");

    let command_plan = build_tmux_attach_command_plan(&plan);

    assert_eq!(command_plan.program, "tmux");
    assert_eq!(
        command_plan.args,
        vec![
            "attach-session".to_string(),
            "-t".to_string(),
            "ajax-web-fix-login:task".to_string(),
        ]
    );
    assert!(!command_plan
        .args
        .iter()
        .any(|arg| arg.contains("web/fix-login")));
}

#[test]
fn tmux_attach_target_never_uses_browser_handle() {
    let plan = attach_plan("web/evil-handle");

    let command_plan = build_tmux_attach_command_plan(&plan);

    assert_eq!(command_plan.args[2], "ajax-web-fix-login:task");
    assert!(!command_plan
        .args
        .iter()
        .any(|arg| arg.contains("evil-handle")));
}

#[test]
fn task_window_probe_targets_ephemeral_session_window() {
    let command = task_window_probe_command("ajax-web-fix-login-m1a2b3c", "task");

    assert_eq!(command.program, "tmux");
    assert_eq!(
        command.args,
        vec![
            "display-message".to_string(),
            "-p".to_string(),
            "-t".to_string(),
            "ajax-web-fix-login-m1a2b3c:task".to_string(),
            "#{window_id}".to_string(),
        ]
    );
}

#[test]
fn tmux_attach_command_uses_clear_capable_terminal_type() {
    let plan = attach_plan("web/fix-login");
    let command_plan = build_tmux_attach_command_plan(&plan);

    let command = build_tmux_attach_command(&command_plan);

    assert_eq!(
        command.get_env("TERM"),
        Some(std::ffi::OsStr::new("xterm-256color"))
    );
}

#[test]
fn isolated_attach_plan_creates_grouped_session_then_attaches() {
    let plan = attach_plan("web/fix-login");

    let isolated = build_isolated_attach_plan_with_token(&plan, "1a2b3c");
    let ephemeral = "ajax-web-fix-login-m1a2b3c";

    assert_eq!(isolated.ephemeral_session, ephemeral);
    // A grouped session shares the shared session's windows but keeps an
    // independent size, so the phone never shrinks the shared window.
    // Quieter status options target the ephemeral session only.
    assert_eq!(
        isolated.setup,
        vec![
            TmuxCommand {
                program: "tmux".to_string(),
                args: vec![
                    "new-session".to_string(),
                    "-d".to_string(),
                    "-s".to_string(),
                    ephemeral.to_string(),
                    "-t".to_string(),
                    "ajax-web-fix-login".to_string(),
                ],
            },
            TmuxCommand {
                program: "tmux".to_string(),
                args: vec![
                    "set-option".to_string(),
                    "-t".to_string(),
                    ephemeral.to_string(),
                    "status-interval".to_string(),
                    "5".to_string(),
                ],
            },
            TmuxCommand {
                program: "tmux".to_string(),
                args: vec![
                    "set-option".to_string(),
                    "-t".to_string(),
                    ephemeral.to_string(),
                    "visual-activity".to_string(),
                    "off".to_string(),
                ],
            },
            TmuxCommand {
                program: "tmux".to_string(),
                args: vec![
                    "set-option".to_string(),
                    "-t".to_string(),
                    ephemeral.to_string(),
                    "visual-bell".to_string(),
                    "off".to_string(),
                ],
            },
        ]
    );
    let set_option_targets: Vec<&str> = isolated
        .setup
        .iter()
        .filter(|cmd| cmd.args.first().map(String::as_str) == Some("set-option"))
        .filter_map(|cmd| {
            cmd.args
                .windows(2)
                .find(|pair| pair[0] == "-t")
                .map(|pair| pair[1].as_str())
        })
        .collect();
    assert_eq!(set_option_targets, vec![ephemeral, ephemeral, ephemeral]);
    assert!(!set_option_targets.contains(&"ajax-web-fix-login"));
    // Attach targets the ephemeral session's task window, never the
    // browser handle and never the shared session directly.
    assert_eq!(
        isolated.attach.args,
        vec![
            "attach-session".to_string(),
            "-t".to_string(),
            format!("{ephemeral}:task"),
        ]
    );
    assert!(!isolated
        .attach
        .args
        .iter()
        .any(|arg| arg == "ajax-web-fix-login:task"));
    assert!(!isolated
        .attach
        .args
        .iter()
        .any(|arg| arg.contains("web/fix-login")));
}

#[test]
fn seed_history_query_parsing() {
    assert!(seed_history_from_query(None));
    assert!(seed_history_from_query(Some("")));
    assert!(!seed_history_from_query(Some("seed=0")));
    assert!(!seed_history_from_query(Some("a=b&seed=0")));
    assert!(seed_history_from_query(Some("seed=1")));
    assert!(seed_history_from_query(Some("seed=00")));
}

#[test]
fn client_id_from_query_parsing() {
    // Absent / empty / no client= -> None (bridge falls back to random plan).
    assert_eq!(client_id_from_query(None), None);
    assert_eq!(client_id_from_query(Some("")), None);
    assert_eq!(client_id_from_query(Some("foo=bar")), None);
    // Empty client= value -> None.
    assert_eq!(client_id_from_query(Some("client=")), None);
    assert_eq!(client_id_from_query(Some("a=b&client=")), None);
    // First matching client= wins.
    assert_eq!(
        client_id_from_query(Some("client=viewport-a")),
        Some("viewport-a".to_string())
    );
    assert_eq!(
        client_id_from_query(Some("seed=0&client=abc")),
        Some("abc".to_string())
    );
    assert_eq!(
        client_id_from_query(Some("client=one&client=two")),
        Some("one".to_string())
    );
    // Allowlist: [A-Za-z0-9_-]{1,64}.
    assert_eq!(
        client_id_from_query(Some("client=Ab_1-2")),
        Some("Ab_1-2".to_string())
    );
    // Anything outside the allowlist -> None (no Injection into tmux names).
    assert_eq!(client_id_from_query(Some("client=view/port")), None);
    assert_eq!(client_id_from_query(Some("client=view port")), None);
    assert_eq!(client_id_from_query(Some("client=view%2Fport")), None);
    assert_eq!(client_id_from_query(Some("client=;evil")), None);
    // Over 64 chars -> None.
    let too_long = format!("client={}", "x".repeat(65));
    assert_eq!(client_id_from_query(Some(&too_long)), None);
    let just_right_src = format!("client={}", "y".repeat(64));
    let just_right_token = just_right_src["client=".len()..].to_string();
    assert_eq!(
        client_id_from_query(Some(&just_right_src)),
        Some(just_right_token)
    );
}

#[test]
fn isolated_plan_for_bridge_uses_stable_plan_when_client_id_present() {
    let plan = attach_plan("web/fix-login");

    // Present client id -> stable per-client plan (reconnect reuses it).
    let a = isolated_plan_for_bridge(&plan, Some("viewport-a"));
    let b = isolated_plan_for_bridge(&plan, Some("viewport-a"));
    assert_eq!(a.ephemeral_session, b.ephemeral_session);
    assert!(a.ephemeral_session.starts_with("ajax-web-fix-login-m"));

    // Different ids -> different ephemeral sessions.
    let c = isolated_plan_for_bridge(&plan, Some("viewport-b"));
    assert_ne!(a.ephemeral_session, c.ephemeral_session);

    // None -> random-per-call path (unique each call, never the shared session).
    let r1 = isolated_plan_for_bridge(&plan, None).ephemeral_session;
    let r2 = isolated_plan_for_bridge(&plan, None).ephemeral_session;
    assert_ne!(r1, r2);
    assert_ne!(r1, "ajax-web-fix-login");
}

#[test]
fn should_wait_reflow_before_seed_only_when_both_true() {
    assert!(!should_wait_reflow_before_seed(false, false));
    assert!(!should_wait_reflow_before_seed(false, true));
    assert!(!should_wait_reflow_before_seed(true, false));
    assert!(should_wait_reflow_before_seed(true, true));
}

#[test]
fn resize_settle_deadline_returns_remaining_until_quiet_elapsed() {
    let t0 = Instant::now();
    assert_eq!(
        resize_settle_deadline(t0, t0),
        Some(Duration::from_millis(150))
    );
    assert_eq!(
        resize_settle_deadline(t0, t0 + Duration::from_millis(149)),
        Some(Duration::from_millis(1))
    );
    assert_eq!(
        resize_settle_deadline(t0, t0 + Duration::from_millis(150)),
        None
    );
    assert_eq!(
        resize_settle_deadline(t0, t0 + Duration::from_millis(200)),
        None
    );
}

#[test]
fn remaining_resize_wait_deadline() {
    let started = Instant::now();
    assert_eq!(
        remaining_resize_wait(started, started),
        Some(Duration::from_millis(500))
    );
    assert_eq!(
        remaining_resize_wait(started, started + Duration::from_millis(499)),
        Some(Duration::from_millis(1))
    );
    assert_eq!(
        remaining_resize_wait(started, started + Duration::from_millis(500)),
        None
    );
    assert_eq!(
        remaining_resize_wait(started, started + Duration::from_millis(501)),
        None
    );
}

#[test]
fn isolated_attach_plan_seeds_browser_scrollback_from_task_window() {
    let plan = attach_plan("web/fix-login");

    let isolated = build_isolated_attach_plan_with_token(&plan, "1a2b3c");

    assert_eq!(isolated.history.program, "tmux");
    assert_eq!(
        isolated.history.args,
        vec![
            "capture-pane",
            "-p",
            "-e",
            "-t",
            "ajax-web-fix-login-m1a2b3c:task",
            "-S",
            "-10000",
            "-E",
            "-1",
        ]
    );
    assert!(!isolated
        .history
        .args
        .iter()
        .any(|arg| arg.contains("web/fix-login")));
}

#[test]
fn history_capture_preserves_display_wrapping() {
    let plan = attach_plan("web/fix-login");
    let isolated = build_isolated_attach_plan_with_token(&plan, "1a2b3c");
    // Display-row capture must match the browser's wrap width; -J joins
    // logical lines and re-wraps badly after seed.
    assert!(!isolated.history.args.contains(&"-J".to_string()));
}

#[test]
fn reaper_targets_only_ephemeral_grouped_sessions() {
    let names = vec![
        "ajax-web-x".to_string(),
        "ajax-web-x-m0123456789ab".to_string(),
        "ajax-web-main".to_string(),
        "other".to_string(),
        // Wrong token length must not match a real session ending in -m...
        "ajax-web-x-mabc".to_string(),
    ];

    let targets = ephemeral_sessions_to_reap(&names);

    assert_eq!(targets, vec!["ajax-web-x-m0123456789ab".to_string()]);
}

#[test]
fn isolated_attach_cleanup_kills_ephemeral_session() {
    let plan = attach_plan("web/fix-login");

    // Random (no client id): teardown destroys — token cannot be reused.
    let random = build_isolated_attach_plan(&plan);
    assert_eq!(
        random.teardown,
        destroy_ephemeral_session_commands(&random.ephemeral_session)
    );

    // Stable client id: linger empty teardown; reaper / destroy path only.
    let client = build_isolated_attach_plan_for_client(&plan, "viewport-a");
    assert!(client.teardown.is_empty());
    assert_eq!(
        destroy_ephemeral_session_commands("ajax-web-fix-login-m1a2b3c"),
        vec![TmuxCommand {
            program: "tmux".to_string(),
            args: vec![
                "kill-session".to_string(),
                "-t".to_string(),
                "ajax-web-fix-login-m1a2b3c".to_string(),
            ],
        }]
    );
}

#[test]
fn reaper_detached_skips_attached_ephemeral_sessions() {
    let rows = vec![
        ("ajax-web-x".to_string(), 1),
        ("ajax-web-x-m0123456789ab".to_string(), 0),
        ("ajax-web-y-mabcdef012345".to_string(), 2),
        ("ajax-web-z-mdeadbeefcafe".to_string(), 0),
    ];
    assert_eq!(
        ephemeral_sessions_to_reap_detached(&rows, None),
        vec![
            "ajax-web-x-m0123456789ab".to_string(),
            "ajax-web-z-mdeadbeefcafe".to_string(),
        ]
    );
}

#[test]
fn reaper_detached_skips_lingered_reconnect_target() {
    let rows = vec![
        ("ajax-web-x-m0123456789ab".to_string(), 0),
        ("ajax-web-y-mabcdef012345".to_string(), 0),
        ("ajax-web-z-mdeadbeefcafe".to_string(), 0),
    ];
    assert_eq!(
        ephemeral_sessions_to_reap_detached(&rows, Some("ajax-web-y-mabcdef012345")),
        vec![
            "ajax-web-x-m0123456789ab".to_string(),
            "ajax-web-z-mdeadbeefcafe".to_string(),
        ]
    );
}

#[test]
fn ephemeral_client_token_normalizes_stable_ids() {
    // Same non-empty id -> same 12 lowercase hex token twice.
    let a = ephemeral_client_token("viewport-a");
    let b = ephemeral_client_token("viewport-a");
    assert_eq!(a, b);
    assert_eq!(a.len(), 12);
    assert!(a
        .bytes()
        .all(|c| (c as char).is_ascii_hexdigit() && !c.is_ascii_uppercase()));

    // Trimming means surrounding whitespace does not change the token.
    assert_eq!(
        ephemeral_client_token("viewport-a"),
        ephemeral_client_token("  viewport-a  ")
    );

    // Different ids -> different tokens.
    assert_ne!(
        ephemeral_client_token("viewport-a"),
        ephemeral_client_token("viewport-b")
    );

    // Empty / whitespace-only id falls back to random-looking 12-hex
    // tokens; callers without a client id stay unique per call.
    let empty = ephemeral_client_token("");
    let ws = ephemeral_client_token("   \t");
    assert_eq!(empty.len(), 12);
    assert_eq!(ws.len(), 12);
    assert!(ephemeral_client_token("")
        .bytes()
        .all(|c| (c as char).is_ascii_hexdigit() && !c.is_ascii_uppercase()));
}

#[test]
fn setup_ignores_duplicate_session_on_new_session_only() {
    let new_session = TmuxCommand::new(["new-session", "-d", "-s", "sess", "-t", "parent"]);
    assert!(should_ignore_setup_failure(
        &new_session,
        "duplicate session: sess",
    ));
    assert!(!should_ignore_setup_failure(
        &new_session,
        "open terminal failed: not a terminal",
    ));
    let set_option = TmuxCommand::new(["set-option", "-t", "sess", "status-interval", "5"]);
    assert!(!should_ignore_setup_failure(
        &set_option,
        "duplicate session: sess",
    ));
}

#[test]
fn isolated_attach_plan_is_stable_for_same_client_token() {
    let plan = attach_plan("web/fix-login");

    let first = build_isolated_attach_plan_for_client(&plan, "viewport-a");
    let second = build_isolated_attach_plan_for_client(&plan, "viewport-a");

    assert_eq!(first.ephemeral_session, second.ephemeral_session);
    assert!(first.ephemeral_session.starts_with("ajax-web-fix-login-m"));
    assert_eq!(first.ephemeral_session, second.ephemeral_session);

    let other = build_isolated_attach_plan_for_client(&plan, "viewport-b");
    assert_ne!(first.ephemeral_session, other.ephemeral_session);

    // Stable plan still uses the idempotent create-or-attach setup and an
    // empty disconnect teardown.
    assert!(first.teardown.is_empty());
    assert_eq!(first.setup, second.setup);
    assert!(first.setup.iter().any(|cmd| {
        cmd.args.first().map(String::as_str) == Some("new-session")
            && cmd.args.iter().any(|arg| arg == "-d")
    }));
}

#[test]
fn isolated_attach_sessions_are_unique_per_call_and_never_the_shared_session() {
    // The no-client / random path must stay unique per call.
    let plan = attach_plan("web/fix-login");

    let first = build_isolated_attach_plan(&plan).ephemeral_session;
    let second = build_isolated_attach_plan(&plan).ephemeral_session;

    assert_ne!(first, second);
    assert_ne!(first, "ajax-web-fix-login");
    assert!(first.starts_with("ajax-web-fix-login-m"));
}

#[test]
fn terminal_output_flush_constants_match_targets() {
    assert_eq!(TERMINAL_OUTPUT_FLUSH_MS, 16);
    assert_eq!(TERMINAL_OUTPUT_MAX_BYTES, 16 * 1024);
}

#[test]
fn terminal_output_frame_bytes_returns_raw_bytes_for_binary_send() {
    let bytes = output_frame_bytes(b"hello".to_vec()).expect("non-empty bytes");
    assert_eq!(bytes, b"hello");
    // Live path sends Message::Binary(raw); must not base64-wrap or JSON-wrap.
    assert!(!String::from_utf8_lossy(&bytes).contains("\"type\""));
    assert!(!String::from_utf8_lossy(&bytes).contains("output"));
    assert!(output_frame_bytes(Vec::new()).is_none());
}

#[test]
fn captured_history_frame_bytes_converts_lf_to_crlf_without_doubling_crlf() {
    // Mixed ANSI, bare LF, CRLF, consecutive bare LF, and lone CR.
    let input = b"\x1b[31mred\x1b[0m\ncrlf\r\n\n\rkeep".to_vec();
    let out = captured_history_frame_bytes(input).expect("non-empty history");
    assert_eq!(out, b"\x1b[31mred\x1b[0m\r\ncrlf\r\n\r\n\rkeep");
    // Bare LF -> CRLF; existing CRLF stays one CRLF; consecutive lines start at col 0.
    assert_eq!(&out[12..14], b"\r\n");
    assert_eq!(&out[18..20], b"\r\n");
    assert_eq!(&out[20..22], b"\r\n");
    assert_eq!(out[22], b'\r');
    assert!(captured_history_frame_bytes(Vec::new()).is_none());
}

#[test]
fn captured_history_frame_bytes_does_not_append_pad_crlfs() {
    assert_eq!(
        captured_history_frame_bytes(b"a\nb".to_vec()),
        Some(b"a\r\nb".to_vec())
    );

    let out = captured_history_frame_bytes(b"x\n".to_vec()).expect("non-empty history");
    assert_eq!(out, b"x\r\n");
    assert!(!out.ends_with(b"\r\n\r\n"));

    assert!(captured_history_frame_bytes(Vec::new()).is_none());
}

#[test]
fn filter_scrollback_hostile_sequences_strips_targets_and_carries_split_sequences() {
    let mut carry = Vec::new();
    let output = filter_scrollback_hostile_sequences(
        &mut carry,
        b"\x1b[?1049h\x1b[55;1Hdialog\x1b[3J\x1b[?1006h\x1b[?1049l",
    );
    assert_eq!(output, b"\x1b[55;1Hdialog");
    assert!(carry.is_empty());

    let mut carry = Vec::new();
    assert_eq!(
        filter_scrollback_hostile_sequences(&mut carry, b"\x1b[2J\x1b[J\x1b[12;4Hhi"),
        b"\x1b[2J\x1b[J\x1b[12;4Hhi"
    );
    assert!(carry.is_empty());

    let mut carry = Vec::new();
    assert_eq!(
        filter_scrollback_hostile_sequences(&mut carry, b"pre\x1b[?104"),
        b"pre"
    );
    assert!(!carry.is_empty());
    assert_eq!(
        filter_scrollback_hostile_sequences(&mut carry, b"9hpost"),
        b"post"
    );
    assert!(carry.is_empty());

    let mut carry = Vec::new();
    assert_eq!(
        filter_scrollback_hostile_sequences(&mut carry, b"\x1b[?104"),
        b""
    );
    assert_eq!(
        filter_scrollback_hostile_sequences(&mut carry, b"8hX"),
        b"\x1b[?1048hX"
    );
    assert!(carry.is_empty());
}

#[test]
fn filter_strips_hostile_sequences_fed_one_byte_at_a_time_without_prefix_leaks() {
    // A PTY read can split an escape sequence at any byte. Feeding the
    // stream byte-by-byte is the worst case: every hostile sequence must
    // still vanish completely and every normal byte must still come out.
    let stream: &[u8] = b"a\x1b[?1049h\x1b[2Jb\x1b[?1002lc\x1b[3J\x1b[31md";
    let mut carry = Vec::new();
    let mut output = Vec::new();
    for byte in stream {
        output.extend(filter_scrollback_hostile_sequences(&mut carry, &[*byte]));
    }
    assert_eq!(output, b"a\x1b[2Jbc\x1b[31md");
    assert!(carry.is_empty());
}

fn counting_sink() -> (
    std::sync::Arc<std::sync::atomic::AtomicUsize>,
    std::sync::Arc<dyn Fn() + Send + Sync>,
) {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let counter = std::sync::Arc::new(AtomicUsize::new(0));
    let counter_clone = std::sync::Arc::clone(&counter);
    let sink: std::sync::Arc<dyn Fn() + Send + Sync> = std::sync::Arc::new(move || {
        counter_clone.fetch_add(1, Ordering::SeqCst);
    });
    (counter, sink)
}

#[test]
fn handle_input_frame_accepts_resize_without_data() {
    let mut writer: Vec<u8> = Vec::new();

    let outcome = handle_input_frame(r#"{"type":"resize","cols":132,"rows":40}"#, &mut writer)
        .expect("resize frame should parse");
    let size = match outcome {
        TextFrameOutcome::Resize(size) => size,
        _ => panic!("resize frame should return a pty size"),
    };

    assert_eq!(size.cols, 132);
    assert_eq!(size.rows, 40);
}

#[test]
fn process_client_frame_fires_sink_once_for_binary_input_within_limit() {
    let (counter, sink) = counting_sink();
    let mut writer: Vec<u8> = Vec::new();
    let frame = Message::Binary(axum::body::Bytes::from(b"hello".to_vec()));

    let outcome = process_client_frame(&frame, &mut writer, &sink);

    assert!(matches!(outcome, FrameOutcome::Handled), "{outcome:?}");
    assert_eq!(writer, b"hello");
    assert_eq!(counter.load(std::sync::atomic::Ordering::SeqCst), 1);
}

#[test]
fn process_client_frame_fires_sink_once_for_text_input_frame() {
    let (counter, sink) = counting_sink();
    let mut writer: Vec<u8> = Vec::new();
    let frame = Message::Text(r#"{"type":"input","data":"x"}"#.into());

    let outcome = process_client_frame(&frame, &mut writer, &sink);

    assert!(matches!(outcome, FrameOutcome::Handled), "{outcome:?}");
    assert_eq!(writer, b"x");
    assert_eq!(counter.load(std::sync::atomic::Ordering::SeqCst), 1);
}

#[test]
fn process_client_frame_resize_does_not_fire_sink() {
    let (counter, sink) = counting_sink();
    let mut writer: Vec<u8> = Vec::new();
    let frame = Message::Text(r#"{"type":"resize","cols":80,"rows":24}"#.into());

    let outcome = process_client_frame(&frame, &mut writer, &sink);

    match outcome {
        FrameOutcome::Resize(size) => {
            assert_eq!(size.cols, 80);
            assert_eq!(size.rows, 24);
        }
        _ => panic!("expected resize outcome, got {outcome:?}"),
    }
    assert_eq!(counter.load(std::sync::atomic::Ordering::SeqCst), 0);
    assert!(writer.is_empty());
}

#[test]
fn process_client_frame_malformed_text_does_not_fire_sink() {
    let (counter, sink) = counting_sink();
    let mut writer: Vec<u8> = Vec::new();
    let frame = Message::Text("not json".into());

    let outcome = process_client_frame(&frame, &mut writer, &sink);

    assert!(matches!(outcome, FrameOutcome::Handled), "{outcome:?}");
    assert!(writer.is_empty());
    assert_eq!(counter.load(std::sync::atomic::Ordering::SeqCst), 0);
}

#[test]
fn process_client_frame_oversized_binary_aborts_without_firing_sink() {
    let (counter, sink) = counting_sink();
    let mut writer: Vec<u8> = Vec::new();
    let big = vec![b'a'; MAX_INPUT_FRAME_BYTES + 1];
    let frame = Message::Binary(axum::body::Bytes::from(big));

    let outcome = process_client_frame(&frame, &mut writer, &sink);

    assert!(matches!(outcome, FrameOutcome::Abort), "{outcome:?}");
    assert!(writer.is_empty());
    assert_eq!(counter.load(std::sync::atomic::Ordering::SeqCst), 0);
}

#[test]
fn cleanup_spawned_child_kills_and_waits() {
    let child = MockChild::instant();
    let killed = Arc::clone(&child.killed);
    let waited = Arc::clone(&child.waited);

    cleanup_spawned_child(child);

    assert!(*killed.lock().unwrap());
    assert!(*waited.lock().unwrap());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn terminal_cleanup_runs_wait_on_blocking_task() {
    let (child, release_tx) = MockChild::gated();
    let killed = Arc::clone(&child.killed);
    let progress = Arc::new(AtomicBool::new(false));
    let progress_for_task = Arc::clone(&progress);

    let cleanup = tokio::spawn(async move {
        cleanup_spawned_child_async(child).await;
    });

    tokio::time::sleep(Duration::from_millis(20)).await;
    tokio::spawn(async move {
        progress_for_task.store(true, Ordering::Relaxed);
    })
    .await
    .expect("concurrent async task should run while cleanup waits");

    assert!(
        progress.load(Ordering::Relaxed),
        "tokio worker should stay responsive while child wait runs on a blocking thread"
    );
    assert!(*killed.lock().unwrap());

    release_tx.send(()).expect("release blocked child wait");
    cleanup.await.expect("cleanup task should finish");
}

#[tokio::test]
async fn terminal_cleanup_does_not_wait_forever_after_kill() {
    // The release sender is held for the whole test, so wait_child never
    // completes on its own; only the cleanup timeout can end it.
    let (child, _release) = MockChild::gated();
    let killed = Arc::clone(&child.killed);
    let timeout = Duration::from_millis(50);

    let started = std::time::Instant::now();
    cleanup_spawned_child_async_with_timeout(child, timeout).await;
    let elapsed = started.elapsed();

    assert!(*killed.lock().unwrap());
    assert!(
        elapsed < Duration::from_millis(250),
        "cleanup should time out instead of waiting forever, took {elapsed:?}"
    );
}
