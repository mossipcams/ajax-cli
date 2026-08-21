//! Granular orchestration-chat module import guards.
//!
//! Enforces the forbidden-import table in
//! `.planning/agent-plans/architecture-granular-rules.md` against production
//! sources only (skips `*_tests.rs`, `test_support.rs`, and `runtime/tests/**`).

#[cfg(test)]
mod tests {
    use crate::architecture::scan::{
        assert_production_module_does_not_depend_on, forbidden_tokens, source_mentions_dependency,
    };

    const PROTOCOL_MAPPING_MODULES: [&str; 4] = ["protocol", "acp_map", "normalize", "acp_usage"];
    const RUNTIME_DEPENDENCIES: [&str; 2] = ["ajax-web::runtime", "crate::runtime"];

    fn runtime_forbidden() -> Vec<String> {
        forbidden_tokens(&RUNTIME_DEPENDENCIES)
    }

    fn protocol_mapping_forbidden() -> Vec<String> {
        let mut forbidden = forbidden_tokens(&[
            "web_session_store",
            "AcpStdioClient",
            "task_session_directory",
            "task_session_spawn",
            "acp_drain",
        ]);
        forbidden.extend(runtime_forbidden());
        forbidden
    }

    fn replay_forbidden() -> Vec<String> {
        let mut forbidden = forbidden_tokens(&[
            "web_session_store",
            "AcpStdioClient",
            "task_session_spawn",
            "acp_drain",
            "ws_bridge",
        ]);
        forbidden.extend(runtime_forbidden());
        forbidden
    }

    fn transcript_model_change_forbidden() -> Vec<String> {
        let mut forbidden = forbidden_tokens(&[
            "AcpStdioClient",
            "task_session_spawn",
            "acp_drain",
            "ws_bridge",
        ]);
        forbidden.extend(runtime_forbidden());
        forbidden
    }

    fn ws_bridge_forbidden() -> Vec<String> {
        let mut forbidden = forbidden_tokens(&[
            "web_session_store",
            "AcpStdioClient",
            "acp_drain",
            "acp_map",
            "StreamNormalizer",
        ]);
        forbidden.extend(runtime_forbidden());
        forbidden
    }

    fn session_cleanup_forbidden() -> Vec<String> {
        let mut forbidden =
            forbidden_tokens(&["AcpStdioClient", "task_session_spawn", "ws_bridge"]);
        forbidden.extend(runtime_forbidden());
        forbidden
    }

    fn web_session_acp_forbidden() -> Vec<String> {
        forbidden_tokens(&[
            "SessionClientMessage",
            "SessionServerEvent",
            "TaskSessionDirectory",
            "bridge_task_session",
        ])
    }

    fn web_session_store_forbidden() -> Vec<String> {
        forbidden_tokens(&[
            "AcpStdioClient",
            "AcpClientEvent",
            "SessionClientMessage",
            "SessionServerEvent",
            "TaskSessionDirectory",
        ])
    }

    fn runtime_production_forbidden() -> Vec<String> {
        forbidden_tokens(&[
            "web_session_store",
            "AcpStdioClient",
            "web_session::task_session::",
            "web_session::acp_drain",
            "web_session::acp_map",
        ])
    }

    fn assert_web_session_module(module: &str, forbidden: &[String]) {
        assert_production_module_does_not_depend_on(
            &format!("ajax-web::slices::web_session::{module}"),
            forbidden,
            "web_session module",
            module,
        );
    }

    #[test]
    fn protocol_mapping_modules_do_not_import_session_internals() {
        let forbidden = protocol_mapping_forbidden();
        for module in PROTOCOL_MAPPING_MODULES {
            assert_web_session_module(module, &forbidden);
        }
    }

    #[test]
    fn replay_module_does_not_import_session_internals() {
        assert_web_session_module("replay", &replay_forbidden());
    }

    #[test]
    fn transcript_and_model_change_modules_do_not_import_session_internals() {
        let forbidden = transcript_model_change_forbidden();
        for module in ["transcript", "model_change"] {
            assert_web_session_module(module, &forbidden);
        }
    }

    #[test]
    fn ws_bridge_module_does_not_import_session_internals() {
        assert_web_session_module("ws_bridge", &ws_bridge_forbidden());
    }

    #[test]
    fn session_cleanup_module_does_not_import_session_internals() {
        assert_web_session_module("session_cleanup", &session_cleanup_forbidden());
    }

    #[test]
    fn web_session_acp_adapter_does_not_import_slice_wire_types() {
        assert_production_module_does_not_depend_on(
            "ajax-web::adapters::web_session_acp",
            &web_session_acp_forbidden(),
            "session mechanism adapter",
            "web_session_acp",
        );
    }

    #[test]
    fn web_session_store_adapter_does_not_import_acp_or_slice_wire_types() {
        assert_production_module_does_not_depend_on(
            "ajax-web::adapters::web_session_store",
            &web_session_store_forbidden(),
            "session mechanism adapter",
            "web_session_store",
        );
    }

    #[test]
    fn runtime_production_does_not_import_session_internals() {
        assert_production_module_does_not_depend_on(
            "ajax-web::runtime",
            &runtime_production_forbidden(),
            "runtime production",
            "runtime",
        );
    }

    #[test]
    fn architecture_rule_rejects_protocol_importing_acp_stdio_client() {
        assert!(
            source_mentions_dependency(
                "use crate::adapters::web_session_acp::AcpStdioClient;",
                &protocol_mapping_forbidden(),
            ),
            "protocol mapping modules must not import AcpStdioClient"
        );
    }
}
