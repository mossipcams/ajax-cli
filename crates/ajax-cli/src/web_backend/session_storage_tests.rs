use super::*;
use ajax_core::registry::SqliteRegistryStore;

#[test]
fn chat_storage_preserves_cli_registry_path_and_defaults_under_state_dir() {
    let root = std::env::temp_dir().join(format!(
        "ajax-chat-storage-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let paths = CliContextPaths::new(root.join("config.toml"), root.join("custom-registry.db"));
    let storage = companion_session_storage(Some(&paths)).unwrap();
    assert_eq!(storage.state_dir, root);
    assert_eq!(storage.registry_db, paths.state_file);
    assert!(!paths.state_file.exists());

    let cli_store = SqliteRegistryStore::new(&paths.state_file);
    cli_store.save(&InMemoryRegistry::default()).unwrap();
    assert_eq!(storage.registry_store().current_revision().unwrap(), 1);
    assert!(!root.join("ajax.db").exists());

    let fallback = SessionStorage::under(root.clone());
    assert_eq!(fallback.state_dir, root);
    assert_eq!(fallback.registry_db, root.join("ajax.db"));
    assert_eq!(fallback.registry_store().current_revision().unwrap(), 0);
    assert!(root.join("ajax.db").is_file());
    std::fs::remove_dir_all(root).unwrap();
}
