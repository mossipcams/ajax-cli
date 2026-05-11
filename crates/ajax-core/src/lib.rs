#![deny(unsafe_op_in_unsafe_fn)]

pub mod adapters;
pub mod attention;
pub mod commands;
pub mod config;
pub mod events;
pub mod live;
pub mod models;
pub mod output;
pub mod policy;
pub mod reconcile;
pub mod registry;

#[cfg(test)]
mod tests {
    #[test]
    fn crate_root_does_not_keep_package_identity_wrapper() {
        let lib = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/lib.rs"),
        )
        .unwrap();
        let wrapper_name = ["package", "_name"].concat();

        assert!(!lib.contains(&wrapper_name));
    }

    #[test]
    fn avoids_duplicate_cockpit_snapshot_contract() {
        let lib = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/lib.rs"),
        )
        .unwrap();

        let duplicate_contract_export = ["pub mod ", "cockpit", ";"].concat();
        assert!(!lib.contains(&duplicate_contract_export));
    }
}
