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
    fn core_manifest_declares_no_web_dependencies() {
        let manifest = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml"),
        )
        .unwrap();

        for forbidden in ["axum", "rcgen", "rustls", "web-push", "ajax-web"] {
            assert!(
                !manifest.lines().any(|line| line
                    .trim_start()
                    .starts_with(&format!("{forbidden} "))
                    || line.trim_start().starts_with(&format!("{forbidden}="))),
                "ajax-core must stay browser-agnostic; found dependency on {forbidden}"
            );
        }
    }
}
