#[cfg(test)]
mod tests {
    use rust_arkitect::dsl::{
        architectural_rules::ArchitecturalRules, arkitect::Arkitect, project::Project,
    };
    use rust_arkitect::{
        rule::Rule, rules::must_not_depend_on::MustNotDependOnRule, rust_file::RustFile,
    };

    const FORBIDDEN_RUNTIME_DEPENDENCIES: [&str; 2] =
        ["ajax-supervisor::runtime", "crate::runtime"];

    #[test]
    fn supervisor_substrates_do_not_depend_on_runtime() {
        let project = Project::from_current_crate();

        #[rustfmt::skip]
        let rules = ArchitecturalRules::define()
            .rules_for_module("ajax-supervisor::agent")
                .it_must_not_depend_on(&FORBIDDEN_RUNTIME_DEPENDENCIES)
            .rules_for_module("ajax-supervisor::event_log")
                .it_must_not_depend_on(&FORBIDDEN_RUNTIME_DEPENDENCIES)
            .rules_for_module("ajax-supervisor::process_observer")
                .it_must_not_depend_on(&FORBIDDEN_RUNTIME_DEPENDENCIES)
            .rules_for_module("ajax-supervisor::repo_observer")
                .it_must_not_depend_on(&FORBIDDEN_RUNTIME_DEPENDENCIES)
            .rules_for_module("ajax-supervisor::renderer")
                .it_must_not_depend_on(&FORBIDDEN_RUNTIME_DEPENDENCIES)
            .rules_for_module("ajax-supervisor::status")
                .it_must_not_depend_on(&FORBIDDEN_RUNTIME_DEPENDENCIES)
            .build();

        let result = Arkitect::ensure_that(project).complies_with(rules);

        assert!(
            result.is_ok(),
            "architecture violations: {:#?}",
            result.err().unwrap_or_default()
        );
    }

    #[test]
    fn architecture_rule_rejects_observer_importing_runtime() {
        let file = RustFile::from_content(
            "src/process_observer.rs",
            "ajax-supervisor::process_observer",
            "use crate::runtime::spawn_monitor;",
        );
        let rule = MustNotDependOnRule::new(
            "ajax-supervisor::process_observer".to_string(),
            forbidden_runtime_dependencies(),
        );

        assert!(
            rule.apply(&file).is_err(),
            "supervisor substrate observers must not depend on the runtime composer"
        );
    }

    fn forbidden_runtime_dependencies() -> Vec<String> {
        FORBIDDEN_RUNTIME_DEPENDENCIES
            .iter()
            .map(|dependency| (*dependency).to_string())
            .collect()
    }
}
