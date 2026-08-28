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

    fn read_production_source(relative: &str) -> String {
        std::fs::read_to_string(format!("src/{relative}")).unwrap()
    }

    #[test]
    fn architecture_rule_stored_id_restore_fail_closes_without_session_new() {
        let source = read_production_source("adapters/web_session_acp/sdk_connection.rs");
        let fn_start = source
            .find("async fn initialize_session")
            .expect("initialize_session must exist");
        let fn_body = &source[fn_start..];
        let resume_start = fn_body
            .find("if let Some(resume_id) = resume_session_id")
            .expect("stored-id restore branch must exist");
        let new_session_start = fn_body
            .find("let (session_id, mut session_new_result)")
            .expect("session/new branch must exist");
        let resume_block = &fn_body[resume_start..new_session_start];
        assert!(
            resume_block.contains("return Err(super::client::restore_unavailable_error"),
            "failed resume/load must return RestoreUnavailable instead of falling through"
        );
        assert!(
            !resume_block.contains("NewSessionRequest"),
            "stored-id restore path must never call session/new"
        );
    }

    #[test]
    fn architecture_rule_session_snapshot_requires_context_continuity_fields() {
        let source = read_production_source("slices/web_session/protocol.rs");
        for marker in [
            "#[serde(rename = \"contextState\")]",
            "#[serde(rename = \"contextEpoch\")]",
            "rename = \"contextError\"",
            "pub context_state: ContextState",
            "pub context_epoch: u64",
        ] {
            assert!(
                source.contains(marker),
                "SessionSnapshot must expose required continuity field `{marker}`"
            );
        }
        assert!(
            !source.contains(
                "skip_serializing_if = \"Option::is_none\"\n    )]\n    pub context_state"
            ),
            "contextState must be required on the snapshot wire"
        );
    }

    #[test]
    fn architecture_rule_typescript_snapshot_requires_context_continuity_fields() {
        let contracts =
            std::fs::read_to_string("web/src/features/chat/session/transport/contracts.ts")
                .unwrap();
        for marker in ["contextState: ContextState", "contextEpoch: number"] {
            assert!(
                contracts.contains(marker),
                "SessionSnapshot interface must require `{marker}`"
            );
        }

        let parse =
            std::fs::read_to_string("web/src/features/chat/session/transport/parse.ts").unwrap();
        for marker in [
            "parseContextState(payload.contextState)",
            "parseContextEpoch(payload.contextEpoch)",
            "if (!contextState || contextEpoch === null) return null",
        ] {
            assert!(
                parse.contains(marker),
                "snapshot parser must require continuity field `{marker}`"
            );
        }
    }
}
