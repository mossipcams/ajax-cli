#![deny(unsafe_op_in_unsafe_fn)]

pub mod adapters;
pub mod analysis;
pub mod attention;
pub mod commands;
pub mod config;
pub mod events;
pub mod lifecycle;
pub mod live;
mod live_application;
pub mod models;
pub mod operation;
pub mod output;
pub mod policy;
pub mod registry;
pub mod ui_state;

#[cfg(test)]
mod tests {
    #[test]
    fn crate_root_does_not_keep_package_identity_wrapper() {
        let lib = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/lib.rs"),
        )
        .unwrap();
        let wrapper_name = ["package", "_name"].concat();

        assert!(!lib.contains(&wrapper_name));
    }

    #[test]
    fn avoids_duplicate_cockpit_snapshot_contract() {
        let lib = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/lib.rs"),
        )
        .unwrap();

        let duplicate_contract_export = ["pub mod ", "cockpit", ";"].concat();
        assert!(!lib.contains(&duplicate_contract_export));
    }

    #[test]
    fn crate_root_does_not_export_reconcile_module() {
        let lib = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/lib.rs"),
        )
        .unwrap();

        let reconcile_export = ["pub mod ", "reconcile", ";"].concat();
        assert!(!lib.contains(&reconcile_export));
    }

    #[test]
    fn command_doctor_checks_live_in_focused_module() {
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let commands = std::fs::read_to_string(manifest_dir.join("src/commands.rs")).unwrap();
        let doctor_module =
            std::fs::read_to_string(manifest_dir.join("src/commands/doctor.rs")).unwrap();
        let adapters = std::fs::read_to_string(manifest_dir.join("src/adapters.rs")).unwrap();
        let environment_adapter =
            std::fs::read_to_string(manifest_dir.join("src/adapters/environment.rs")).unwrap();

        assert!(commands.contains("mod doctor;"));
        assert!(!commands.contains("pub struct DoctorEnvironment"));
        assert!(adapters.contains("pub mod environment;"));
        assert!(!doctor_module.contains("std::env"));
        assert!(!doctor_module.contains("path.exists()"));
        assert!(doctor_module.contains("pub fn doctor_with_environment"));
        assert!(environment_adapter.contains("pub struct DoctorEnvironment"));
        assert!(environment_adapter.contains("std::env"));
        assert!(environment_adapter.contains("path.exists()"));
    }

    #[test]
    fn command_task_projection_lives_in_focused_module() {
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let commands = std::fs::read_to_string(manifest_dir.join("src/commands.rs")).unwrap();
        let projection_module =
            std::fs::read_to_string(manifest_dir.join("src/commands/projection.rs")).unwrap();

        assert!(commands.contains("mod projection;"));
        assert!(!commands.contains("fn task_summary("));
        assert!(!commands.contains("fn cockpit_summary("));
        assert!(projection_module.contains("pub(super) fn task_summary("));
        assert!(projection_module.contains("pub(super) fn cockpit_summary("));
    }

    #[test]
    fn command_task_lookup_lives_in_focused_module() {
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let commands = std::fs::read_to_string(manifest_dir.join("src/commands.rs")).unwrap();
        let lookup_module =
            std::fs::read_to_string(manifest_dir.join("src/commands/lookup.rs")).unwrap();

        assert!(commands.contains("mod lookup;"));
        assert!(!commands.contains("fn find_task<"));
        assert!(!commands.contains("fn task_repo_path<"));
        assert!(lookup_module.contains("pub(super) fn find_task<"));
        assert!(lookup_module.contains("pub(super) fn task_repo_path<"));
    }
}
