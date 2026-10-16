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

    #[test]
    fn native_harness_code_never_names_the_acp_protocol() {
        let forbidden = [
            concat!("agent_client", "_protocol"),
            concat!("AcpClient", "Event"),
        ];
        let files = [
            "claude_sdk_client.rs",
            "claude_sdk_map.rs",
            "cursor_sdk_client.rs",
            "cursor_sdk_map.rs",
            "cursor_sdk_spawn.rs",
            "claude_sdk_spawn.rs",
            "pi_rpc_client.rs",
            "pi_rpc_map.rs",
            "pi_rpc_handshake.rs",
            "rpc_client.rs",
            "rpc_session.rs",
            "rpc_handshake.rs",
            "jsonl_process.rs",
        ];
        for name in files {
            let source = std::fs::read_to_string(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("src/adapters/web_session_acp")
                    .join(name),
            )
            .unwrap_or_else(|error| panic!("missing native harness file {name}: {error}"));
            for token in forbidden {
                assert!(
                    !source.contains(token),
                    "{name} contains `{token}`: the Claude Agent SDK and Pi RPC code must stay free of ACP protocol types; convert at SessionClient instead"
                );
            }
        }
    }

    #[test]
    fn harness_transport_is_selected_through_transport_for() {
        let files = [
            "session_client.rs",
            "claude_sdk_spawn.rs",
            "cursor_sdk_spawn.rs",
            "catalog.rs",
        ];
        let forbidden = [
            concat!("fn uses_", "pi_rpc("),
            concat!("fn uses_", "claude_sdk("),
            concat!("fn uses_", "cursor_sdk("),
        ];
        for name in files {
            let source = std::fs::read_to_string(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("src/adapters/web_session_acp")
                    .join(name),
            )
            .unwrap_or_else(|error| panic!("missing web_session_acp file {name}: {error}"));
            for token in forbidden {
                assert!(
                    !source.contains(token),
                    "{name} contains `{token}`: harness transport selection must go through `transport_for`, not per-harness feature gates"
                );
            }
        }
        let session_client = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("src/adapters/web_session_acp/session_client.rs"),
        )
        .unwrap_or_else(|error| panic!("missing web_session_acp file session_client.rs: {error}"));
        assert_eq!(
            session_client.matches("fn transport_for(").count(),
            1,
            "session_client.rs must define `transport_for(` exactly once as the single transport selection point"
        );
    }

    #[test]
    fn session_client_shared_methods_dispatch_on_two_arms() {
        let source = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("src/adapters/web_session_acp/session_client.rs"),
        )
        .unwrap_or_else(|error| panic!("missing web_session_acp file session_client.rs: {error}"));
        for enum_token in ["enum Transport<", "enum TransportMut<"] {
            assert!(
                source.contains(enum_token),
                "session_client.rs is missing `{enum_token}`: the two-arm dispatch needs the transport enums"
            );
        }
        let harness_arms = [
            concat!("SessionClient::", "Pi("),
            concat!("SessionClient::", "Claude("),
            concat!("SessionClient::", "Cursor("),
        ];
        let methods = [
            "pub fn shutdown(",
            "pub fn host_exited(",
            "pub fn child_pid(",
            "pub fn poll_event(",
            "pub fn wait_event(",
            "pub(crate) fn cancel(",
            "pub(crate) fn prompt_in_flight(",
        ];
        for signature in methods {
            let start = source
                .find(signature)
                .unwrap_or_else(|| panic!("session_client.rs is missing `{signature}`"));
            let rest = &source[start..];
            let end_candidates: Vec<usize> = ["\n    pub", "\n    #[cfg(test)]"]
                .iter()
                .filter_map(|marker| rest.find(marker))
                .collect();
            let end = end_candidates.into_iter().next().unwrap_or(rest.len());
            let body = &rest[..end];
            for token in harness_arms {
                assert!(
                    !body.contains(token),
                    "{signature} contains `{token}`: the shared methods must match on `transport()`/`transport_mut()` (two arms), not on each harness"
                );
            }
        }
    }
}
