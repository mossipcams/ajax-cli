//! Wave 0: cross-layer run-state characterization (no behavior changes).
//!
//! Records disagreements between `reduce_agent_status`, materialized Task fields
//! (the apply layer), and `derive_operator_status`. Wave 1+ may converge these;
//! this file must not weaken assertions to make them pass.

use std::time::{Duration, UNIX_EPOCH};

use ajax_core::{
    agent_status::{
        reduce_agent_status, ActivityKind, Confidence, ObservationSource, ParentPhase,
        ProcessLiveness, ReduceInput, StatusObservation,
    },
    commands::{
        mark_new_task_provisioning_step_completed, record_new_task, CommandContext, NewTaskRequest,
        StartProvisioningStep,
    },
    config::{Config, ManagedRepo},
    live::apply_authoritative_observation_at,
    models::{
        AgentClient, AgentRuntimeStatus, LifecycleStatus, LiveObservation, LiveStatusKind,
        SideFlag, Task, TaskId,
    },
    registry::{InMemoryRegistry, Registry},
    ui_state::{derive_operator_status, TaskStatus},
};

const PRIMARY_RUN: &str = "primary";

#[derive(Debug)]
struct GoldenRow {
    name: &'static str,
    stored_agent_status: AgentRuntimeStatus,
    stored_live_kind: Option<LiveStatusKind>,
    open_attempt_running: bool,
    reducer_live_kind: LiveStatusKind,
    reducer_phase: ParentPhase,
    operator_status: TaskStatus,
    /// Human-readable contradictions that exist today and must not be papered over.
    disagreements: Vec<&'static str>,
}

fn reduce_with(
    observations: &[StatusObservation],
    process_alive: bool,
) -> ajax_core::agent_status::StatusProjection {
    let now = UNIX_EPOCH + Duration::from_secs(10_000);
    reduce_agent_status(ReduceInput {
        now,
        primary_run_id: PRIMARY_RUN.to_string(),
        process_liveness: Some(ProcessLiveness {
            alive: process_alive,
            observed_at: now,
        }),
        observations,
    })
}

fn observation(source: ObservationSource, kind: ActivityKind, run_id: &str) -> StatusObservation {
    let observed_at = UNIX_EPOCH + Duration::from_secs(9_900);
    StatusObservation {
        source,
        observed_at,
        expires_at: observed_at + Duration::from_secs(300),
        confidence: Confidence::High,
        run_id: run_id.to_string(),
        parent_run_id: None,
        kind,
    }
}

fn active_codex_task() -> Task {
    let mut task = Task::new(
        TaskId::new("web/fix-login"),
        "web",
        "fix-login",
        "Fix login",
        "ajax/fix-login",
        "main",
        "/tmp/worktrees/web-fix-login",
        "ajax-web-fix-login",
        "task",
        AgentClient::Codex,
    );
    task.lifecycle_status = LifecycleStatus::Active;
    task
}

/// Provisioned ACP task after `AgentCommandSent` with no `TurnStarted` / host
/// observation — models GitHub #1096 (spawn/auth never starts a turn).
pub fn issue_1096_provisioned_acp_after_agent_command_sent() -> Task {
    let mut context = CommandContext::new(
        Config {
            repos: vec![ManagedRepo::new("web", "/repo/web", "main")],
            ..Config::default()
        },
        InMemoryRegistry::default(),
    );
    let request = NewTaskRequest {
        repo: "web".to_string(),
        title: "Fix login".to_string(),
        agent: "cursor".to_string(),
        skip_interactive_agent: true,
        model: None,
    };
    let task = record_new_task(&mut context, &request).unwrap();
    let task_id = task.id.clone();
    mark_new_task_provisioning_step_completed(
        &mut context,
        &task_id,
        StartProvisioningStep::AgentCommandSent,
    )
    .unwrap();
    context.registry.get_task(&task_id).unwrap().clone()
}

fn row_issue_1096() -> GoldenRow {
    let task = issue_1096_provisioned_acp_after_agent_command_sent();
    let projection = reduce_with(&[], false);
    let operator = derive_operator_status(&task);

    let open_attempt_running = task.agent_attempts.iter().any(|attempt| {
        attempt.status == AgentRuntimeStatus::Running && attempt.finished_at.is_none()
    });

    GoldenRow {
        name: "issue_1096_provisioned_acp_agent_command_sent_no_turn",
        stored_agent_status: task.agent_status,
        stored_live_kind: task.live_status.as_ref().map(|live| live.kind),
        open_attempt_running,
        reducer_live_kind: projection.live.kind,
        reducer_phase: projection.phase,
        operator_status: operator.status,
        disagreements: vec![
            "operator Unknown while launch episode closed (NotStarted) — attempt history no longer reads in-progress (#1096 fixed Wave 1)",
        ],
    }
}

fn row_acknowledged_waiting() -> GoldenRow {
    let mut task = active_codex_task();
    let observed_at = UNIX_EPOCH + Duration::from_secs(400);
    apply_authoritative_observation_at(
        &mut task,
        LiveObservation::new(LiveStatusKind::WaitingForInput, "waiting for input"),
        observed_at,
    );
    ajax_core::live::acknowledge_attention(&mut task, UNIX_EPOCH + Duration::from_secs(500));

    let obs = [observation(
        ObservationSource::ProviderLifecycle,
        ActivityKind::WaitingInput,
        PRIMARY_RUN,
    )];
    let projection = reduce_with(&obs, true);
    let operator = derive_operator_status(&task);

    GoldenRow {
        name: "acknowledged_waiting_live_preserved",
        stored_agent_status: task.agent_status,
        stored_live_kind: task.live_status.as_ref().map(|live| live.kind),
        open_attempt_running: false,
        reducer_live_kind: projection.live.kind,
        reducer_phase: projection.phase,
        operator_status: operator.status,
        disagreements: vec![
            "agent_status Waiting but derive_operator_status reports Idle after acknowledgment",
            "live_status still WaitingForInput while operator band is Idle",
        ],
    }
}

fn row_acp_turn_ended() -> GoldenRow {
    let mut task = active_codex_task();
    task.set_skip_interactive_agent(true);
    task.agent_attempts
        .push(ajax_core::models::AgentAttempt::new(
            task.selected_agent,
            task.worktree_path.display().to_string(),
        ));
    apply_authoritative_observation_at(
        &mut task,
        LiveObservation::new(LiveStatusKind::AgentRunning, "Agent working"),
        UNIX_EPOCH + Duration::from_secs(100),
    );
    apply_authoritative_observation_at(
        &mut task,
        LiveObservation::new(LiveStatusKind::Done, "Response ready"),
        UNIX_EPOCH + Duration::from_secs(200),
    );

    let obs = [observation(
        ObservationSource::ProviderLifecycle,
        ActivityKind::Done,
        PRIMARY_RUN,
    )];
    let projection = reduce_with(&obs, true);
    let operator = derive_operator_status(&task);

    GoldenRow {
        name: "acp_turn_ended_done_between_turns",
        stored_agent_status: task.agent_status,
        stored_live_kind: task.live_status.as_ref().map(|live| live.kind),
        open_attempt_running: task.agent_attempts.iter().any(|attempt| {
            attempt.status == AgentRuntimeStatus::Running && attempt.finished_at.is_none()
        }),
        reducer_live_kind: projection.live.kind,
        reducer_phase: projection.phase,
        operator_status: operator.status,
        disagreements: vec![
            "agent_status Done and live Done but operator projects Waiting (Response ready)",
            "reducer FullyCompleted/Done vs open launch-episode attempt (Wave 1 intentionally keeps attempt open on TurnEnded)",
        ],
    }
}

fn snapshot_row(row: &GoldenRow) -> String {
    format!(
        "{name}: stored_agent={stored_agent:?} stored_live={stored_live:?} \
         open_attempt_running={open_attempt_running} reducer_live={reducer_live:?} \
         reducer_phase={reducer_phase:?} operator={operator:?} disagreements={disagreements:?}",
        name = row.name,
        stored_agent = row.stored_agent_status,
        stored_live = row.stored_live_kind,
        open_attempt_running = row.open_attempt_running,
        reducer_live = row.reducer_live_kind,
        reducer_phase = row.reducer_phase,
        operator = row.operator_status,
        disagreements = row.disagreements,
    )
}

#[test]
fn golden_table_records_cross_layer_disagreements_without_fixing_them() {
    let rows = [
        row_issue_1096(),
        row_acknowledged_waiting(),
        row_acp_turn_ended(),
    ];
    for row in &rows {
        eprintln!("WAVE0_GOLDEN: {}", snapshot_row(row));
        assert!(
            !row.disagreements.is_empty(),
            "{} must document at least one known disagreement",
            row.name
        );
    }

    let issue_1096 = &rows[0];
    assert_eq!(
        issue_1096.stored_agent_status,
        AgentRuntimeStatus::NotStarted
    );
    assert!(
        !issue_1096.open_attempt_running,
        "#1096: launch episode must close when spawn/auth never starts a turn"
    );
    assert_eq!(issue_1096.operator_status, TaskStatus::Unknown);
    assert_eq!(issue_1096.reducer_phase, ParentPhase::Unknown);

    let acknowledged = &rows[1];
    assert_eq!(
        acknowledged.stored_agent_status,
        AgentRuntimeStatus::Waiting
    );
    assert_eq!(acknowledged.operator_status, TaskStatus::Idle);
    assert_eq!(
        acknowledged.stored_live_kind,
        Some(LiveStatusKind::WaitingForInput)
    );

    let turn_ended = &rows[2];
    assert_eq!(turn_ended.stored_agent_status, AgentRuntimeStatus::Done);
    assert_eq!(turn_ended.operator_status, TaskStatus::Waiting);
    assert_eq!(turn_ended.reducer_phase, ParentPhase::FullyCompleted);
}

#[test]
fn issue_1096_fixture_matches_fixed_launch_episode_contract() {
    let task = issue_1096_provisioned_acp_after_agent_command_sent();
    assert_eq!(task.lifecycle_status, LifecycleStatus::Active);
    assert_eq!(task.agent_status, AgentRuntimeStatus::NotStarted);
    assert!(
        !task.has_side_flag(SideFlag::AgentRunning),
        "provisioned ACP must not set AgentRunning before the first turn (#1069)"
    );
    assert_eq!(task.live_status, None);
    assert_eq!(task.agent_attempts.len(), 1);
    assert_ne!(
        task.agent_attempts[0].status,
        AgentRuntimeStatus::Running,
        "launch episode closes when spawn/auth never starts (#1096)"
    );
    assert!(
        task.agent_attempts[0].finished_at.is_some(),
        "finished_at set when launch ends without a turn (#1096)"
    );
}
