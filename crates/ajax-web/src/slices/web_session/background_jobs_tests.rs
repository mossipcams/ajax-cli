//! Background-job regressions (issue #1412). Clocks are injected and run-id sets
//! are synthetic: nothing here spawns a process or runs `ps`.

use super::background_jobs::{run_ids, BackgroundJobs, END_DEBOUNCE, POLL_EVERY};
use super::session_activity::{SessionActivity, SessionActivityReporter};
use super::SessionServerEvent;
use ajax_core::acp_execution_state::AcpExecutionEvent;
use ajax_core::models::{LiveObservation, LiveStatusKind};
use std::cell::RefCell;
use std::collections::BTreeSet;
use std::time::{Duration, Instant, SystemTime};

fn live(ids: &[&str]) -> BTreeSet<String> {
    ids.iter().map(|id| id.to_string()).collect()
}

fn prompt() -> SessionServerEvent {
    SessionServerEvent::PromptAccepted {
        client_message_id: "c1".to_string(),
    }
}

fn turn_end() -> SessionServerEvent {
    SessionServerEvent::TurnEnd {
        stop_reason: Some("end_turn".to_string()),
    }
}

/// Play the actor's steps for one poll: the tracker, then the reporter, committing what is reported.
fn poll(
    jobs: &mut BackgroundJobs,
    reporter: &mut SessionActivityReporter,
    ids: &BTreeSet<String>,
    at: Instant,
) -> Option<SessionActivity> {
    let events = jobs.observe(ids, at);
    if events.is_empty() {
        return None;
    }
    let activity = reporter.activity_for_batch(events, SystemTime::now())?;
    reporter.commit(activity);
    Some(activity)
}

fn event(reporter: &mut SessionActivityReporter, event: &SessionServerEvent) -> SessionActivity {
    let activity = reporter.activity_for_event(event).expect("activity");
    reporter.commit(activity);
    activity
}

/// A turn that ended while one background shell is alive, with the tracker's first poll already run.
fn turn_ended_with_job(at: Instant) -> (BackgroundJobs, SessionActivityReporter) {
    let mut reporter = SessionActivityReporter::default();
    event(&mut reporter, &prompt());
    assert_eq!(
        event(&mut reporter, &turn_end()),
        SessionActivity::TurnEnded
    );
    let mut jobs = BackgroundJobs::default();
    jobs.note_turn_ended();
    assert_eq!(
        poll(&mut jobs, &mut reporter, &live(&["bg-20-tok"]), at),
        Some(SessionActivity::BackgroundRunning)
    );
    (jobs, reporter)
}

#[test]
fn turn_end_with_a_live_job_reads_as_background_work_not_response_ready() {
    let (jobs, reporter) = turn_ended_with_job(Instant::now());

    assert!(jobs.is_tracking());
    assert_eq!(
        SessionActivity::BackgroundRunning.observation(),
        LiveObservation::new(LiveStatusKind::AgentRunning, "Background work running")
    );
    assert!(!reporter.turn_running());
}

#[test]
fn a_job_gone_for_less_than_the_debounce_keeps_background_work_running() {
    let start = Instant::now();
    let (mut jobs, mut reporter) = turn_ended_with_job(start);

    assert_eq!(
        poll(&mut jobs, &mut reporter, &live(&[]), start + POLL_EVERY),
        None
    );
    assert_eq!(
        poll(&mut jobs, &mut reporter, &live(&[]), start + 2 * POLL_EVERY),
        None
    );
    assert!(jobs.is_tracking());
}

#[test]
fn a_job_gone_for_the_debounce_reports_response_ready() {
    let start = Instant::now();
    let (mut jobs, mut reporter) = turn_ended_with_job(start);

    assert_eq!(
        poll(&mut jobs, &mut reporter, &live(&[]), start + POLL_EVERY),
        None
    );
    assert_eq!(
        poll(&mut jobs, &mut reporter, &live(&[]), start + 2 * POLL_EVERY),
        None
    );
    assert_eq!(
        poll(&mut jobs, &mut reporter, &live(&[]), start + END_DEBOUNCE),
        Some(SessionActivity::TurnEnded)
    );
    assert_eq!(
        SessionActivity::TurnEnded.observation(),
        LiveObservation::new(LiveStatusKind::Done, "Response ready")
    );
    assert!(!jobs.is_tracking());
}

#[test]
fn a_new_turn_while_jobs_are_tracked_shows_the_turn_status() {
    let start = Instant::now();
    let (mut jobs, mut reporter) = turn_ended_with_job(start);

    assert_eq!(
        event(&mut reporter, &prompt()),
        SessionActivity::TurnStarted
    );
    assert!(reporter.turn_running());
    assert!(jobs.is_tracking());
    assert!(!jobs.poll_due(reporter.turn_running(), start + 10 * POLL_EVERY));
    assert_eq!(
        poll(
            &mut jobs,
            &mut reporter,
            &live(&["bg-20-tok"]),
            start + POLL_EVERY
        ),
        None
    );
}

#[test]
fn the_tracker_starts_each_job_once_and_ends_it_once() {
    let start = Instant::now();
    let mut jobs = BackgroundJobs::default();
    let started = AcpExecutionEvent::ChildStarted {
        run_id: "bg-20-tok".to_string(),
        completion_holding: true,
    };
    let ended = AcpExecutionEvent::ChildEnded {
        run_id: "bg-20-tok".to_string(),
        completion_holding: true,
    };

    let mut emitted = Vec::new();
    let steps = [
        (live(&["bg-20-tok"]), 0),
        (live(&["bg-20-tok"]), 2),
        (live(&[]), 4),
        (live(&[]), 6),
        (live(&[]), 8),
        (live(&[]), 10),
    ];
    for (ids, secs) in steps {
        let events = jobs.observe(&ids, start + Duration::from_secs(secs));
        emitted.push((secs, events));
    }

    assert_eq!(emitted[0], (0, vec![started]));
    assert_eq!(emitted[1], (2, vec![]));
    assert_eq!(emitted[2], (4, vec![]));
    assert_eq!(emitted[3], (6, vec![]));
    assert_eq!(emitted[4], (8, vec![ended]));
    assert_eq!(emitted[5], (10, vec![]));
}

#[test]
fn run_ids_look_up_each_new_pid_once_and_forget_pids_that_leave() {
    let mut tokens = std::collections::BTreeMap::new();
    let lookups = RefCell::new(Vec::new());
    let lookup = |pid: u32| {
        lookups.borrow_mut().push(pid);
        Some(format!("t{pid}"))
    };

    let first = run_ids(&mut tokens, &[20, 30], lookup);
    let second = run_ids(&mut tokens, &[20, 30], lookup);
    assert_eq!(
        first,
        BTreeSet::from(["bg-20-t20".to_string(), "bg-30-t30".to_string()])
    );
    assert_eq!(second, first);
    assert_eq!(*lookups.borrow(), vec![20, 30]);

    run_ids(&mut tokens, &[20], lookup);
    run_ids(&mut tokens, &[20, 30], lookup);
    assert_eq!(*lookups.borrow(), vec![20, 30, 30]);
}

#[test]
fn a_pid_whose_start_token_is_unreadable_is_left_out() {
    let mut tokens = std::collections::BTreeMap::new();

    let ids = run_ids(&mut tokens, &[20, 30], |pid| {
        (pid == 20).then(|| "t20".to_string())
    });

    assert_eq!(ids, BTreeSet::from(["bg-20-t20".to_string()]));
}

#[test]
fn polling_waits_for_the_turn_to_end_then_for_the_spacing() {
    let start = Instant::now();
    let mut jobs = BackgroundJobs::default();

    assert!(!jobs.poll_due(false, start), "nothing to watch yet");

    jobs.note_turn_ended();
    assert!(
        !jobs.poll_due(true, start),
        "never poll while the root turn runs"
    );
    assert!(jobs.poll_due(false, start), "poll right after a turn ends");

    jobs.mark_polled(start);
    assert!(
        !jobs.poll_due(false, start),
        "no job tracked, no further poll"
    );

    jobs.observe(&live(&["bg-20-tok"]), start);
    assert!(!jobs.poll_due(false, start + POLL_EVERY - Duration::from_millis(1)));
    assert!(jobs.poll_due(false, start + POLL_EVERY));
}
