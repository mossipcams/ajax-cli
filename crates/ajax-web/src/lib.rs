#![deny(unsafe_op_in_unsafe_fn)]

pub mod action_vocabulary;
pub mod adapters;
pub mod runtime;
pub mod slices;

#[cfg(test)]
mod architecture;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WebError {
    CommandFailed(String),
    JsonSerialization(String),
}

impl std::fmt::Display for WebError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CommandFailed(message) => write!(formatter, "{message}"),
            Self::JsonSerialization(message) => {
                write!(formatter, "json serialization failed: {message}")
            }
        }
    }
}

impl std::error::Error for WebError {}

#[cfg(test)]
mod tests {
    #[test]
    fn web_crate_declares_vertical_slice_layout() {
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let lib = std::fs::read_to_string(manifest_dir.join("src/lib.rs")).unwrap();
        let slices = std::fs::read_to_string(manifest_dir.join("src/slices/mod.rs")).unwrap();

        assert!(lib.contains("pub mod adapters;"));
        assert!(lib.contains("pub mod runtime;"));
        assert!(lib.contains("pub mod slices;"));
        for module in ["cockpit", "operate", "install", "attention"] {
            assert!(
                slices.contains(&format!("pub mod {module};")),
                "missing ajax-web vertical slice: {module}"
            );
        }
    }

    #[test]
    fn web_mechanisms_stay_out_of_slice_names() {
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let slices = std::fs::read_to_string(manifest_dir.join("src/slices/mod.rs")).unwrap();
        let adapters = std::fs::read_to_string(manifest_dir.join("src/adapters/mod.rs")).unwrap();

        for mechanism in ["http", "tls", "push", "assets", "server"] {
            assert!(
                !slices.contains(&format!("pub mod {mechanism};")),
                "mechanism must not be an ajax-web vertical slice: {mechanism}"
            );
        }

        for adapter in ["http", "tls", "push", "assets"] {
            assert!(
                adapters.contains(&format!("pub mod {adapter};")),
                "missing ajax-web adapter module: {adapter}"
            );
        }
    }
}
