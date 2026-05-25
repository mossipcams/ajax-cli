#![deny(unsafe_op_in_unsafe_fn)]

pub mod adapters;
pub mod runtime;
pub mod slices;

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
    fn repo_file(path: &str) -> String {
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let repo_root = manifest_dir
            .parent()
            .and_then(std::path::Path::parent)
            .expect("ajax-web crate should live under crates/");
        std::fs::read_to_string(repo_root.join(path)).unwrap()
    }

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

    #[test]
    fn docker_image_contract_matches_mobile_web_runtime() {
        let dockerfile = repo_file("Dockerfile.ajax-web");
        let compose = repo_file("compose.ajax-web.yml");

        assert!(dockerfile.contains("ENV AJAX_WEB_SNAPSHOT_ONLY=1"));
        assert!(dockerfile.contains("VOLUME [\"/ajax-dev\"]"));
        assert!(dockerfile.contains("EXPOSE 8788"));
        assert!(
            dockerfile.contains("gosu"),
            "runtime image must include a privilege-drop tool"
        );
        assert!(
            !dockerfile.contains("\nUSER ajax"),
            "runtime image must start as root so mounted legacy volumes can be migrated"
        );
        assert!(
            dockerfile.contains("ENTRYPOINT [\"/usr/local/bin/ajax-web-entrypoint\"]"),
            "runtime image must use the Ajax web entrypoint"
        );
        assert!(
            dockerfile.contains("HEALTHCHECK"),
            "runtime image must declare a container healthcheck"
        );
        assert!(dockerfile.contains("/healthz"));

        let entrypoint = repo_file("docker/ajax-web-entrypoint.sh");
        assert!(entrypoint.contains("chown -R ajax:ajax /ajax-dev"));
        assert!(entrypoint.contains("exec gosu ajax \"$@\""));

        assert!(compose.contains("\"8788:8788\"") || compose.contains("- 8788:8788"));
        assert!(compose.contains("ajax-web-dev-home:/ajax-dev"));
        assert!(compose.contains("ajax-web-dev-home:"));
        assert!(
            !compose.contains("./:/ajax-dev"),
            "compose must not mount the source tree over Ajax state"
        );
    }

    #[test]
    fn docker_context_excludes_local_and_heavy_artifacts() {
        let dockerignore = repo_file(".dockerignore");

        for expected in [
            "target/",
            ".git/",
            "node_modules/",
            ".ajax-dev/",
            "ajax.db",
            "*.log",
        ] {
            assert!(
                dockerignore.lines().any(|line| line.trim() == expected),
                ".dockerignore must exclude {expected}"
            );
        }
    }
}
