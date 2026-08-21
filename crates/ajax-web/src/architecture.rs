#[cfg(test)]
pub(crate) mod scan {
    use std::path::{Path, PathBuf};

    pub(crate) fn assert_module_does_not_depend_on(
        module: &str,
        forbidden: &[String],
        kind: &str,
        name: &str,
    ) {
        let violations = module_sources(module)
            .into_iter()
            .filter_map(|path| {
                let source = std::fs::read_to_string(&path).unwrap();
                source_mentions_dependency(&source, forbidden).then_some(path)
            })
            .collect::<Vec<_>>();

        assert!(
            violations.is_empty(),
            "architecture violations in {kind} `{name}`: {violations:#?}"
        );
    }

    pub(crate) fn assert_production_module_does_not_depend_on(
        module: &str,
        forbidden: &[String],
        kind: &str,
        name: &str,
    ) {
        let violations = production_module_sources(module)
            .into_iter()
            .filter_map(|path| {
                let source = std::fs::read_to_string(&path).unwrap();
                source_mentions_dependency(&source, forbidden).then_some(path)
            })
            .collect::<Vec<_>>();

        assert!(
            violations.is_empty(),
            "architecture violations in {kind} `{name}`: {violations:#?}"
        );
    }

    pub(crate) fn production_module_sources(module: &str) -> Vec<PathBuf> {
        module_sources(module)
            .into_iter()
            .filter(|path| !is_skipped_production_source(path))
            .collect()
    }

    pub(crate) fn is_skipped_production_source(path: &Path) -> bool {
        if path
            .components()
            .any(|component| component.as_os_str() == "tests")
            && path
                .components()
                .any(|component| component.as_os_str() == "runtime")
        {
            return true;
        }
        path.file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with("_tests.rs") || name == "test_support.rs")
    }

    pub(crate) fn module_sources(module: &str) -> Vec<PathBuf> {
        let relative = module.split("::").skip(1).collect::<Vec<_>>().join("/");
        let file = PathBuf::from("src").join(format!("{relative}.rs"));
        let dir = PathBuf::from("src").join(relative);
        let mut sources = Vec::new();
        if file.exists() {
            sources.push(file);
        }
        if dir.exists() {
            collect_rust_files(&dir, &mut sources);
        }
        sources
    }

    pub(crate) fn collect_rust_files(dir: &Path, files: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                collect_rust_files(&path, files);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                files.push(path);
            }
        }
    }

    pub(crate) fn source_mentions_dependency(source: &str, forbidden: &[String]) -> bool {
        forbidden
            .iter()
            .any(|dependency| source_mentions_path(source, dependency))
    }

    pub(crate) fn source_mentions_path(source: &str, dependency: &str) -> bool {
        if source.contains(dependency) {
            return true;
        }
        let Some((parent, child)) = dependency.rsplit_once("::") else {
            return false;
        };
        source.contains(&format!("{parent}::{{")) && source.contains(child)
    }

    pub(crate) fn forbidden_tokens(tokens: &[&str]) -> Vec<String> {
        tokens.iter().map(|token| (*token).to_string()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::scan::{
        assert_module_does_not_depend_on, module_sources, source_mentions_dependency,
    };
    use std::path::{Path, PathBuf};

    const SLICES: [&str; 10] = [
        "cockpit",
        "dev_deploy",
        "diff_review",
        "install",
        "operate",
        "push",
        "session_models",
        "stt",
        "terminal",
        "web_session",
    ];
    const ADAPTERS: [&str; 12] = [
        "assets",
        "browser_session",
        "cloudflare_access",
        "http",
        "program",
        "server",
        "skills",
        "stt_provider",
        "terminal_pty",
        "tls",
        "web_session_acp",
        "web_session_store",
    ];

    const FORBIDDEN_RUNTIME_DEPENDENCIES: [&str; 2] = ["ajax-web::runtime", "crate::runtime"];
    const SESSION_MECHANISM_ADAPTERS: [&str; 2] = ["web_session_acp", "web_session_store"];
    const WEB_SESSION_SLICE: &str = "web_session";

    #[test]
    fn session_mechanism_adapters_do_not_depend_on_web_session_slice() {
        let forbidden = [
            format!("ajax-web::slices::{WEB_SESSION_SLICE}"),
            format!("crate::slices::{WEB_SESSION_SLICE}"),
        ];
        for adapter in SESSION_MECHANISM_ADAPTERS {
            assert_module_does_not_depend_on(
                &format!("ajax-web::adapters::{adapter}"),
                &forbidden,
                "session mechanism adapter",
                adapter,
            );
        }
    }

    #[test]
    fn session_mechanism_adapters_do_not_depend_on_each_other() {
        for adapter in SESSION_MECHANISM_ADAPTERS {
            let forbidden = SESSION_MECHANISM_ADAPTERS
                .iter()
                .filter(|other| **other != adapter)
                .flat_map(|other| {
                    [
                        format!("ajax-web::adapters::{other}"),
                        format!("crate::adapters::{other}"),
                    ]
                })
                .collect::<Vec<_>>();
            assert_module_does_not_depend_on(
                &format!("ajax-web::adapters::{adapter}"),
                &forbidden,
                "session mechanism adapter",
                adapter,
            );
        }
    }

    #[test]
    fn web_session_slice_may_call_session_mechanism_adapters() {
        let sources = module_sources(&format!("ajax-web::slices::{WEB_SESSION_SLICE}"));
        let joined = sources
            .iter()
            .filter_map(|path| std::fs::read_to_string(path).ok())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            joined.contains("web_session_acp") && joined.contains("web_session_store"),
            "web_session slice should depend on session mechanism adapters"
        );
    }

    #[test]
    fn each_web_adapter_does_not_depend_on_slices_or_runtime() {
        for adapter in ADAPTERS {
            let forbidden_slices = forbidden_slices_for_adapter(adapter);
            let forbidden_runtime = forbidden_runtime_dependencies();
            let forbidden = forbidden_slices
                .iter()
                .chain(forbidden_runtime.iter())
                .cloned()
                .collect::<Vec<_>>();
            let module = format!("ajax-web::adapters::{adapter}");

            assert_module_does_not_depend_on(&module, &forbidden, "adapter", adapter);
        }
    }

    #[test]
    fn actions_module_does_not_depend_on_sibling_slices_or_runtime() {
        let forbidden_slices = forbidden_paths_for_slices(&SLICES);
        let forbidden_runtime = forbidden_runtime_dependencies();
        let forbidden = forbidden_slices
            .iter()
            .chain(forbidden_runtime.iter())
            .cloned()
            .collect::<Vec<_>>();

        assert_module_does_not_depend_on(
            "ajax-web::slices::actions",
            &forbidden,
            "module",
            "actions",
        );
    }

    #[test]
    fn each_web_slice_is_isolated_from_sibling_slices_and_runtime() {
        for slice in SLICES {
            let forbidden_siblings = forbidden_paths_for_sibling_slices(slice);
            let forbidden_runtime = forbidden_runtime_dependencies();
            let forbidden = forbidden_siblings
                .iter()
                .chain(forbidden_runtime.iter())
                .cloned()
                .collect::<Vec<_>>();
            let module = format!("ajax-web::slices::{slice}");

            assert_module_does_not_depend_on(&module, &forbidden, "slice", slice);
        }
    }

    #[test]
    fn architecture_rule_rejects_cross_slice_dependency() {
        assert!(
            source_mentions_dependency(
                "use crate::slices::operate::OperateRequest;",
                &forbidden_paths_for_sibling_slices("cockpit")
            ),
            "web slices must be independent of sibling slices"
        );
    }

    #[test]
    fn architecture_rule_rejects_adapter_importing_specific_slice() {
        assert!(
            source_mentions_dependency(
                "use crate::slices::install::browser_shell;",
                &forbidden_paths_for_slices(&["install"])
            ),
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

    fn forbidden_slices_for_adapter(adapter: &str) -> Vec<String> {
        if adapter == "stt_provider" {
            let siblings = SLICES
                .iter()
                .copied()
                .filter(|slice| *slice != "stt")
                .collect::<Vec<_>>();
            forbidden_paths_for_slices(&siblings)
        } else {
            forbidden_paths_for_slices(&SLICES)
        }
    }

    fn forbidden_runtime_dependencies() -> Vec<String> {
        FORBIDDEN_RUNTIME_DEPENDENCIES
            .iter()
            .map(|dependency| (*dependency).to_string())
            .collect()
    }

    fn declared_modules(mod_rs: &str) -> std::collections::BTreeSet<String> {
        let source = std::fs::read_to_string(mod_rs).unwrap();
        source
            .lines()
            .filter_map(|line| {
                let line = line.trim();
                let name = line
                    .strip_prefix("pub mod ")
                    .or_else(|| line.strip_prefix("pub(crate) mod "))
                    .or_else(|| line.strip_prefix("mod "))?
                    .strip_suffix(';')?
                    .trim();
                (!name.is_empty()).then(|| name.to_string())
            })
            .collect()
    }

    #[test]
    fn vite_build_contract_emits_single_app_css() {
        let vite = std::fs::read_to_string("web/vite.config.mts").unwrap();
        assert!(
            vite.contains("cssCodeSplit: false"),
            "Vite must keep cssCodeSplit disabled so the shell ships one CSS asset"
        );
        assert!(
            vite.contains("if (name.endsWith(\".css\")) return \"app.css\""),
            "Vite must name the sole CSS bundle app.css for the Rust embed contract"
        );
    }

    #[test]
    fn web_src_stylesheet_graph_uses_manifest_and_owned_modules() {
        let mut css_files = Vec::new();
        collect_css_files(Path::new("web/src"), &mut css_files);
        css_files.sort();
        assert_eq!(
            css_files,
            vec![
                PathBuf::from("web/src/styles/app-shell/interact.css"),
                PathBuf::from("web/src/styles/app-shell/layout.css"),
                PathBuf::from("web/src/styles/app-shell/motion.css"),
                PathBuf::from("web/src/styles/app-shell/narrow.css"),
                PathBuf::from("web/src/styles/app-shell/nav.css"),
                PathBuf::from("web/src/styles/app-shell/page-lead.css"),
                PathBuf::from("web/src/styles/app-shell/primitives.css"),
                PathBuf::from("web/src/styles/app-shell/shell-layout.css"),
                PathBuf::from("web/src/styles/app-shell/skeleton.css"),
                PathBuf::from("web/src/styles/app-shell-continuation.css"),
                PathBuf::from("web/src/styles/app-shell-layout.css"),
                PathBuf::from("web/src/styles/app-shell.css"),
                PathBuf::from("web/src/styles/chat/activity.css"),
                PathBuf::from("web/src/styles/chat/composer.css"),
                PathBuf::from("web/src/styles/chat/conversation.css"),
                PathBuf::from("web/src/styles/chat/markdown.css"),
                PathBuf::from("web/src/styles/chat/model.css"),
                PathBuf::from("web/src/styles/chat/permissions.css"),
                PathBuf::from("web/src/styles/chat/queued.css"),
                PathBuf::from("web/src/styles/chat/scrolling.css"),
                PathBuf::from("web/src/styles/chat/status.css"),
                PathBuf::from("web/src/styles/chat/surface.css"),
                PathBuf::from("web/src/styles/chat.css"),
                PathBuf::from("web/src/styles/diff-review.css"),
                PathBuf::from("web/src/styles/foundation.css"),
                PathBuf::from("web/src/styles/settings.css"),
                PathBuf::from("web/src/styles/task/detail.css"),
                PathBuf::from("web/src/styles/task/list.css"),
                PathBuf::from("web/src/styles/task/meta.css"),
                PathBuf::from("web/src/styles/task/new-task.css"),
                PathBuf::from("web/src/styles/task/test-in-dev.css"),
                PathBuf::from("web/src/styles/task-workspace/sheets.css"),
                PathBuf::from("web/src/styles/task-workspace.css"),
                PathBuf::from("web/src/styles/task.css"),
                PathBuf::from("web/src/styles/terminal.css"),
                PathBuf::from("web/src/styles.css"),
            ],
            "T3 extraction keeps one JS manifest plus owned foundation, shell, settings, chat, task-workspace, app-shell continuation, task, terminal, shell-layout, and diff-review modules"
        );
    }

    #[test]
    fn main_entry_imports_only_the_styles_manifest() {
        let main_tsx = std::fs::read_to_string("web/src/app/main.tsx").unwrap();
        let css_imports = main_tsx
            .lines()
            .filter(|line| line.contains("import") && line.contains(".css"))
            .collect::<Vec<_>>();
        assert_eq!(
            css_imports,
            vec!["import \"../styles.css\";"],
            "main.tsx must remain the sole JS-side CSS entry point"
        );
    }

    #[test]
    fn static_asset_adapter_embeds_only_app_css() {
        let assets = std::fs::read_to_string("src/adapters/assets.rs").unwrap();
        assert!(
            assets.contains("include_bytes!(\"../../web/dist/app.css\")"),
            "embedded shell must ship dist/app.css"
        );
        assert!(
            assets.contains("/app.css\" => Some(StaticAsset"),
            "static asset lookup must expose /app.css"
        );
        assert!(
            !assets.contains("include_bytes!(\"../../web/dist/styles.css\")"),
            "source manifest must not be embedded as a second CSS asset"
        );
    }

    fn collect_css_files(dir: &Path, files: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                collect_css_files(&path, files);
            } else if path.extension().is_some_and(|extension| extension == "css") {
                files.push(path);
            }
        }
    }

    #[test]
    fn guarded_modules_match_declared_modules() {
        let declared_adapters = declared_modules("src/adapters/mod.rs");
        let guarded_adapters: std::collections::BTreeSet<String> =
            ADAPTERS.iter().map(|name| (*name).to_string()).collect();

        let missing_adapters = declared_adapters
            .difference(&guarded_adapters)
            .cloned()
            .collect::<Vec<_>>();
        let stale_adapters = guarded_adapters
            .difference(&declared_adapters)
            .cloned()
            .collect::<Vec<_>>();

        assert!(
            missing_adapters.is_empty() && stale_adapters.is_empty(),
            "declared web adapters missing an architecture guard: {missing_adapters:?}; stale guards: {stale_adapters:?}"
        );

        let declared_slices = declared_modules("src/slices/mod.rs");
        let mut guarded_slices: std::collections::BTreeSet<String> =
            SLICES.iter().map(|name| (*name).to_string()).collect();
        guarded_slices.insert("actions".to_string());

        let missing_slices = declared_slices
            .difference(&guarded_slices)
            .cloned()
            .collect::<Vec<_>>();
        let stale_slices = guarded_slices
            .difference(&declared_slices)
            .cloned()
            .collect::<Vec<_>>();

        assert!(
            missing_slices.is_empty() && stale_slices.is_empty(),
            "declared web slices missing an architecture guard: {missing_slices:?}; stale guards: {stale_slices:?}"
        );
    }
}
