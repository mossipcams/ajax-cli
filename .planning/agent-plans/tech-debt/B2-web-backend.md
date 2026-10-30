# B2 — ajax-web backend structural debt audit

Scope: tracked `crates/ajax-web/src` and `crates/ajax-web/sidecar`; frontend reads only to trace consumers. This report is the only authorized write. Audit performed in-process at the user's explicit direction. No implementation, test deletion, build, test suite, package installation, or Git mutation is authorized.

## Audit plan and status

- [x] Read AGENTS.md and architecture.md, including Chat-first, and focused Web Cockpit/session guidance.
- [x] Establish tracked-file baseline and perform first full AST visibility/reference sweep.
- [x] Measure normalized client/handshake/map/sidecar overlap.
- [ ] Confirm local-only declarations by hand and publish full sweep results/method.
- [ ] Trace wrappers, duplicated state, routes, and overlapping tests.
- [ ] Rank measured candidates, total without overlap, and verify report-only delta.

The existing wider plan is `.planning/agent-plans/tech-debt-15pct.md`; no additional plan file is created. Cleanup requires separate approval, especially where a documented contract or state ownership changes.

## Corrected baseline

`git ls-files crates/ajax-web/src crates/ajax-web/sidecar | xargs wc -l` → **81,947 total**, counted once. `src`: **79,867**, 240 tracked files; sidecar: **2,080**, five tracked files. Dedicated-test filename classification gives 38,142 production-file lines in src and 1,137 in sidecar, matching the plan's **39,279** production-file / **42,668** dedicated-test split. Inline tests still inflate production-file counts; candidate calculations exclude them explicitly.

`src/slices/web_session`: 91 files, 28,362 total, 11,113 production-file lines. `src/adapters/web_session_acp`: 73 files, 22,298 total, 9,629 production-file lines. These are not doubled `wc` totals. The previous B report's 159,734 src baseline is rejected.

## Findings (working order; final version ranked by net removal)

### B-01 Consolidate shared RPC forwarding and sidecar framing, retain real dialect behavior

- Paths: `src/adapters/web_session_acp/{claude_sdk_client,cursor_sdk_client,pi_rpc_client,session_client,rpc_client,rpc_session,rpc_handshake,pi_rpc_handshake,claude_sdk_map,cursor_sdk_map,pi_rpc_map}.rs`; `sidecar/{claude_sdk_sidecar,cursor_sdk_sidecar}.mjs`, all relative to `crates/ajax-web`.
- Prod lines removed: **159 net** in the concrete source-span design below: 104 client forwarding lines + 63 duplicate sidecar framing lines − 8 Rust assembly lines. Retained Rust/sidecar family: **3,908 of 4,067 production lines**. Exact normalized overlap and replacement accounting are reproduced below; this is not a compiled refactor.
- Test lines removed: **0 currently justified**. Dialect-specific fixture and protocol coverage stays.
- Evidence: AST function-body sweep selects bodies consisting solely of `self.core.<call>` without blocks or statements. Counts above are inclusive source spans, cross-checked against tracked per-file `wc -l`: clients 321/453/394; excluding terminal inline test modules 299/435/353. `session_client.rs:119–141` already returns `Transport::Native(session.client.core())` for all three clients; its event, cancellation, shutdown and liveness paths call the common core directly.
- Normalized overlap, using whitespace collapse and only the substitutions `ClaudeSdkClient|CursorSdkClient|PiRpcClient → DialectClient` and `claude_sdk|cursor_sdk|pi_rpc → dialect`: contiguous matches of at least three lines with at least two alphanumeric lines. Pairwise results: Claude/Cursor clients **115** lines, Claude/Pi **96**, Cursor/Pi **79**; handshakes **0**; Claude/Cursor maps **32**, Claude/Pi **14**, Cursor/Pi **7**; sidecars **140**. Pairwise figures overlap and MUST NOT be summed as savings. Braces/imports also match; these are duplicate-text measurements, not proof each matched line can be removed.
- What replaces it: use the existing dialect-parameterized `RpcSession` mapper and `RpcClientCore`; one copy of sidecar I/O framing. Keep Pi's four-request partial handshake, image type tags, provider-qualified model IDs and slash-command translation; keep Cursor model parameters/mode and Claude effort/usage/elicitation. A giant new generic protocol engine would add code.
- Behavior or public contract change: none intended. Preserve model catalogs, prompt acknowledgments, resume validation, stdout draining and shutdown guarantees.
- Risk: medium for forwarding; high for sidecar shutdown/dispatch sharing. No compiled prototype is claimed.
- Depends on: none.
- Confidence: likely for consolidation; exact overlap is proven, final net reduction must include replacement glue.


## Function-level reachability sweep

All tracked src Rust public declarations (including pub(crate)/pub(super)) were parsed with ast-grep. Dedicated test files and cfg(test) function/impl/module ranges were excluded. References span tracked code/config/scripts; comments, imports and strings are conservative references. This is a name-reference screen, not compiler call-graph proof: same-name methods can hide unused members. Local use, inferred types, trait dispatch and external library consumers need manual review. No-external-reference span totals are **not savings**. Test-file counts include all dedicated Rust tests, not only own tests.

```python
import subprocess, json, re, collections
from pathlib import Path
tracked = subprocess.check_output(['git','ls-files'],text=True).splitlines()
source = {p for p in tracked if p.startswith('crates/ajax-web/src/') and p.endswith('.rs')}
def test_file(p): return any('test' in part or part == 'fixtures' for part in Path(p).parts)
texts = {p:Path(p).read_text() for p in tracked if Path(p).is_file() and Path(p).suffix in
         ('.rs','.ts','.tsx','.mjs','.js','.py','.sh','.toml','.json','.yml','.yaml')}
def ast(kind):
    r=subprocess.run(['ast-grep','run','--lang','rust','--kind',kind,'--json=compact','crates/ajax-web/src'],capture_output=True,text=True)
    assert r.returncode in (0,1),r.stderr
    return [x for x in json.loads(r.stdout or '[]') if x['file'] in source]
def cfg_test(x):
    for line in reversed(texts[x['file']].splitlines()[:x['range']['start']['line']]):
        s=line.strip()
        if not s or s.startswith('//'): continue
        if s.startswith('#['):
            if re.search(r'cfg\(test\)',s): return True
            continue
        break
    return False
kinds=['function_item','function_signature_item','struct_item','enum_item','trait_item']
nodes={k:ast(k) for k in kinds+['mod_item','impl_item']}
excluded=collections.defaultdict(list)
for xs in nodes.values():
    for x in xs:
        if cfg_test(x): excluded[x['file']].append((x['range']['start']['line'],x['range']['end']['line']))
production={}
for path,t in texts.items():
    if test_file(path): continue
    rows=t.splitlines()
    for a,b in excluded[path]: rows[a:b+1]=['']*(b-a+1)
    production[path]='\n'.join(rows)
refs=collections.defaultdict(set); testrefs=collections.defaultdict(set); local={}
for path,t in production.items():
    local[path]=collections.Counter(re.findall(r'\b[A-Za-z_]\w*\b',t))
    for word in local[path]: refs[word].add(path)
for path,t in texts.items():
    if test_file(path) and path.endswith('.rs'):
        for word in set(re.findall(r'\b[A-Za-z_]\w*\b',t)): testrefs[word].add(path)
items=[]
for kind in kinds:
    for x in nodes[kind]:
        path=x['file']; a=x['range']['start']['line']; b=x['range']['end']['line']
        if path not in production or any(lo<=a<=hi for lo,hi in excluded[path]): continue
        m=re.match(r'pub(?:\([^)]*\))?\s+(?:(?:async|unsafe|const|extern)\s+)*(fn|struct|enum|trait)\s+(\w+)',x['text'])
        if m: items.append((path,a+1,b+1,m[1],m[2],len(refs[m[2]]-{path}),local[path][m[2]]-1,len(testrefs[m[2]]-{path})))
zero=[x for x in items if x[5]==0]
print('All src: declarations=%d; zero external production name references=%d; inclusive spans=%d' % (len(items),len(zero),sum(x[2]-x[1]+1 for x in zero)))
for area in ['slices/web_session/','adapters/web_session_acp/']:
    group=[x for x in items if area in x[0]]; z=[x for x in zero if area in x[0]]
    print('%s declarations=%d; zero=%d; spans=%d' % (area,len(group),len(z),sum(x[2]-x[1]+1 for x in z)))
print('| File:line | Kind/name | Span | Local tokens beyond declaration | Test files |')
print('|---|---|---:|---:|---:|')
for path,a,b,kind,name,_,here,tests in sorted(zero,key=lambda x:(-(x[2]-x[1]),x[0])):
    print('| %s:%d | %s `%s` | %d | %d | %d |' % (path.removeprefix('crates/ajax-web/src/'),a,kind,name,b-a+1,here,tests))
print('No local token and no outside production token:')
for x in zero:
    if x[6]==0: print('%s:%d %s %s (%d lines)' % (x[0],x[1],x[3],x[4],x[2]-x[1]+1))
```

All src: declarations=1009; zero external production name references=195; inclusive spans=2696
slices/web_session/ declarations=306; zero=48; spans=700
adapters/web_session_acp/ declarations=307; zero=34; spans=487
| File:line | Kind/name | Span | Local tokens beyond declaration | Test files |
|---|---|---:|---:|---:|
| runtime/task_routes/cockpit.rs:139 | fn `refresh_cockpit_and_cache_locked` | 72 | 2 | 0 |
| runtime/task_routes/cockpit.rs:605 | fn `axum_task_diff` | 71 | 1 | 0 |
| runtime/task_routes/cockpit.rs:452 | fn `axum_task_session_transcript` | 68 | 1 | 0 |
| adapters/web_session_acp/jsonl_process.rs:262 | fn `read_stdout_lines_bounded` | 64 | 1 | 1 |
| runtime/task_routes/cockpit.rs:547 | fn `axum_task_pull_requests` | 57 | 1 | 0 |
| slices/session_models.rs:305 | fn `collapse_cursor_catalog` | 51 | 1 | 0 |
| adapters/terminal_pty/attach.rs:185 | fn `build_isolated_attach_plan_with_token` | 49 | 2 | 1 |
| adapters/web_session_acp/rpc_session.rs:296 | fn `prompt_response_step` | 46 | 1 | 1 |
| slices/web_session/headless_dispatch.rs:16 | fn `run_queued_prompts` | 46 | 1 | 0 |
| slices/web_session/acp_drain.rs:267 | fn `map_request_finished` | 42 | 1 | 1 |
| slices/web_session/task_session_exit.rs:518 | fn `dispatch_queued_prompt` | 41 | 1 | 0 |
| slices/session_models.rs:249 | fn `parse_agent_models_output` | 38 | 1 | 0 |
| slices/web_session/output_content.rs:181 | fn `from_output_block` | 37 | 2 | 0 |
| adapters/session_shim/log.rs:111 | fn `read_lines_from` | 36 | 2 | 0 |
| adapters/web_session_acp/rpc_session.rs:382 | fn `carry_tool_inputs` | 36 | 1 | 1 |
| runtime/state.rs:426 | fn `report_or_defer_task_session_activity` | 36 | 1 | 1 |
| runtime/task_routes/live.rs:541 | fn `gate_rejection_response` | 36 | 2 | 0 |
| slices/web_session/task_session_replacement.rs:51 | fn `finalize_client_metadata` | 35 | 2 | 0 |
| adapters/session_shim/spawn.rs:165 | fn `connect_to_child` | 33 | 1 | 1 |
| adapters/terminal_pty/bridge.rs:41 | fn `handle_input_frame` | 33 | 1 | 1 |
| slices/web_session/acp_usage.rs:28 | fn `parse_turn_usage` | 33 | 1 | 1 |
| slices/web_session/task_session_spawn.rs:245 | fn `respawn` | 33 | 2 | 6 |
| slices/web_session/process_identity.rs:84 | fn `terminate_if_same` | 30 | 0 | 1 |
| adapters/stt_provider/mod.rs:136 | fn `parse_sidecar_event_line` | 29 | 1 | 1 |
| adapters/terminal_pty/bridge.rs:75 | fn `process_client_frame` | 29 | 4 | 1 |
| slices/push.rs:220 | fn `flush_if_dirty` | 29 | 1 | 0 |
| adapters/terminal_pty/bridge.rs:105 | fn `filter_scrollback_hostile_sequences` | 28 | 1 | 1 |
| runtime/headless.rs:19 | fn `targets_for` | 26 | 1 | 0 |
| slices/web_session/prompt_content.rs:190 | fn `validate_prompt_content` | 26 | 1 | 1 |
| adapters/server.rs:240 | fn `resolve_test_in_stable_config` | 25 | 1 | 0 |
| slices/web_session/process_tree.rs:50 | fn `background_shells` | 24 | 1 | 1 |
| adapters/web_session_acp/catalog.rs:142 | fn `catalog_from_neutral` | 23 | 2 | 1 |
| adapters/server.rs:29 | fn `restart_launch_from_env` | 22 | 1 | 0 |
| adapters/web_session_acp/pi_rpc_client.rs:76 | fn `builtin_command` | 22 | 1 | 1 |
| slices/web_session/replay.rs:17 | fn `plan_replay` | 21 | 1 | 1 |
| slices/web_session/task_session_directory.rs:458 | fn `drop_session` | 21 | 2 | 8 |
| runtime/state.rs:326 | fn `report_task_session_activity` | 20 | 1 | 4 |
| slices/web_session/acp_usage.rs:62 | fn `turn_usage_dedup_key` | 20 | 1 | 1 |
| adapters/session_shim/spawn.rs:114 | fn `retire_live_shim` | 19 | 1 | 1 |
| adapters/web_session_acp/cursor_sdk_spawn.rs:96 | fn `spawn_error` | 19 | 1 | 2 |
| runtime/task_routes/cockpit.rs:356 | fn `axum_task_detail` | 19 | 1 | 0 |
| slices/session_models.rs:46 | fn `harness_version` | 19 | 10 | 0 |
| slices/web_session/prompt_content.rs:170 | fn `reject_disallowed_wire_blocks` | 19 | 1 | 1 |
| adapters/web_session_acp/catalog.rs:93 | fn `read_cursor_sdk_catalog` | 18 | 1 | 0 |
| adapters/web_session_acp/catalog.rs:199 | fn `read_claude_sdk_catalog` | 18 | 1 | 0 |
| adapters/web_session_acp/rpc_session.rs:363 | fn `fold_pending_error` | 18 | 1 | 1 |
| slices/web_session/task_session_exit.rs:43 | fn `persist_ledger_update` | 18 | 7 | 0 |
| adapters/assets.rs:26 | fn `shell_version_from_assets` | 17 | 1 | 0 |
| adapters/terminal_pty/attach.rs:72 | fn `cleanup_spawned_child_async_with_timeout` | 17 | 1 | 1 |
| adapters/web_session_acp/catalog.rs:124 | fn `read_pi_rpc_catalog` | 17 | 1 | 0 |
| runtime/bridge.rs:196 | fn `unsupported_operate_action` | 17 | 1 | 0 |
| adapters/web_session_acp/client.rs:166 | fn `restore_handshake_timeout` | 16 | 1 | 0 |
| adapters/web_session_acp/session_client.rs:770 | fn `prompt_parts` | 16 | 3 | 0 |
| slices/operate/mod.rs:611 | fn `format_execution_outputs` | 16 | 2 | 1 |
| slices/web_session/acp_drain.rs:20 | fn `classify_prompt_terminal` | 16 | 1 | 1 |
| slices/web_session/background_jobs.rs:64 | fn `record_poll` | 16 | 1 | 1 |
| slices/web_session/process_tree.rs:33 | fn `cached_table` | 16 | 1 | 1 |
| adapters/terminal_pty/bridge.rs:232 | fn `probe_failure_detail` | 15 | 1 | 1 |
| adapters/web_session_acp/config_options.rs:81 | fn `model_config_boolean_option` | 15 | 8 | 2 |
| slices/web_session/mod.rs:352 | fn `dispatch_prompt` | 15 | 0 | 1 |
| adapters/web_session_acp/config_options.rs:38 | fn `find_option_by_category` | 14 | 3 | 0 |
| slices/web_session/acp_drain.rs:229 | fn `map_acp_session_update_with_startup` | 14 | 1 | 1 |
| slices/web_session/task_session_exit.rs:560 | fn `begin_failure_events` | 14 | 1 | 1 |
| adapters/stt_provider/mod.rs:84 | fn `encode_sidecar_audio_frame` | 13 | 1 | 1 |
| adapters/terminal_pty/attach.rs:157 | fn `ephemeral_client_token` | 13 | 1 | 1 |
| adapters/terminal_pty/bridge.rs:142 | fn `captured_history_frame_bytes` | 13 | 1 | 1 |
| adapters/web_session_acp/config_options.rs:57 | fn `thought_level_option` | 13 | 5 | 1 |
| adapters/web_session_acp/config_options.rs:97 | fn `read_fast_current_value` | 13 | 6 | 2 |
| adapters/web_session_acp/sdk_connection.rs:148 | fn `client_capabilities` | 13 | 2 | 1 |
| slices/web_session/acp_usage.rs:100 | fn `turn_usage_event` | 13 | 1 | 1 |
| adapters/server.rs:52 | fn `restart_launch_for_supervisor` | 12 | 1 | 0 |
| adapters/skills.rs:18 | fn `skill_search_roots` | 12 | 1 | 0 |
| adapters/terminal_pty/attach.rs:255 | fn `is_ephemeral_session_name` | 12 | 2 | 0 |
| adapters/tls.rs:83 | fn `write_private` | 12 | 1 | 0 |
| adapters/web_session_acp/sdk_connection.rs:592 | fn `preferred_permission_config` | 12 | 1 | 1 |
| adapters/web_session_acp/sdk_elicitation.rs:136 | fn `wire_content_from_json` | 12 | 1 | 0 |
| runtime/mod.rs:235 | struct `TlsListener` | 12 | 2 | 2 |
| slices/dev_deploy/mod.rs:160 | fn `set_ready` | 12 | 1 | 1 |
| slices/web_session/normalize.rs:164 | fn `is_stream_continuation` | 12 | 1 | 1 |
| adapters/session_shim/spawn.rs:102 | fn `start_shim` | 11 | 1 | 1 |
| adapters/terminal_pty/attach.rs:235 | fn `random_session_token` | 11 | 2 | 0 |
| adapters/terminal_pty/attach.rs:276 | fn `ephemeral_sessions_to_reap_detached` | 11 | 1 | 1 |
| slices/actions.rs:16 | fn `web_action` | 11 | 1 | 0 |
| slices/web_session/startup_recovery.rs:151 | fn `persisted_chat_running` | 11 | 2 | 0 |
| slices/web_session/ws_bridge.rs:43 | fn `outbound_frames` | 11 | 1 | 1 |
| adapters/server.rs:133 | fn `web_profile_from_sources` | 10 | 3 | 0 |
| adapters/terminal_pty/bridge.rs:216 | fn `send_error_and_close` | 10 | 6 | 0 |
| adapters/web_session_store/prompt_ledger/kernel.rs:14 | fn `entry_content_json` | 10 | 3 | 1 |
| slices/dev_deploy/mod.rs:473 | fn `test_in_dev_command_args` | 10 | 1 | 1 |
| slices/session_models.rs:28 | struct `SessionModelsResponse` | 10 | 7 | 0 |
| slices/stt.rs:99 | fn `encode_audio_frame` | 10 | 0 | 0 |
| slices/web_session/orphan_sweep.rs:44 | fn `sweep_adopting` | 10 | 1 | 0 |
| slices/web_session/ws_bridge.rs:26 | fn `resume_cursor_after_early` | 10 | 1 | 0 |
| adapters/terminal_pty/bridge.rs:184 | fn `isolated_plan_for_bridge` | 9 | 1 | 1 |
| adapters/web_session_acp/cursor_sdk_spawn.rs:14 | fn `node_version_ok` | 9 | 1 | 1 |
| adapters/web_session_acp/jsonl_process.rs:76 | fn `from_parts` | 9 | 2 | 0 |
| adapters/web_session_store/mod.rs:249 | fn `try_append_events` | 9 | 0 | 2 |
| runtime/mod.rs:56 | fn `api_access_policy` | 9 | 1 | 2 |
| runtime/task_routes/cockpit.rs:31 | fn `request_marks_foreground_presence` | 9 | 1 | 0 |
| slices/actions.rs:39 | fn `remediation_action_state` | 9 | 1 | 0 |
| slices/actions.rs:93 | fn `supported_browser_action` | 9 | 0 | 0 |
| slices/dev_deploy/mod.rs:361 | fn `restart_script_from_env` | 9 | 1 | 0 |
| adapters/server.rs:163 | fn `test_in_stable_script_args` | 8 | 1 | 0 |
| adapters/server.rs:231 | struct `TestInStableResolveInput` | 8 | 2 | 0 |
| adapters/terminal_pty/bridge.rs:198 | fn `remaining_resize_wait` | 8 | 1 | 1 |
| adapters/terminal_pty/bridge.rs:207 | fn `resize_settle_deadline` | 8 | 3 | 1 |
| adapters/terminal_pty/bridge.rs:248 | fn `run_tmux_command` | 8 | 6 | 1 |
| runtime/state.rs:961 | fn `store_completed_response` | 8 | 1 | 1 |
| slices/dev_deploy/mod.rs:55 | struct `DevDeploySource` | 8 | 5 | 1 |
| slices/diff_review.rs:14 | struct `PullRequestDto` | 8 | 4 | 0 |
| slices/diff_review.rs:30 | struct `DiffFileDto` | 8 | 3 | 0 |
| slices/web_session/task_session_directory.rs:106 | fn `live_activities` | 8 | 1 | 1 |
| adapters/terminal_pty/attach.rs:101 | fn `build_tmux_attach_command_plan` | 7 | 1 | 1 |
| adapters/terminal_pty/attach.rs:268 | fn `ephemeral_sessions_to_reap` | 7 | 1 | 1 |
| adapters/terminal_pty/bridge.rs:134 | fn `output_frame_bytes` | 7 | 3 | 1 |
| adapters/web_session_acp/config_options.rs:125 | fn `read_select_current_value` | 7 | 4 | 0 |
| adapters/web_session_acp/config_options.rs:159 | fn `option_value_advertised` | 7 | 8 | 0 |
| runtime/state.rs:66 | fn `moonshine_provider_from_config` | 7 | 2 | 0 |
| runtime/state.rs:910 | fn `completed_response` | 7 | 1 | 1 |
| runtime/state.rs:95 | struct `WebSharedState` | 7 | 6 | 1 |
| slices/diff_review.rs:61 | struct `TaskDiffDto` | 7 | 3 | 0 |
| slices/web_session/acp_usage.rs:20 | struct `NormalizedTurnUsage` | 7 | 5 | 1 |
| slices/web_session/normalize.rs:177 | fn `merge_stream_text` | 7 | 1 | 1 |
| slices/web_session/process_identity.rs:50 | enum `Termination` | 7 | 10 | 1 |
| slices/web_session/transcript.rs:202 | fn `is_superseded_marker` | 7 | 3 | 1 |
| adapters/server.rs:144 | fn `web_profile_from_env` | 6 | 0 | 0 |
| adapters/server.rs:156 | fn `test_in_stable_script` | 6 | 2 | 0 |
| adapters/web_session_acp/catalog.rs:7 | struct `AgentOptionGroup` | 6 | 5 | 0 |
| adapters/web_session_acp/config_options.rs:133 | fn `read_boolean_current_value` | 6 | 1 | 0 |
| adapters/web_session_acp/session_client.rs:45 | struct `PiSession` | 6 | 2 | 0 |
| adapters/web_session_acp/session_client.rs:52 | struct `ClaudeSession` | 6 | 2 | 0 |
| adapters/web_session_acp/session_client.rs:59 | struct `CursorSession` | 6 | 2 | 0 |
| adapters/web_session_store/prompt_ledger.rs:443 | fn `ledger_path` | 6 | 4 | 3 |
| adapters/web_session_store/prompt_ledger.rs:13 | enum `LedgerLoadError` | 6 | 22 | 1 |
| slices/agent_auth.rs:60 | struct `HarnessAuthStatus` | 6 | 8 | 0 |
| slices/dev_deploy/mod.rs:379 | fn `spawn_test_in_dev` | 6 | 0 | 0 |
| slices/web_session/acp_usage.rs:11 | fn `should_emit` | 6 | 1 | 1 |
| slices/web_session/ws_bridge.rs:37 | fn `should_skip_replayed_envelopes` | 6 | 2 | 1 |
| adapters/http.rs:130 | fn `apply_no_store` | 5 | 1 | 0 |
| adapters/server.rs:127 | fn `flag_value_from_args` | 5 | 3 | 0 |
| adapters/server.rs:225 | struct `TestInStableConfig` | 5 | 3 | 0 |
| adapters/server.rs:23 | enum `RestartLaunch` | 5 | 11 | 0 |
| adapters/session_shim/mod.rs:21 | struct `ShimPaths` | 5 | 2 | 0 |
| adapters/terminal_pty/bridge.rs:28 | enum `TextFrameOutcome` | 5 | 8 | 1 |
| adapters/terminal_pty/bridge.rs:35 | enum `FrameOutcome` | 5 | 21 | 1 |
| adapters/web_session_acp/sdk_elicitation.rs:149 | fn `accept_action` | 5 | 1 | 0 |
| runtime/mod.rs:316 | fn `browser_session_json_response` | 5 | 1 | 2 |
| slices/diff_review.rs:47 | struct `DiffFlagDto` | 5 | 3 | 0 |
| slices/diff_review.rs:54 | struct `DiffJudgmentDto` | 5 | 3 | 0 |
| slices/web_session/process_tree.rs:10 | struct `ProcessEntry` | 5 | 10 | 1 |
| slices/web_session/session_evidence.rs:68 | fn `note_activity_report_failure` | 5 | 2 | 0 |
| slices/web_session/startup_recovery.rs:16 | fn `interrupted_by_restart_event` | 5 | 1 | 0 |
| adapters/server.rs:151 | fn `test_in_stable_enabled` | 4 | 1 | 0 |
| adapters/session_shim/relay.rs:9 | enum `RelayError` | 4 | 4 | 0 |
| adapters/stt_provider/mod.rs:30 | enum `ProviderHealth` | 4 | 5 | 1 |
| adapters/terminal_pty/attach.rs:63 | fn `cleanup_spawned_child` | 4 | 1 | 1 |
| adapters/terminal_pty/attach.rs:171 | fn `kill_session_command` | 4 | 3 | 0 |
| adapters/terminal_pty/attach.rs:91 | struct `TmuxAttachCommandPlan` | 4 | 4 | 0 |
| adapters/terminal_pty/attach.rs:48 | trait `TerminalChild` | 4 | 4 | 1 |
| adapters/terminal_pty/bridge.rs:227 | fn `base_session_probe_command` | 4 | 1 | 1 |
| adapters/tls.rs:9 | struct `TlsIdentity` | 4 | 5 | 0 |
| runtime/mod.rs:51 | enum `ApiAccess` | 4 | 5 | 2 |
| slices/dev_deploy/mod.rs:155 | fn `set_restarting` | 4 | 1 | 1 |
| slices/dev_deploy/mod.rs:173 | fn `set_failed` | 4 | 3 | 1 |
| slices/diff_review.rs:24 | struct `DiffHunkDto` | 4 | 3 | 0 |
| slices/diff_review.rs:167 | struct `PullRequestListProjection` | 4 | 2 | 0 |
| slices/diff_review.rs:173 | struct `TaskDiffRouteProjection` | 4 | 2 | 0 |
| slices/push.rs:54 | struct `PushSubscriptionKeys` | 4 | 1 | 1 |
| slices/web_session/orphan_sweep.rs:39 | struct `SweepResult` | 4 | 3 | 0 |
| slices/web_session/replay.rs:12 | struct `ReplayPlan` | 4 | 4 | 0 |
| slices/web_session/startup_recovery.rs:172 | struct `QueuedScan` | 4 | 2 | 0 |
| slices/web_session/task_session_directory.rs:642 | fn `clear_context` | 4 | 1 | 2 |
| adapters/browser_session.rs:52 | fn `cookie_pair` | 3 | 2 | 1 |
| adapters/stt_provider/mod.rs:281 | fn `worker_spawns` | 3 | 5 | 1 |
| adapters/terminal_pty/attach.rs:176 | fn `destroy_ephemeral_session_commands` | 3 | 1 | 1 |
| adapters/terminal_pty/attach.rs:307 | fn `reap_detached_ephemeral_terminal_sessions` | 3 | 0 | 0 |
| adapters/terminal_pty/bridge.rs:194 | fn `should_wait_reflow_before_seed` | 3 | 1 | 1 |
| adapters/web_session_acp/config_options.rs:167 | fn `option_boolean_advertised` | 3 | 1 | 0 |
| adapters/web_session_acp/session_client.rs:145 | fn `from_acp` | 3 | 1 | 0 |
| adapters/web_session_acp/session_client.rs:149 | fn `from_pi` | 3 | 1 | 2 |
| adapters/web_session_acp/session_client.rs:153 | fn `from_claude` | 3 | 1 | 0 |
| adapters/web_session_acp/session_client.rs:157 | fn `from_cursor` | 3 | 1 | 1 |
| adapters/web_session_store/prompt_ledger.rs:85 | fn `owns_prompt` | 3 | 0 | 1 |
| runtime/mod.rs:148 | fn `log_web_listening` | 3 | 1 | 2 |
| runtime/state.rs:918 | fn `has_in_flight_mutation` | 3 | 1 | 2 |
| runtime/task_routes/cockpit.rs:42 | struct `PresenceRequest` | 3 | 1 | 0 |
| slices/dev_deploy/mod.rs:39 | fn `is_active` | 3 | 2 | 0 |
| slices/training/runner.rs:32 | fn `with_host` | 3 | 0 | 0 |
| slices/web_session/mod.rs:368 | fn `clear_prompt_queue` | 3 | 1 | 1 |
| slices/web_session/session_activity.rs:154 | fn `turn_running` | 3 | 2 | 0 |
| slices/web_session/session_error.rs:29 | fn `restore_unavailable` | 3 | 1 | 0 |
| slices/web_session/task_session_directory.rs:836 | fn `has_live_entry` | 3 | 3 | 5 |
| slices/web_session/ws_bridge.rs:18 | fn `should_send_keepalive` | 3 | 1 | 1 |
| slices/web_session/mod.rs:350 | struct `PromptQueueFull` | 1 | 2 | 1 |
| slices/web_session/task_session.rs:135 | struct `HolderCount` | 1 | 3 | 0 |
No local token and no outside production token:
crates/ajax-web/src/adapters/server.rs:144 fn web_profile_from_env (6 lines)
crates/ajax-web/src/adapters/terminal_pty/attach.rs:307 fn reap_detached_ephemeral_terminal_sessions (3 lines)
crates/ajax-web/src/slices/training/runner.rs:32 fn with_host (3 lines)
crates/ajax-web/src/slices/actions.rs:93 fn supported_browser_action (9 lines)
crates/ajax-web/src/adapters/web_session_store/mod.rs:249 fn try_append_events (9 lines)
crates/ajax-web/src/adapters/web_session_store/prompt_ledger.rs:85 fn owns_prompt (3 lines)
crates/ajax-web/src/slices/web_session/process_identity.rs:84 fn terminate_if_same (30 lines)
crates/ajax-web/src/slices/stt.rs:99 fn encode_audio_frame (10 lines)
crates/ajax-web/src/slices/dev_deploy/mod.rs:379 fn spawn_test_in_dev (6 lines)
crates/ajax-web/src/slices/web_session/mod.rs:352 fn dispatch_prompt (15 lines)


### B-02 Retire legacy JSONL / JSON ledger import after an explicit upgrade cutoff

- Paths: `crates/ajax-web/src/adapters/web_session_store/legacy.rs`, `mod.rs`, `prompt_ledger.rs`, and import-specific tests in `tests.rs` and `prompt_ledger/tests.rs`.
- Prod lines removed: **236**, a conservative exact excision budget: all 88 lines of `legacy.rs`; `mod.rs:61–118` (58) plus `mod legacy;` (1); `prompt_ledger.rs` constants 9–10 (2), `DiskLedger` 70–76 (7), `read_legacy` 212–232 (21), import branch 239–262 (24), `delete_legacy_file` 274–280 (7), `migrate_legacy_file` 282–309 (28). Sum 88+58+1+2+7+21+24+7+28 = 236. Retain the database read paths and legacy-file deletion/listing so Drop can still remove old sensitive data. Remaining error-variant/import cleanup is not credited. `wc -l` file baselines: legacy 88, mod 518, prompt_ledger 631.
- Test lines removed: **188 function-body lines**, excluding attributes, blank separators and shared helpers. AST ranges in store `tests.rs`: `torn_final_line_is_skipped_on_import` 73–89 (17), `issue_1228_unreadable_file_is_an_error_not_an_empty_session` 214–230 (17), `issue_1240_truncated_final_row_is_not_corruption` 257–270 (14), `import_is_not_repeated_when_the_run_already_exists` 294–314 (21), `migrated_transcript_copy_is_owner_only` 416–439 (24); ledger `tests.rs`: `load_rejects_malformed_ledger` 201–209 (9), `load_rejects_unsupported_version` 212–223 (12), `load_rejects_wrong_kind` 226–237 (12), `legacy_ledger_is_imported_once_and_removed` 240–269 (30), `legacy_ledger_seen_after_import_is_renamed_not_imported` 272–303 (32). These all construct legacy files; no whole test file is removable. Keep `issue_1240_interior_corrupt_rows_are_counted_and_survive_rewrite` because persistent corruption reporting still matters; adapt its fixture in a future approved change.
- Evidence: `rg -n 'legacy::read|read_legacy|delete_legacy_file|migrate_legacy_file' crates/ajax-web/src/adapters/web_session_store` finds the JSONL import at `mod.rs:64`, ledger import at `prompt_ledger.rs:239`, and the rename/delete calls at 255/257/260. `load_run` checks the old file on every read; `try_load` checks the old ledger before reading kernel rows. `docs/architecture/durable-execution.md:206–219` documents one-time migration, after which the DB is the only source. This is reachable compatibility code, **not dead code**.
- What replaces it: normal `latest_run_for_task` and `prompts_for_run` reads already following the import branches. Require users with pre-kernel files to pass through an existing migration-capable release first. No new migration framework is proposed or counted.
- Behavior or public contract change: **PUBLIC** — removes documented automatic import of `web-session/*.jsonl` and `*.prompt-ledger.json`. Old-only sessions would otherwise appear empty and queued prompts would not load. Must not ship without an approved version cutoff and upgrade path; no evidence about the installed population was available.
- Risk: high for upgrades, low for already-migrated DB-backed sessions. Preserve legacy cleanup, current DB corruption detection, atomic commits and owner fences.
- Depends on: an approved migration-support cutoff, not B-01. B-03's ledger-state redesign is separate and must not recount these ranges.
- Confidence: likely. The code and counts are exact; eligibility for removal depends on product support policy.


### B-04 Remove the superseded queue helper and unused PID-termination API family

- Paths: `crates/ajax-web/src/slices/web_session/{mod,tests,process_identity,process_identity_tests}.rs`.
- Prod lines removed: **80**, inclusive AST spans: `process_identity.rs` `Termination` 50–56 (7), `send_signal` 58–68 (11), `wait_no_longer_same` 70–81 (12), `terminate_if_same` 84–113 (30); `web_session/mod.rs` `PromptDispatch` 335–338 (4), `PromptQueueFull` 350 (1), `dispatch_prompt` 352–366 (15). Attributes/import cleanup is uncredited. File `wc -l` totals: 113, 125, 523, 815 respectively for process implementation/tests and session mod/tests.
- Test lines removed: **86**, only the dead APIs' tests/helpers: process tests `kill_and_reap` 17–25 (9), `terminates_the_matching_process` 68–79 (12), `never_signals_a_process_with_a_different_start_time` 82–98 (17), `already_gone_is_reported` 101–110 (10), `refuses_pid_one_and_own_pid` 113–119 (7); queue tests `dispatch_prompt_starts_when_idle` 700–707 (8), `dispatch_prompt_queues_when_in_flight` 710–718 (9), `dispatch_prompt_rejects_when_queue_full` 721–734 (14). Keep process identity/reuse judgment, live start-token tests, queued_prompt fixture, cancel tests and all live queue reliability tests. Family total **166**, above the 150-line cutoff.
- Evidence: `rg -n '\b(PromptDispatch|PromptQueueFull|dispatch_prompt|terminate_if_same|Termination)\b' crates scripts` finds `dispatch_prompt` calls only at session `tests.rs:703,713,726`; termination calls only at `process_identity_tests.rs:72,86,109,114,116`. The core `Operation::PromptDispatch` hits are a DIFFERENT enum variant and stay. `rg -n 'terminate_if_same|dispatch_prompt|PromptQueueFull' docs architecture.md README.md` returns no matches. The full AST screen independently flags the two functions as having zero external production references and zero own-file references. Private signal/wait helpers are called only from the dead termination function.
- What replaces it: nothing. Production prompt admission uses `task_session_exit::submit_prompt` (359 onward), persists ownership and enforces the actual queue cap at 428; production PID identity checking uses `foreign_live_child`, which stays. Removing an unused termination routine does not remove the live process group/shim teardown paths.
- Behavior or public contract change: no repository runtime behavior. **PUBLIC Rust API caveat**: the queue helper/types are reachable through public `ajax_web::slices::web_session`; no CLI, HTTP, config or documented contract removal found, but external crate consumers cannot be disproved by repository searches. Termination is pub(super), private to the slice.
- Risk: low internally; check library API support policy before deleting exported queue symbols.
- Depends on: none. Do not count `QueuedPrompt` or its tests here; they are still live and belong to B-03's separate queue consolidation.
- Confidence: proven for repository unreachability and exclusive tests; external Rust consumers unknown.


## B-01 measurement and concrete retained design

```python
from pathlib import Path
import subprocess,re,difflib,itertools
base=Path('crates/ajax-web/src/adapters/web_session_acp')
groups=[['claude_sdk_client.rs','cursor_sdk_client.rs','pi_rpc_client.rs'],
        ['rpc_handshake.rs','pi_rpc_handshake.rs'],
        ['claude_sdk_map.rs','cursor_sdk_map.rs','pi_rpc_map.rs'],
        ['../../../sidecar/claude_sdk_sidecar.mjs','../../../sidecar/cursor_sdk_sidecar.mjs']]
def norm(t):
    t=re.sub(r'ClaudeSdkClient|CursorSdkClient|PiRpcClient','DialectClient',t)
    t=re.sub(r'claude_sdk|cursor_sdk|pi_rpc','dialect',t)
    return re.sub(r'\s+',' ',t).strip()
def prod_lines(p):
    t=p.read_text().splitlines()
    cutoff=next((i for i,s in enumerate(t) if s=='#[cfg(test)]' and i+1<len(t)
                 and t[i+1].startswith('mod ')),len(t))
    return t[:cutoff]
for names in groups:
    for n in names:
        assert subprocess.run(['git','ls-files','--error-unmatch',str(base/n)],capture_output=True).returncode==0
        print(subprocess.check_output(['wc','-l',str(base/n)],text=True).strip(),
              'production-prefix',len(prod_lines(base/n)))
    unique=0
    for j,b in enumerate(names):
        covered=set()
        for a in names[:j]:
            aa=[norm(s) for s in prod_lines(base/a)]; bb=[norm(s) for s in prod_lines(base/b)]
            blocks=[m for m in difflib.SequenceMatcher(None,aa,bb,autojunk=False).get_matching_blocks()
                    if m.size>=3 and sum(bool(re.search(r'[A-Za-z0-9]',s)) for s in aa[m.a:m.a+m.size])>=2]
            print(a,b,'matched lines',sum(m.size for m in blocks),
                  'blocks (1-based A,B,length)',[(m.a+1,m.b+1,m.size) for m in blocks])
            for m in blocks: covered.update(range(m.b,m.b+m.size))
        unique+=len(covered)
    print('Later-file matched positions, union against all earlier files:',unique)
```

```text
321 crates/ajax-web/src/adapters/web_session_acp/claude_sdk_client.rs production-prefix 299
453 crates/ajax-web/src/adapters/web_session_acp/cursor_sdk_client.rs production-prefix 435
394 crates/ajax-web/src/adapters/web_session_acp/pi_rpc_client.rs production-prefix 353
claude_sdk_client.rs cursor_sdk_client.rs matched lines 115 blocks (1-based A,B,length) [(1, 1, 6), (12, 12, 11), (27, 23, 3), (40, 119, 3), (56, 142, 8), (70, 162, 5), (77, 171, 3), (81, 175, 7), (91, 185, 3), (97, 191, 4), (105, 196, 3), (118, 199, 6), (147, 229, 14), (252, 397, 22), (283, 419, 17)]
claude_sdk_client.rs pi_rpc_client.rs matched lines 96 blocks (1-based A,B,length) [(1, 1, 8), (18, 14, 3), (120, 156, 5), (127, 164, 4), (147, 176, 14), (172, 214, 8), (190, 239, 4), (196, 245, 3), (203, 257, 4), (222, 276, 3), (251, 301, 12), (263, 318, 11), (283, 329, 8), (291, 345, 9)]
cursor_sdk_client.rs pi_rpc_client.rs matched lines 79 blocks (1-based A,B,length) [(1, 1, 6), (18, 14, 3), (123, 97, 4), (138, 111, 6), (201, 156, 4), (229, 176, 14), (252, 276, 3), (397, 302, 11), (408, 318, 19), (427, 345, 9)]
Later-file matched positions, union against all earlier files: 221
136 crates/ajax-web/src/adapters/web_session_acp/rpc_handshake.rs production-prefix 136
222 crates/ajax-web/src/adapters/web_session_acp/pi_rpc_handshake.rs production-prefix 222
rpc_handshake.rs pi_rpc_handshake.rs matched lines 0 blocks (1-based A,B,length) []
Later-file matched positions, union against all earlier files: 0
322 crates/ajax-web/src/adapters/web_session_acp/claude_sdk_map.rs production-prefix 322
188 crates/ajax-web/src/adapters/web_session_acp/cursor_sdk_map.rs production-prefix 188
336 crates/ajax-web/src/adapters/web_session_acp/pi_rpc_map.rs production-prefix 336
claude_sdk_map.rs cursor_sdk_map.rs matched lines 32 blocks (1-based A,B,length) [(1, 3, 5), (28, 22, 3), (54, 25, 9), (133, 35, 8), (198, 159, 7)]
claude_sdk_map.rs pi_rpc_map.rs matched lines 14 blocks (1-based A,B,length) [(2, 2, 4), (56, 49, 3), (266, 279, 7)]
cursor_sdk_map.rs pi_rpc_map.rs matched lines 7 blocks (1-based A,B,length) [(4, 2, 4), (27, 49, 3)]
Later-file matched positions, union against all earlier files: 46
598 crates/ajax-web/src/adapters/web_session_acp/../../../sidecar/claude_sdk_sidecar.mjs production-prefix 598
377 crates/ajax-web/src/adapters/web_session_acp/../../../sidecar/cursor_sdk_sidecar.mjs production-prefix 377
../../../sidecar/claude_sdk_sidecar.mjs ../../../sidecar/cursor_sdk_sidecar.mjs matched lines 140 blocks (1-based A,B,length) [(4, 10, 4), (9, 14, 13), (23, 28, 14), (76, 42, 7), (101, 62, 3), (106, 66, 4), (343, 141, 7), (393, 190, 7), (434, 244, 4), (458, 259, 4), (482, 277, 10), (499, 296, 7), (511, 310, 7), (537, 317, 5), (546, 324, 4), (559, 338, 40)]
Later-file matched positions, union against all earlier files: 140
```

The smallest single-dialect client design is already largely present: `RpcClientCore` owns lifecycle/event transport, and `RpcSession` takes a mapper plus the Pi image-tag switch. Retain that common engine; collapse redundant public forwarding façades onto `core()` instead of inventing a new trait. Keep the dialect-specific constructors/configuration state as branches/modules behind it. This is a specific bounded consolidation, not a claim that 1,000 lines of dialect semantics disappear.

| Component | Current production lines | Retain | Removed |
|---|---:|---:|---:|
| Claude client (before inline tests) | 299 | 262 | 37 |
| Cursor client (before inline tests) | 435 | 398 | 37 |
| Pi client (before inline tests) | 353 | 323 | 30 |
| Both handshake files | 358 | 358 | 0 |
| All three map files | 846 | 846 | 0 |
| `rpc_client.rs` + `rpc_session.rs` | 801 | 801 | 0 |
| Rust family subtotal, before sidecar embedding glue | 3,092 | **2,988** | **104** |
| Claude + Cursor sidecars, including one retained shared framing copy | 975 | **912** | **63** |
| Replacement Rust source assembly | 0 | 8 | -8 |
| **Net** | **4,067** | **3,908** | **159** |

The 104 lines are exact AST spans. Each of the three clients loses the ten direct wrappers `child_pid`, `session_id`, `take_available_commands`, `begin_prompt`, `cancel`, `prompt_in_flight`, `poll_event`, `wait_event`, `shutdown`, `host_exited` (30 lines each); Claude and Cursor also lose their seven-line `begin_prompt_with_images` wrapper. Pi's image/compact/name method stays. Change callers to the existing `client.core().method(...)` without adding another wrapper. Keep `respond_dialog`/`respond_elicitation`, model validation, requested-versus-applied state, and all config/catalog code.

For sidecars, share exactly Claude ranges 9–19 (11), 25–35 (11), 76–79 (4), 484–487 (4), and 566–598 (33): write-drain counters/writeLine, respond/fail, flagValue, shutdown acknowledgment, and line decoder/stdin callbacks. Cursor's corresponding functions are 18–24, 30–40, 42–45, 279–282 and 345–377, plus its write-drain counters. These blocks are identical. One shared source fragment may be concatenated before each dialect by the Rust launch source; function declarations are hoisted and stdin callbacks execute after module initialization. Keep both imports in their dialect fragments and keep both distinct `dispatch` functions, SDK resolution/pumps/initialization, error messages, shutdown cancellation and timeout logic. Convert each one-line SIDECAR_SOURCE definition to a five-line `concat!(include_str!(common), "\n", include_str!(dialect))` form: eight added Rust lines. **912 is the measured retained sidecar source budget, not a compiled result.** Additional formatting beyond this explicit span model is not claimed as savings.

The direct-file sidecar test launchers must switch to the same assembled source: `claude_sidecar_tests.rs:11–28` currently spawns the Claude `.mjs` file; `sidecar/cursor_sdk_sidecar.test.mjs:10,24` currently spawns Cursor by path. Their assertions stay; no test savings are credited. Production SDK resolution overrides and embedded launch tests must continue covering real launch assembly. This makes the 159-line target **likely**, with exact duplicate/excision counts, not a proven compiled net patch. A larger single enum/client rewrite has no measured benefit here and is not budgeted.

Mapper differences checked by hand: Claude filters parent-tool events and translates `stream_event`, synthetic assistant errors, TodoWrite and elicitation; Cursor translates its own assistant/thinking/tool_call/result protocol and model variants; Pi folds `auto_retry_*`, compaction, extension dialogs and `agent_settled`. These are protocol semantics. The zero handshake match result agrees with code: SDK sidecars await one `init`; Pi sends four requests and tolerates absent optional responses after get_state. Remove neither mechanism wholesale.

