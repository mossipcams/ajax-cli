#[cfg(test)]
mod tests {
    use rust_arkitect::dsl::{
        architectural_rules::ArchitecturalRules, arkitect::Arkitect, project::Project,
    };
    use rust_arkitect::{
        rule::Rule, rules::must_not_depend_on::MustNotDependOnRule, rust_file::RustFile,
    };

    const SLICES: [&str; 4] = ["attention", "cockpit", "install", "operate"];
    const ADAPTERS: [&str; 4] = ["assets", "http", "push", "tls"];

    const FORBIDDEN_RUNTIME_DEPENDENCIES: [&str; 2] = ["ajax-web::runtime", "crate::runtime"];

    #[test]
    fn each_web_adapter_does_not_depend_on_slices_or_runtime() {
        for adapter in ADAPTERS {
            let project = Project::from_current_crate();
            let forbidden_slices = forbidden_paths_for_slices(&SLICES);
            let forbidden_runtime = forbidden_runtime_dependencies();
            let forbidden = forbidden_slices
                .iter()
                .chain(forbidden_runtime.iter())
                .map(String::as_str)
                .collect::<Vec<_>>();
            let module = format!("ajax-web::adapters::{adapter}");

            #[rustfmt::skip]
            let rules = ArchitecturalRules::define()
                .rules_for_module(module.as_str())
                    .it_must_not_depend_on(&forbidden)
                .build();

            let result = Arkitect::ensure_that(project).complies_with(rules);

            assert!(
                result.is_ok(),
                "architecture violations in adapter `{adapter}`: {:#?}",
                result.err().unwrap_or_default()
            );
        }
    }

    #[test]
    fn action_vocabulary_does_not_depend_on_slices_or_runtime() {
        let project = Project::from_current_crate();
        let forbidden_slices = forbidden_paths_for_slices(&SLICES);
        let forbidden_runtime = forbidden_runtime_dependencies();
        let forbidden = forbidden_slices
            .iter()
            .chain(forbidden_runtime.iter())
            .map(String::as_str)
            .collect::<Vec<_>>();

        #[rustfmt::skip]
        let rules = ArchitecturalRules::define()
            .rules_for_module("ajax-web::action_vocabulary")
                .it_must_not_depend_on(&forbidden)
            .build();

        let result = Arkitect::ensure_that(project).complies_with(rules);

        assert!(
            result.is_ok(),
            "architecture violations in action_vocabulary: {:#?}",
            result.err().unwrap_or_default()
        );
    }

    #[test]
    fn each_web_slice_is_isolated_from_sibling_slices_and_runtime() {
        for slice in SLICES {
            let project = Project::from_current_crate();
            let forbidden_siblings = forbidden_paths_for_sibling_slices(slice);
            let forbidden_runtime = forbidden_runtime_dependencies();
            let forbidden = forbidden_siblings
                .iter()
                .chain(forbidden_runtime.iter())
                .map(String::as_str)
                .collect::<Vec<_>>();
            let module = format!("ajax-web::slices::{slice}");

            let rules = ArchitecturalRules::define()
                .rules_for_module(module.as_str())
                .it_must_not_depend_on(&forbidden)
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
    fn architecture_rule_rejects_cross_slice_dependency() {
        let file = RustFile::from_content(
            "src/slices/cockpit.rs",
            "ajax-web::slices::cockpit",
            "use crate::slices::operate::OperateRequest;",
        );
        let rule = MustNotDependOnRule::new(
            "ajax-web::slices::cockpit".to_string(),
            forbidden_paths_for_sibling_slices("cockpit"),
        );

        assert!(
            rule.apply(&file).is_err(),
            "web slices must be independent of sibling slices"
        );
    }

    #[test]
    fn architecture_rule_rejects_adapter_importing_specific_slice() {
        let file = RustFile::from_content(
            "src/adapters/http.rs",
            "ajax-web::adapters::http",
            "use crate::slices::install::pwa_shell;",
        );
        let rule = MustNotDependOnRule::new(
            "ajax-web::adapters::http".to_string(),
            forbidden_paths_for_slices(&["install"]),
        );

        assert!(
            rule.apply(&file).is_err(),
            "web adapter mechanisms must not import any specific slice"
        );
    }

    fn forbidden_paths_for_slices(slices: &[&str]) -> Vec<String> {
        slices
            .iter()
            .flat_map(|slice| {
                [
                    format!("ajax-web::slices::{slice}"),
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

    fn forbidden_runtime_dependencies() -> Vec<String> {
        FORBIDDEN_RUNTIME_DEPENDENCIES
            .iter()
            .map(|dependency| (*dependency).to_string())
            .collect()
    }
}
