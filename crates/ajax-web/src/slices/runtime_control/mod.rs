//! Runtime control panel: durable lifecycle operations for the web control plane.

mod logs;
mod operation;
mod status;
mod store;

pub use operation::{handle_server_restart, handle_server_update, schedule_runtime_update};
pub use status::{runtime_status_json, RuntimeStatusInput};

#[cfg(test)]
mod store_tests {
    use super::store::{
        operation_is_active, queue_operation, read_state, OperationKind, OperationPhase,
        OperationRecord,
    };

    #[test]
    fn queue_operation_writes_durable_state() {
        let dir = std::env::temp_dir().join(format!("ajax-runtime-store-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        queue_operation(&dir, OperationKind::Restart).expect("queue");
        let state = read_state(&dir);
        assert_eq!(
            state.operation.as_ref().map(|op| op.kind),
            Some(OperationKind::Restart)
        );
        assert_eq!(
            state.operation.as_ref().map(|op| op.phase),
            Some(OperationPhase::Queued)
        );
        assert!(operation_is_active(&state));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn terminal_results_are_not_active() {
        let state = super::store::RuntimeControlState {
            commit: None,
            operation: Some(OperationRecord {
                kind: OperationKind::Update,
                phase: OperationPhase::RolledBack,
                started_at: None,
                finished_at: None,
                result: None,
                rollback: true,
            }),
            updated_at: None,
        };
        assert!(!operation_is_active(&state));
    }
}
