//! Background-job regressions (issue #1412). Clocks are injected and run-id sets
//! are synthetic: nothing here spawns a process or runs `ps`.

use super::background_jobs::{run_ids, BackgroundJobs, PollPlan, END_DEBOUNCE, POLL_EVERY};
use super::process_tree::ProcessEntry;
use super::session_activity::{SessionActivity, SessionActivityReporter};
use super::SessionServerEvent;
use ajax_core::acp_execution_state::AcpExecutionEvent;
use ajax_core::models::{LiveObservation, LiveStatusKind};
use std::collections::BTreeSet;
use std::time::{Duration, Instant, SystemTime};

fn live(ids: &[&str]) -> BTreeSet<String> {
    ids.iter().map(|id| id.to_string()).collect()
}

/// A nono root (pid 1) with one zsh shell child (pid 20) started at `start`.
fn job_table(start: &str) -> Vec<ProcessEntry> {
    vec![
        ProcessEntry {
            pid: 1,
            ppid: 0,
            start: "Sat Oct 25 02:10:36 2026".to_string(),
            comm: "nono".to_string(),
        },
        ProcessEntry {
            pid: 20,
            ppid: 1,
            start: start.to_string(),
            comm: "zsh".to_string(),
        },
    ]
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
    let mut jobs = BackgroundJobs::default();
    event(&mut reporter, &prompt());
    assert_eq!(jobs.plan(Some(1), true, at), PollPlan::Skip);
    assert_eq!(
        event(&mut reporter, &turn_end()),
        SessionActivity::TurnEnded
    );
    assert_eq!(
        jobs.plan(Some(1), reporter.turn_running(), at),
        PollPlan::Scan(1)
    );
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
    assert_eq!(
        jobs.plan(Some(1), reporter.turn_running(), start + 10 * POLL_EVERY),
        PollPlan::Skip
    );
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
fn run_ids_come_straight_from_the_snapshot_pid_and_start_time() {
    let ids = run_ids(&job_table("Sat Oct 25 02:11:00 2026"), 1);

    assert_eq!(
        ids,
        BTreeSet::from(["bg-20-Sat_Oct_25_02:11:00_2026".to_string()])
    );
}

#[test]
fn run_ids_differ_for_the_same_pid_with_a_different_start() {
    let first = run_ids(&job_table("Sat Oct 25 02:11:00 2026"), 1);
    let reused = run_ids(&job_table("Sat Oct 25 03:45:12 2026"), 1);

    assert_eq!(first.len(), 1);
    assert_eq!(reused.len(), 1);
    assert_ne!(first, reused);
}

#[test]
fn run_ids_are_empty_when_the_root_has_no_shell_children() {
    let table = vec![ProcessEntry {
        pid: 1,
        ppid: 0,
        start: "Sat Oct 25 02:10:36 2026".to_string(),
        comm: "nono".to_string(),
    }];

    assert_eq!(run_ids(&table, 1), BTreeSet::new());
}

#[test]
fn polling_waits_for_the_turn_to_end_then_for_the_spacing() {
    let start = Instant::now();
    let mut jobs = BackgroundJobs::default();

    assert_eq!(jobs.plan(Some(1), false, start), PollPlan::Scan(1));
    assert_eq!(
        jobs.plan(Some(1), false, start),
        PollPlan::Skip,
        "no job tracked, no further poll"
    );
    assert_eq!(
        jobs.plan(Some(1), true, start),
        PollPlan::Skip,
        "never poll while the root turn runs"
    );
    assert_eq!(
        jobs.plan(Some(1), false, start),
        PollPlan::Scan(1),
        "poll right after the turn stops"
    );

    jobs.observe(&live(&["bg-20-tok"]), start);
    assert_eq!(
        jobs.plan(
            Some(1),
            false,
            start + POLL_EVERY - Duration::from_millis(1)
        ),
        PollPlan::Skip
    );
    assert_eq!(
        jobs.plan(Some(1), false, start + POLL_EVERY),
        PollPlan::Scan(1)
    );
}

#[test]
fn a_fresh_tracker_scans_once_before_any_turn() {
    let start = Instant::now();
    let mut jobs = BackgroundJobs::default();

    assert_eq!(jobs.plan(Some(1), false, start), PollPlan::Scan(1));
    assert_eq!(jobs.plan(Some(1), false, start), PollPlan::Skip);
}

#[test]
fn a_turn_that_stops_without_turn_end_arms_a_scan() {
    let start = Instant::now();
    let mut reporter = SessionActivityReporter::default();
    let mut jobs = BackgroundJobs::default();
    assert_eq!(jobs.plan(Some(1), false, start), PollPlan::Scan(1));

    event(&mut reporter, &prompt());
    assert_eq!(jobs.plan(Some(1), true, start), PollPlan::Skip);

    assert_eq!(
        event(
            &mut reporter,
            &SessionServerEvent::Error {
                message: "boom".to_string(),
            }
        ),
        SessionActivity::TurnFailed
    );
    assert_eq!(
        jobs.plan(Some(1), reporter.turn_running(), start),
        PollPlan::Scan(1)
    );
}

#[test]
fn losing_the_client_ends_every_tracked_job_at_once() {
    let start = Instant::now();
    let mut reporter = SessionActivityReporter::default();
    let mut jobs = BackgroundJobs::default();
    event(&mut reporter, &prompt());
    assert_eq!(jobs.plan(Some(1), true, start), PollPlan::Skip);
    event(&mut reporter, &turn_end());
    assert_eq!(
        jobs.plan(Some(1), reporter.turn_running(), start),
        PollPlan::Scan(1)
    );
    assert_eq!(
        poll(
            &mut jobs,
            &mut reporter,
            &live(&["bg-20-tok", "bg-21-tok"]),
            start
        ),
        Some(SessionActivity::BackgroundRunning)
    );

    let PollPlan::EndAll(events) = jobs.plan(None, reporter.turn_running(), start) else {
        panic!("a lost client must end the tracked jobs");
    };
    assert_eq!(
        events,
        vec![
            AcpExecutionEvent::ChildEnded {
                run_id: "bg-20-tok".to_string(),
                completion_holding: true,
            },
            AcpExecutionEvent::ChildEnded {
                run_id: "bg-21-tok".to_string(),
                completion_holding: true,
            },
        ]
    );
    assert!(!jobs.is_tracking());
    assert_eq!(
        reporter.activity_for_batch(events, SystemTime::now()),
        Some(SessionActivity::TurnEnded),
        "no debounce: the work reads as ready at once"
    );
    assert_eq!(jobs.plan(None, false, start), PollPlan::Skip);
}

#[test]
fn a_failed_turn_with_a_held_child_reads_as_failed_not_background() {
    let mut reporter = SessionActivityReporter::default();
    event(&mut reporter, &prompt());
    let started = AcpExecutionEvent::ChildStarted {
        run_id: "bg-20-tok".to_string(),
        completion_holding: true,
    };
    assert_eq!(
        reporter.activity_for_batch(vec![started], SystemTime::now()),
        None
    );

    assert_eq!(
        event(
            &mut reporter,
            &SessionServerEvent::TurnEnd {
                stop_reason: Some("error".to_string()),
            }
        ),
        SessionActivity::TurnFailed
    );
}

#[test]
fn a_settled_turn_with_a_held_child_reads_as_background_work() {
    let mut reporter = SessionActivityReporter::default();
    event(&mut reporter, &prompt());
    let started = AcpExecutionEvent::ChildStarted {
        run_id: "bg-20-tok".to_string(),
        completion_holding: true,
    };
    assert_eq!(
        reporter.activity_for_batch(vec![started], SystemTime::now()),
        None
    );

    assert_eq!(
        event(&mut reporter, &turn_end()),
        SessionActivity::BackgroundRunning
    );
}

#[test]
fn a_cancelled_turn_with_a_held_child_reads_as_background_work() {
    let mut reporter = SessionActivityReporter::default();
    event(&mut reporter, &prompt());
    let started = AcpExecutionEvent::ChildStarted {
        run_id: "bg-20-tok".to_string(),
        completion_holding: true,
    };
    assert_eq!(
        reporter.activity_for_batch(vec![started], SystemTime::now()),
        None
    );

    assert_eq!(
        event(
            &mut reporter,
            &SessionServerEvent::TurnEnd {
                stop_reason: Some("cancelled".to_string()),
            }
        ),
        SessionActivity::BackgroundRunning
    );
}
