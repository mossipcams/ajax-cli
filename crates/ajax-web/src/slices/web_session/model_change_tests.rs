use crate::adapters::web_session_acp::{
    with_test_acp_extra_args, with_test_acp_program, SessionConfigValue,
};
use crate::adapters::web_session_store;
use crate::slices::web_session::test_support::{
    fake_acp_fixture, scratch_dir, BlockingSessionDirectory,
};
use crate::slices::web_session::SessionServerEvent;
use ajax_core::models::AgentClient;

fn apply_model_change(label: &str, extra_args: &[&str]) -> (bool, Vec<SessionServerEvent>) {
    let dir = scratch_dir(label);
    let handle = "web/model-change-usage";
    let directory = BlockingSessionDirectory::new(dir.clone());
    let script = fake_acp_fixture();
    let applied = with_test_acp_program(&script, || {
        with_test_acp_extra_args(extra_args, || {
            directory
                .acquire(handle, &dir, "auto", AgentClient::Cursor)
                .expect("acquire");
            directory
                .apply_config_option(
                    handle,
                    "model",
                    SessionConfigValue::Select("grok-4.6".to_string()),
                )
                .is_ok()
        })
    });
    let stored = web_session_store::load::<SessionServerEvent>(&dir, handle);
    drop(directory);
    std::fs::remove_dir_all(dir).unwrap();
    (applied, stored.events)
}

#[test]
fn successful_model_change_appends_usage_reset() {
    let (applied, events) =
        apply_model_change("model-change-usage-ok", &["--cursor-parameterized-models"]);

    assert!(applied, "model change must succeed");
    assert!(events.contains(&SessionServerEvent::UsageReset));
}

#[test]
fn refused_model_change_appends_no_usage_reset() {
    let (applied, events) = apply_model_change(
        "model-change-usage-refused",
        &["--cursor-parameterized-models", "--model-refuse"],
    );

    assert!(!applied, "refused model change must fail");
    assert!(!events.contains(&SessionServerEvent::UsageReset));
}
