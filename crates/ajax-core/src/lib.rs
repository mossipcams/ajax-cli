#![deny(unsafe_op_in_unsafe_fn)]

pub mod adapters;
pub mod attention;
pub mod commands;
pub mod config;
pub mod models;
pub mod output;
pub mod policy;
pub mod reconcile;
pub mod registry;

pub fn package_name() -> &'static str {
    "ajax-core"
}

#[cfg(test)]
mod tests {
    #[test]
    fn exposes_package_identity() {
        assert_eq!(super::package_name(), "ajax-core");
    }
}
