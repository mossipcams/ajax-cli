//! Background-level regressions (issue #1412). Clocks are injected and reads are
//! fake closures: nothing here spawns a process or runs `ps`.

use super::background_jobs::{
    probe, BackgroundLevel, Step, END_DEBOUNCE, IDLE_POLL_EVERY, POLL_EVERY, POLL_MAX_AGE, SCAN_TTL,
};
use super::session_activity::{SessionActivity, SessionActivityReporter};
use super::SessionServerEvent;
use ajax_core::models::{LiveObservation, LiveStatusKind};
use std::cell::{Cell, RefCell};
use std::time::{Duration, Instant, SystemTime};

fn prompt() -> SessionServerEvent {
    SessionServerEvent::PromptAccepted {
        client_message_id: "c1".to_string(),
    }
}

fn turn_end() -> SessionServerEvent {
    turn_end_with("end_turn")
}

fn turn_end_with(stop_reason: &str) -> SessionServerEvent {
    SessionServerEvent::TurnEnd {
        stop_reason: Some(stop_reason.to_string()),
    }
}

/// A probe that must never run.
fn no_read() -> Option<bool> {
    panic!("the background level must not be read here")
}

/// Apply one root event as the actor does, committing whatever is reported.
fn apply(
    reporter: &mut SessionActivityReporter,
    event: &SessionServerEvent,
    probe: &mut dyn FnMut() -> Option<bool>,
) -> Option<SessionActivity> {
    let activity = reporter.activity_for_event_with_probe(event, probe)?;
    reporter.commit(activity);
    Some(activity)
}

/// The turn-end read as the actor takes it: the level result is recorded as it is returned.
fn read_at_turn_end(level: &mut BackgroundLevel, read: Option<bool>, now: Instant) -> Option<bool> {
    level.record_turn_end(read, now);
    read
}

/// One tick as `poll_background_jobs` runs it, with the `ps` read injected.
fn tick(
    level: &mut BackgroundLevel,
    reporter: &mut SessionActivityReporter,
    root: Option<u32>,
    read: impl FnOnce() -> Option<bool>,
    now: Instant,
) -> Option<SessionActivity> {
    let turn_finished = reporter.turn_finished();
    let held = reporter.background_held();
    let activity = match level.step(root, turn_finished, held, now) {
        Step::Skip => return None,
        Step::Clear => reporter.set_background(false, SystemTime::now()),
        Step::Poll(_) => {
            let next = level.record_poll(read(), held, now);
            if next == held {
                return None;
            }
            reporter.set_background(next, SystemTime::now())
        }
    }?;
    reporter.commit(activity);
    Some(activity)
}

/// A finished turn whose turn-end read found a live background shell.
fn turn_ended_with_shell(
    level: &mut BackgroundLevel,
    reporter: &mut SessionActivityReporter,
    at: Instant,
) {
    apply(reporter, &prompt(), &mut || Some(false));
    let activity = apply(reporter, &turn_end(), &mut || {
        read_at_turn_end(level, Some(true), at)
    });
    assert_eq!(activity, Some(SessionActivity::BackgroundRunning));
}

#[test]
fn a_session_that_never_ran_a_turn_never_reports_background_or_polls() {
    let start = Instant::now();
    let mut reporter = SessionActivityReporter::default();
    let mut level = BackgroundLevel::default();

    for step in 0..10 {
        let at = start + POLL_EVERY * step;
        assert_eq!(
            tick(&mut level, &mut reporter, Some(1), no_read, at),
            None,
            "an idle root has no turn to hold"
        );
    }
    assert_eq!(
        reporter.set_background(true, SystemTime::now()),
        None,
        "an idle root never carries the level"
    );
    assert!(!reporter.background_held());
}

#[test]
fn turn_end_with_a_live_shell_reports_background_as_the_first_activity() {
    let mut reporter = SessionActivityReporter::default();
    let mut level = BackgroundLevel::default();
    apply(&mut reporter, &prompt(), &mut || Some(false));

    let mut reported = Vec::new();
    let mut probes = 0;
    let at = Instant::now();
    reported.extend(apply(&mut reporter, &turn_end(), &mut || {
        probes += 1;
        read_at_turn_end(&mut level, Some(true), at)
    }));

    assert_eq!(reported.first(), Some(&SessionActivity::BackgroundRunning));
    assert_eq!(probes, 1);
    assert_eq!(
        SessionActivity::BackgroundRunning.observation(),
        LiveObservation::new(LiveStatusKind::AgentRunning, "Background work running")
    );
    assert!(reporter.background_held());
}

#[test]
fn a_cancelled_turn_reads_the_level_and_a_failed_turn_never_does() {
    let mut reporter = SessionActivityReporter::default();
    apply(&mut reporter, &prompt(), &mut || Some(false));
    let probes = Cell::new(0);
    let cancelled = apply(&mut reporter, &turn_end_with("cancelled"), &mut || {
        probes.set(probes.get() + 1);
        Some(false)
    });
    assert_eq!(cancelled, Some(SessionActivity::TurnEnded));
    assert_eq!(probes.get(), 1, "a cancelled turn is a finished turn");

    let mut reporter = SessionActivityReporter::default();
    apply(&mut reporter, &prompt(), &mut || Some(false));
    let failed = apply(&mut reporter, &turn_end_with("error"), &mut no_read);
    assert_eq!(failed, Some(SessionActivity::TurnFailed));
}

#[test]
fn prompt_and_turn_end_applied_back_to_back_read_the_level_once() {
    let mut reporter = SessionActivityReporter::default();
    let probes = Cell::new(0);
    let mut probe = || {
        probes.set(probes.get() + 1);
        Some(true)
    };

    assert_eq!(
        apply(&mut reporter, &prompt(), &mut probe),
        Some(SessionActivity::TurnStarted)
    );
    assert_eq!(
        apply(&mut reporter, &turn_end(), &mut probe),
        Some(SessionActivity::BackgroundRunning)
    );
    assert_eq!(probes.get(), 1);
}

#[test]
fn a_failed_turn_reports_failed_even_with_a_held_level() {
    let mut reporter = SessionActivityReporter::default();
    let mut level = BackgroundLevel::default();
    let at = Instant::now();
    turn_ended_with_shell(&mut level, &mut reporter, at);

    assert_eq!(
        apply(&mut reporter, &turn_end_with("error"), &mut no_read),
        Some(SessionActivity::TurnFailed)
    );
}

#[test]
fn a_settled_root_polls_an_unheld_level_and_reports_a_shell_started_after_the_turn() {
    let start = Instant::now();
    let mut reporter = SessionActivityReporter::default();
    let mut level = BackgroundLevel::default();
    apply(&mut reporter, &prompt(), &mut || Some(false));
    let ended = apply(&mut reporter, &turn_end(), &mut || {
        read_at_turn_end(&mut level, Some(false), start)
    });
    assert_eq!(ended, Some(SessionActivity::TurnEnded));

    // An SDK-initiated turn starts a shell after the level ended: the next idle poll sees it.
    assert_eq!(
        tick(
            &mut level,
            &mut reporter,
            Some(1),
            || Some(false),
            start + Duration::from_millis(1)
        ),
        None
    );
    assert_eq!(
        tick(
            &mut level,
            &mut reporter,
            Some(1),
            || Some(true),
            // The first idle poll ran at 1 ms, so the next is due one spacing after it.
            start + Duration::from_millis(1) + IDLE_POLL_EVERY
        ),
        Some(SessionActivity::BackgroundRunning)
    );
    assert!(reporter.background_held());
}

#[test]
fn polls_are_spaced_by_the_idle_cadence_when_unheld_and_the_poll_cadence_when_held() {
    let start = Instant::now();
    let step = Duration::from_millis(100);

    let mut reporter = SessionActivityReporter::default();
    let mut level = BackgroundLevel::default();
    apply(&mut reporter, &prompt(), &mut || Some(false));
    apply(&mut reporter, &turn_end(), &mut || {
        read_at_turn_end(&mut level, Some(false), start)
    });
    let unheld = RefCell::new(Vec::new());
    for ticks in 1..=120u32 {
        let at = start + step * ticks;
        tick(
            &mut level,
            &mut reporter,
            Some(1),
            || {
                unheld.borrow_mut().push(at);
                Some(false)
            },
            at,
        );
    }
    let unheld = unheld.into_inner();
    assert!(
        unheld.len() >= 2,
        "an unheld settled root polls: {unheld:?}"
    );
    assert!(
        unheld
            .windows(2)
            .all(|pair| pair[1] - pair[0] == IDLE_POLL_EVERY),
        "unheld polls are IDLE_POLL_EVERY apart: {unheld:?}"
    );

    let mut reporter = SessionActivityReporter::default();
    let mut level = BackgroundLevel::default();
    turn_ended_with_shell(&mut level, &mut reporter, start);
    let held = RefCell::new(Vec::new());
    for ticks in 1..=120u32 {
        let at = start + step * ticks;
        tick(
            &mut level,
            &mut reporter,
            Some(1),
            || {
                held.borrow_mut().push(at);
                Some(true)
            },
            at,
        );
    }
    let held = held.into_inner();
    assert!(held.len() >= 2, "a held level polls: {held:?}");
    assert!(
        held.windows(2).all(|pair| pair[1] - pair[0] == POLL_EVERY),
        "held polls are POLL_EVERY apart: {held:?}"
    );
}

#[test]
fn an_unavailable_turn_end_read_keeps_a_held_level_held() {
    let start = Instant::now();
    let mut reporter = SessionActivityReporter::default();
    let mut level = BackgroundLevel::default();
    turn_ended_with_shell(&mut level, &mut reporter, start);

    let probes = Cell::new(0);
    let repeated = apply(&mut reporter, &turn_end(), &mut || {
        probes.set(probes.get() + 1);
        read_at_turn_end(&mut level, None, start + POLL_EVERY)
    });
    assert_eq!(probes.get(), 1);
    assert_eq!(
        repeated, None,
        "no evidence: the held level is not re-reported"
    );
    assert!(
        reporter.background_held(),
        "None must not clear a held level"
    );
}

#[test]
fn an_unavailable_turn_end_read_keeps_an_unheld_level_unheld() {
    let start = Instant::now();
    let mut reporter = SessionActivityReporter::default();
    let mut level = BackgroundLevel::default();
    apply(&mut reporter, &prompt(), &mut || Some(false));

    let ended = apply(&mut reporter, &turn_end(), &mut || {
        read_at_turn_end(&mut level, None, start)
    });
    assert_eq!(ended, Some(SessionActivity::TurnEnded));
    assert!(!reporter.background_held());
}

#[test]
fn held_level_ends_when_scans_fail_for_the_scan_ttl() {
    let start = Instant::now();
    let mut reporter = SessionActivityReporter::default();
    let mut level = BackgroundLevel::default();
    turn_ended_with_shell(&mut level, &mut reporter, start);

    let mut reported = Vec::new();
    for step in 1..=SCAN_TTL.as_secs() / POLL_EVERY.as_secs() {
        let at = start + POLL_EVERY * u32::try_from(step).unwrap();
        reported.push(tick(&mut level, &mut reporter, Some(1), || None, at));
    }

    let last = reported.pop().expect("polls ran");
    assert!(reported.iter().all(Option::is_none), "{reported:?}");
    assert_eq!(last, Some(SessionActivity::TurnEnded));
    assert!(!reporter.background_held());
}

#[test]
fn losing_the_client_ends_the_held_level_at_once() {
    let start = Instant::now();
    let mut reporter = SessionActivityReporter::default();
    let mut level = BackgroundLevel::default();
    turn_ended_with_shell(&mut level, &mut reporter, start);

    assert_eq!(
        tick(
            &mut level,
            &mut reporter,
            None,
            no_read,
            start + Duration::from_millis(1)
        ),
        Some(SessionActivity::TurnEnded),
        "no debounce: the work reads as ready at once"
    );
}

#[test]
fn a_shell_missing_for_less_than_the_debounce_keeps_the_level() {
    let start = Instant::now();
    let mut reporter = SessionActivityReporter::default();
    let mut level = BackgroundLevel::default();
    turn_ended_with_shell(&mut level, &mut reporter, start);

    for secs in [2, 4] {
        assert_eq!(
            tick(
                &mut level,
                &mut reporter,
                Some(1),
                || Some(false),
                start + Duration::from_secs(secs)
            ),
            None,
            "{secs}s unseen is inside the debounce"
        );
    }
    assert_eq!(
        tick(
            &mut level,
            &mut reporter,
            Some(1),
            || Some(false),
            start + END_DEBOUNCE + Duration::from_secs(1)
        ),
        Some(SessionActivity::TurnEnded),
        "past the debounce the level clears"
    );
}

#[test]
fn a_new_turn_clears_the_level_before_the_next_turn_ends() {
    let start = Instant::now();
    let mut reporter = SessionActivityReporter::default();
    let mut level = BackgroundLevel::default();
    turn_ended_with_shell(&mut level, &mut reporter, start);

    let started = apply(&mut reporter, &prompt(), &mut no_read);
    assert_eq!(started, Some(SessionActivity::TurnStarted));
    assert!(!reporter.background_held());

    let ended = apply(&mut reporter, &turn_end(), &mut || {
        read_at_turn_end(&mut level, Some(false), start + POLL_EVERY)
    });
    assert_eq!(
        ended,
        Some(SessionActivity::TurnEnded),
        "the follow-up turn ends Response ready, never Background work running in between"
    );
}

#[test]
fn a_turn_end_without_a_prompt_read_reads_the_level_and_reports_background() {
    let mut reporter = SessionActivityReporter::default();
    let mut level = BackgroundLevel::default();
    let at = Instant::now();
    let probes = Cell::new(0);

    let mut reported = Vec::new();
    reported.extend(apply(&mut reporter, &turn_end(), &mut || {
        probes.set(probes.get() + 1);
        read_at_turn_end(&mut level, Some(true), at)
    }));

    assert_eq!(
        probes.get(),
        1,
        "the turn end is the read, with no turn start seen"
    );
    assert_eq!(reported.first(), Some(&SessionActivity::BackgroundRunning));
    assert!(reporter.background_held());
}

#[test]
fn a_second_turn_end_without_a_prompt_clears_the_stale_level() {
    let start = Instant::now();
    let mut reporter = SessionActivityReporter::default();
    let mut level = BackgroundLevel::default();
    turn_ended_with_shell(&mut level, &mut reporter, start);

    let probes = Cell::new(0);
    let ended = apply(&mut reporter, &turn_end(), &mut || {
        probes.set(probes.get() + 1);
        read_at_turn_end(&mut level, Some(false), start + POLL_EVERY)
    });

    assert_eq!(probes.get(), 1, "a repeated turn end reads the level again");
    assert_eq!(ended, Some(SessionActivity::TurnEnded));
    assert!(!reporter.background_held());
}

#[test]
fn a_failed_level_read_never_reports_a_background_level_for_a_missing_root() {
    assert_eq!(probe(None, POLL_MAX_AGE), Some(false));
}
