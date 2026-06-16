#[cfg(test)]
mod tests {
    use rust_arkitect::dsl::{
        architectural_rules::ArchitecturalRules, arkitect::Arkitect, project::Project,
    };
    use rust_arkitect::{
        rule::Rule, rules::must_not_depend_on::MustNotDependOnRule, rust_file::RustFile,
    };

    const SLICES: [&str; 3] = ["pane", "review", "remediate"];

    const SUBSTRATE_MECHANISMS: [&str; 4] = ["adapters", "registry", "analysis", "runtime"];

    #[test]
    fn each_substrate_mechanism_does_not_depend_on_any_slice() {
        for mechanism in SUBSTRATE_MECHANISMS {
            let project = Project::from_current_crate();
            let forbidden = forbidden_paths_for_slices(&SLICES);
            let forbidden_refs = forbidden.iter().map(String::as_str).collect::<Vec<_>>();
            let module = format!("ajax-core::{mechanism}");

            let rules = ArchitecturalRules::define()
                .rules_for_module(module.as_str())
                .it_must_not_depend_on(&forbidden_refs)
                .build();

            let result = Arkitect::ensure_that(project).complies_with(rules);

            assert!(
                result.is_ok(),
                "architecture violations in mechanism `{mechanism}`: {:#?}",
                result.err().unwrap_or_default()
            );
        }
    }

    #[test]
    fn each_slice_is_isolated_from_sibling_slices() {
        for slice in SLICES {
            let project = Project::from_current_crate();
            let forbidden = forbidden_paths_for_sibling_slices(slice);
            if forbidden.is_empty() {
                continue;
            }
            let forbidden_refs = forbidden.iter().map(String::as_str).collect::<Vec<_>>();
            let module = format!("ajax-core::slices::{slice}");

            let rules = ArchitecturalRules::define()
                .rules_for_module(module.as_str())
                .it_must_not_depend_on(&forbidden_refs)
                .build();

            let result = Arkitect::ensure_that(project).complies_with(rules);

            assert!(
                result.is_ok(),
                "architecture violations in slice `{slice}`: {:#?}",
                result.err().unwrap_or_default()
            );
        }
    }

    #[test]
    fn architecture_rule_rejects_use_crate_slices_dependency() {
        let file = RustFile::from_content(
            "src/adapters/example.rs",
            "ajax-core::adapters::example",
            "use crate::slices::review;",
        );
        let rule = MustNotDependOnRule::new(
            "ajax-core::adapters".to_string(),
            forbidden_paths_for_slices(&["review"]),
        );

        assert!(
            rule.apply(&file).is_err(),
            "mechanism modules must not be allowed to import specific slice modules"
        );
    }

    #[test]
    fn architecture_rule_rejects_direct_crate_slices_dependency() {
        let file = RustFile::from_content(
            "src/adapters/example.rs",
            "ajax-core::adapters::example",
            "fn example() { crate::slices::review::review_task_plan(); }",
        );
        let rule = MustNotDependOnRule::new(
            "ajax-core::adapters".to_string(),
            forbidden_paths_for_slices(&["review"]),
        );

        assert!(
            rule.apply(&file).is_err(),
            "mechanism modules must not be allowed to call specific slice modules directly"
        );
    }

    #[test]
    fn remediate_slice_owns_selection_brief_and_execution() {
        let remediations = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/remediation.rs"),
        )
        .unwrap();

        assert!(remediations.contains("slices::remediate"));
        assert!(!remediations.contains("TmuxAdapter::new("));
        assert!(!remediations.contains("send_agent_command("));

        let slice = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/slices/remediate.rs"),
        )
        .unwrap();

        for required in [
            "pub fn remediations_for_task",
            "pub fn format_brief",
            "pub fn execute_remediation",
            "send_agent_command(",
        ] {
            assert!(
                slice.contains(required),
                "remediate slice should own `{required}`"
            );
        }
    }

    #[test]
    fn pane_prompt_answering_remains_outside_task_action_decisions() {
        let recommended = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/recommended.rs"),
        )
        .unwrap();

        assert!(!recommended.contains("PromptAnswer"));
        assert!(!recommended.contains("capture_prompt"));
        assert!(!recommended.contains("answer_prompt"));
    }

    #[test]
    fn compatibility_operation_modules_are_not_public_core_api() {
        let lib_rs = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/lib.rs"),
        )
        .unwrap();

        assert!(!lib_rs.contains("pub mod operation;"));
        assert!(!lib_rs.contains("pub mod task_operations;"));
        assert!(!lib_rs.contains("mod task_operations;"));
    }

    #[test]
    fn capability_slices_and_command_plans_do_not_depend_on_operation_module() {
        for path in [
            "src/slices/resume.rs",
            "src/slices/ship.rs",
            "src/slices/drop.rs",
            "src/slices/review/decision.rs",
            "src/slices/review/planning.rs",
            "src/commands/open.rs",
            "src/commands/check.rs",
            "src/commands/merge.rs",
            "src/commands/teardown.rs",
            "src/recommended.rs",
        ] {
            let file = std::fs::read_to_string(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(path),
            )
            .unwrap();

            assert!(
                !file.contains("operation::"),
                "{path} should not depend on the compatibility operation module"
            );
        }
    }

    #[test]
    fn task_action_slices_do_not_depend_on_legacy_task_command_executor() {
        for path in [
            "src/slices/resume.rs",
            "src/slices/review/mod.rs",
            "src/slices/repair.rs",
            "src/slices/ship.rs",
        ] {
            let file = std::fs::read_to_string(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(path),
            )
            .unwrap();

            for forbidden in [
                "TaskCommandKind",
                "plan_task_command_operation(",
                "execute_task_command_operation(",
                "task_operations::task_command",
            ] {
                assert!(
                    !file.contains(forbidden),
                    "{path} should not depend on `{forbidden}`"
                );
            }
        }
    }

    #[test]
    fn start_and_tidy_slices_do_not_depend_on_task_operations_modules() {
        for path in ["src/slices/start.rs", "src/slices/tidy.rs"] {
            let file = std::fs::read_to_string(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(path),
            )
            .unwrap();

            assert!(
                !file.contains("task_operations::"),
                "{path} should not depend on task_operations compatibility modules"
            );
        }
    }

    #[test]
    fn task_operations_file_does_not_own_start_task_command_or_tidy_anymore() {
        let legacy_path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/task_operations.rs");
        assert!(
            !legacy_path.exists(),
            "task_operations.rs should be removed after slice migration"
        );
    }

    #[test]
    fn drop_slice_does_not_depend_on_task_operations_drop_module() {
        let drop_slice = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/slices/drop.rs"),
        )
        .unwrap();

        for forbidden in [
            "task_operations::drop_task",
            "plan_drop_task_operation(",
            "execute_drop_task_operation(",
            "complete_drop_task_operation(",
        ] {
            assert!(
                !drop_slice.contains(forbidden),
                "drop slice should not depend on `{forbidden}`"
            );
        }
    }

    #[test]
    fn read_capabilities_live_under_cockpit_slice_facade() {
        let slice = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/slices/cockpit.rs"),
        )
        .unwrap();

        for required in [
            "pub fn list_repos",
            "pub fn list_tasks",
            "pub fn review_queue",
            "pub fn inbox",
            "pub fn cockpit",
            "pub fn cockpit_view",
        ] {
            assert!(
                slice.contains(required),
                "cockpit slice facade should export `{required}`"
            );
        }
    }

    #[test]
    fn cockpit_slice_does_not_delegate_read_capabilities_back_to_commands() {
        let slice = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/slices/cockpit.rs"),
        )
        .unwrap();

        for forbidden in [
            "commands::list_repos(",
            "commands::list_tasks(",
            "commands::review_queue(",
            "commands::inspect_task(",
            "commands::inbox(",
            "commands::next(",
            "commands::status(",
            "commands::cockpit(",
            "commands::cockpit_view(",
        ] {
            assert!(
                !slice.contains(forbidden),
                "cockpit slice should own read capability instead of delegating `{forbidden}`"
            );
        }
    }

    fn forbidden_paths_for_slices(slices: &[&str]) -> Vec<String> {
        slices
            .iter()
            .flat_map(|slice| {
                [
                    format!("ajax-core::slices::{slice}"),
                    format!("crate::slices::{slice}"),
                ]
            })
            .collect()
    }

    fn forbidden_paths_for_sibling_slices(slice: &str) -> Vec<String> {
        let siblings = SLICES
            .iter()
            .copied()
            .filter(|sibling| *sibling != slice)
            .collect::<Vec<_>>();
        forbidden_paths_for_slices(&siblings)
    }
}
