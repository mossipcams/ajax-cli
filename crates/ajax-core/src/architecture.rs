#[cfg(test)]
mod tests {
    use rust_arkitect::dsl::{
        architectural_rules::ArchitecturalRules, arkitect::Arkitect, project::Project,
    };
    use rust_arkitect::{
        rule::Rule, rules::must_not_depend_on::MustNotDependOnRule, rust_file::RustFile,
    };

    const FORBIDDEN_SLICE_DEPENDENCIES: [&str; 2] = ["ajax-core::slices", "crate::slices"];

    #[test]
    fn substrate_mechanisms_do_not_depend_on_slices() {
        let project = Project::from_current_crate();

        #[rustfmt::skip]
        let rules = ArchitecturalRules::define()
            .rules_for_module("ajax-core::adapters")
                .it_must_not_depend_on(&FORBIDDEN_SLICE_DEPENDENCIES)
            .rules_for_module("ajax-core::registry")
                .it_must_not_depend_on(&FORBIDDEN_SLICE_DEPENDENCIES)
            .rules_for_module("ajax-core::analysis")
                .it_must_not_depend_on(&FORBIDDEN_SLICE_DEPENDENCIES)
            .rules_for_module("ajax-core::runtime")
                .it_must_not_depend_on(&FORBIDDEN_SLICE_DEPENDENCIES)
            .build();

        let result = Arkitect::ensure_that(project).complies_with(rules);

        assert!(
            result.is_ok(),
            "architecture violations: {:#?}",
            result.err().unwrap_or_default()
        );
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
            forbidden_slice_dependencies(),
        );

        assert!(
            rule.apply(&file).is_err(),
            "mechanism modules must not be allowed to import crate::slices"
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
            forbidden_slice_dependencies(),
        );

        assert!(
            rule.apply(&file).is_err(),
            "mechanism modules must not be allowed to call crate::slices directly"
        );
    }

    fn forbidden_slice_dependencies() -> Vec<String> {
        FORBIDDEN_SLICE_DEPENDENCIES
            .iter()
            .map(|dependency| (*dependency).to_string())
            .collect()
    }
}
