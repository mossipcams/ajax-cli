#[cfg(test)]
mod tests {
    use rust_arkitect::dsl::{
        architectural_rules::ArchitecturalRules, arkitect::Arkitect, project::Project,
    };
    use rust_arkitect::{
        rule::Rule, rules::must_not_depend_on::MustNotDependOnRule, rust_file::RustFile,
    };

    const SLICES: [&str; 2] = ["pane", "review"];

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
    fn commands_module_does_not_own_external_command_execution_loop() {
        let source = std::fs::read_to_string("src/commands.rs").unwrap();

        assert!(
            !source.contains("for command in &plan.commands {"),
            "commands.rs should not own the external command execution loop"
        );
        if source.contains("pub fn execute_plan(") {
            assert!(
                source.contains("task_operations::kernel::execute_external_plan"),
                "execute_plan should only remain as a thin compatibility wrapper"
            );
        }
    }

    #[test]
    fn check_and_merge_do_not_mutate_tasks_through_raw_registry_access() {
        for file in ["src/commands/check.rs", "src/commands/merge.rs"] {
            let source = std::fs::read_to_string(file).unwrap();
            assert!(
                !source.contains(".get_task_mut("),
                "{file} should mutate task lifecycle through typed helpers instead of raw registry access"
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
