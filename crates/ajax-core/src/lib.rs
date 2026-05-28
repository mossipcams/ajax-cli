#![deny(unsafe_op_in_unsafe_fn)]

pub mod adapters;
pub mod agent_prompt;
pub mod analysis;
pub mod attention;
pub mod commands;
pub mod config;
pub mod events;
pub mod ghost_task;
pub mod lifecycle;
pub mod live;
mod live_application;
pub mod models;
pub mod operation;
pub mod output;
pub mod policy;
pub mod recommended;
pub mod registry;
pub mod remediation;
pub mod runtime;
pub mod runtime_refresh;
pub mod slices;
pub mod task_operations;
pub mod ui_state;
pub mod use_cases;
pub mod validity;

#[cfg(test)]
mod architecture;

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

    #[test]
    fn use_case_contracts_are_not_owned_by_command_facade() {
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let lib = std::fs::read_to_string(manifest_dir.join("src/lib.rs")).unwrap();
        let commands = std::fs::read_to_string(manifest_dir.join("src/commands.rs")).unwrap();
        let use_cases = std::fs::read_to_string(manifest_dir.join("src/use_cases.rs")).unwrap();

        assert!(lib.contains("pub mod use_cases;"));
        assert!(!commands.contains("pub struct CommandContext"));
        assert!(!commands.contains("pub enum CommandError"));
        assert!(!commands.contains("pub struct CommandPlan"));
        assert!(!commands.contains("pub enum OpenMode"));
        assert!(use_cases.contains("pub struct CommandContext"));
        assert!(use_cases.contains("pub enum CommandError"));
        assert!(use_cases.contains("pub struct CommandPlan"));
        assert!(use_cases.contains("pub enum OpenMode"));
    }

    #[test]
    fn architecture_rules_can_use_rust_arkitect() {
        let _project = rust_arkitect::dsl::project::Project::from_current_crate();
    }

    #[test]
    fn architecture_rules_are_executable() {
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let lib = std::fs::read_to_string(manifest_dir.join("src/lib.rs")).unwrap();
        let architecture =
            std::fs::read_to_string(manifest_dir.join("src/architecture.rs")).unwrap();

        assert!(lib.contains("mod architecture;"));
        assert!(architecture.contains("rust_arkitect::dsl"));
        assert!(architecture.contains("complies_with"));
        assert!(architecture.contains("crate::slices"));
    }

    #[test]
    fn command_review_compatibility_paths_delegate_to_review_slice() {
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let commands = std::fs::read_to_string(manifest_dir.join("src/commands.rs")).unwrap();
        let diff_module =
            std::fs::read_to_string(manifest_dir.join("src/commands/diff.rs")).unwrap();

        assert!(commands.contains("crate::slices::review::review_queue(context)"));
        assert!(diff_module.contains("crate::slices::review::review_task_plan"));
    }

    #[test]
    fn core_remains_browser_agnostic() {
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let sources = [
            "src/adapters.rs",
            "src/analysis.rs",
            "src/commands.rs",
            "src/config.rs",
            "src/lib.rs",
            "src/models.rs",
            "src/output.rs",
            "src/runtime.rs",
            "src/runtime_refresh.rs",
            "src/task_operations.rs",
            "src/use_cases.rs",
        ]
        .into_iter()
        .map(|relative| std::fs::read_to_string(manifest_dir.join(relative)).unwrap())
        .collect::<Vec<_>>()
        .join("\n");

        for forbidden in [
            ["service", " worker"].concat(),
            ["web", " push"].concat(),
            ["rust", "ls"].concat(),
            ["http", " route"].concat(),
            ["manifest", ".webmanifest"].concat(),
            ["ajax-web", "::runtime"].concat(),
        ] {
            assert!(
                !sources.contains(&forbidden),
                "ajax-core must not own browser/PWA mechanism: {forbidden}"
            );
        }
    }
}
