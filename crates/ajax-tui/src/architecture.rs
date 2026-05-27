#[cfg(test)]
mod tests {
    use rust_arkitect::dsl::{
        architectural_rules::ArchitecturalRules, arkitect::Arkitect, project::Project,
    };
    use rust_arkitect::{
        rule::Rule, rules::must_not_depend_on::MustNotDependOnRule, rust_file::RustFile,
    };

    const FORBIDDEN_RUNTIME_DEPENDENCIES: [&str; 2] = ["ajax-tui::runtime", "crate::runtime"];

    #[test]
    fn cockpit_pieces_do_not_depend_on_runtime() {
        let project = Project::from_current_crate();

        #[rustfmt::skip]
        let rules = ArchitecturalRules::define()
            .rules_for_module("ajax-tui::actions")
                .it_must_not_depend_on(&FORBIDDEN_RUNTIME_DEPENDENCIES)
            .rules_for_module("ajax-tui::cockpit_state")
                .it_must_not_depend_on(&FORBIDDEN_RUNTIME_DEPENDENCIES)
            .rules_for_module("ajax-tui::input")
                .it_must_not_depend_on(&FORBIDDEN_RUNTIME_DEPENDENCIES)
            .rules_for_module("ajax-tui::layout")
                .it_must_not_depend_on(&FORBIDDEN_RUNTIME_DEPENDENCIES)
            .rules_for_module("ajax-tui::navigation")
                .it_must_not_depend_on(&FORBIDDEN_RUNTIME_DEPENDENCIES)
            .rules_for_module("ajax-tui::palette")
                .it_must_not_depend_on(&FORBIDDEN_RUNTIME_DEPENDENCIES)
            .rules_for_module("ajax-tui::rendering")
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
    fn architecture_rule_rejects_cockpit_state_importing_runtime() {
        let file = RustFile::from_content(
            "src/cockpit_state.rs",
            "ajax-tui::cockpit_state",
            "use crate::runtime::run_interactive;",
        );
        let rule = MustNotDependOnRule::new(
            "ajax-tui::cockpit_state".to_string(),
            forbidden_runtime_dependencies(),
        );

        assert!(
            rule.apply(&file).is_err(),
            "Cockpit state must remain testable without depending on the runtime event loop"
        );
    }

    fn forbidden_runtime_dependencies() -> Vec<String> {
        FORBIDDEN_RUNTIME_DEPENDENCIES
            .iter()
            .map(|dependency| (*dependency).to_string())
            .collect()
    }
}
