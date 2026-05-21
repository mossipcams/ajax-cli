use serde_json::Value;
use std::path::PathBuf;

const REQUIRED_LOCAL_GATES: &[&str] = &[
    "cargo fmt --check",
    "cargo check --all-targets --all-features",
    "cargo clippy --all-targets --all-features -- -D warnings",
    "cargo nextest run --all-features",
    "cargo test --doc",
    "npm run lint:duplication",
];

const REQUIRED_REMOTE_GATES: &[&str] = &[
    "npm ci",
    "npm run lint:duplication",
    "cargo fmt --check",
    "cargo check --all-targets --all-features",
    "RUSTFLAGS: -D warnings",
    "cargo check --no-default-features",
    "cargo check --locked",
    "cargo clippy --all-targets --all-features -- -D warnings",
    "cargo test --all-features",
    "cargo test --locked",
    "RUSTDOCFLAGS: -D warnings",
    "cargo doc --no-deps --all-features",
    "cargo audit",
];

const CI_REQUIRED_STATUS_NEEDS: &[&str] = &[
    "format-and-duplication",
    "check",
    "clippy",
    "test",
    "docs",
    "audit",
    "smoke",
];

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("ajax-cli crate should live under crates/")
        .parent()
        .expect("crates directory should live under workspace root")
        .to_path_buf()
}

#[test]
fn github_actions_exposes_ruleset_required_ci_status_check() {
    let root = workspace_root();
    let workflow_path = root.join(".github/workflows/ci.yml");
    let workflow = std::fs::read_to_string(&workflow_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", workflow_path.display()));

    assert!(
        workflow.contains("\n  ci:\n"),
        "CI workflow should include a job that publishes the required `CI` check:\n{workflow}"
    );
    assert!(
        workflow.contains("\n    name: CI\n"),
        "CI workflow should publish a job-level check named `CI` for repository rulesets:\n{workflow}"
    );
    for dependency in CI_REQUIRED_STATUS_NEEDS {
        assert!(
            workflow.contains(&format!("      - {dependency}")),
            "the required `CI` check should depend on `{dependency}`:\n{workflow}"
        );
    }
}

#[test]
fn husky_pre_commit_runs_full_local_validation_before_commit() {
    let root = workspace_root();
    let package_json_path = root.join("package.json");
    let package_json = std::fs::read_to_string(&package_json_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", package_json_path.display()));
    let manifest: Value =
        serde_json::from_str(&package_json).expect("package.json should be valid JSON");

    assert_eq!(manifest["scripts"]["test"], "npm run verify");
    let duplication_script = manifest["scripts"]["lint:duplication"]
        .as_str()
        .expect("package.json should define a scripts.lint:duplication string");
    assert!(
        duplication_script.contains("jscpd"),
        "scripts.lint:duplication should invoke jscpd:\n{duplication_script}"
    );
    let verify_script = manifest["scripts"]["verify"]
        .as_str()
        .expect("package.json should define a scripts.verify string");
    for gate in REQUIRED_LOCAL_GATES {
        assert!(
            verify_script.contains(gate),
            "scripts.verify should include `{gate}` in:\n{verify_script}"
        );
    }
    assert_eq!(manifest["scripts"]["prepare"], "husky");
    assert_eq!(manifest["devDependencies"]["husky"], "^9.1.7");

    let hook_path = root.join(".husky/pre-commit");
    let hook = std::fs::read_to_string(&hook_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", hook_path.display()));
    assert!(
        hook.lines().any(|line| line.trim() == "npm run verify"),
        ".husky/pre-commit should run npm run verify before commit:\n{hook}"
    );
}

#[test]
fn github_actions_runs_full_remote_validation_on_push_and_pull_request() {
    let root = workspace_root();
    let workflow_path = root.join(".github/workflows/ci.yml");
    let workflow = std::fs::read_to_string(&workflow_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", workflow_path.display()));

    assert!(
        workflow.contains("pull_request:"),
        "CI should run for pull requests:\n{workflow}"
    );
    assert!(
        workflow.contains("push:") && workflow.contains("- main"),
        "CI should run on pushes to main:\n{workflow}"
    );
    for gate in REQUIRED_REMOTE_GATES {
        assert!(
            workflow.contains(gate),
            "CI workflow should include `{gate}` in:\n{workflow}"
        );
    }
}

#[test]
fn jscpd_configuration_scans_project_sources_without_generated_outputs() {
    let root = workspace_root();
    let config_path = root.join(".jscpd.json");
    let config = std::fs::read_to_string(&config_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", config_path.display()));
    let config: Value = serde_json::from_str(&config).expect(".jscpd.json should be valid JSON");

    assert_eq!(config["threshold"], 0);
    assert_eq!(config["minLines"], 50);
    assert_eq!(config["reporters"], serde_json::json!(["console"]));
    assert_eq!(config["mode"], "strict");

    let paths = config["path"]
        .as_array()
        .expect(".jscpd.json should define a path array");
    for path in ["crates", "scripts", "docs", "README.md", "RELEASE.md"] {
        assert!(
            paths.iter().any(|entry| entry == path),
            ".jscpd.json should scan {path}: {paths:?}"
        );
    }

    let ignores = config["ignore"]
        .as_array()
        .expect(".jscpd.json should define an ignore array");
    for ignored in [
        "target/**",
        "node_modules/**",
        "Cargo.lock",
        "package-lock.json",
        "crates/ajax-core/proptest-regressions/**",
    ] {
        assert!(
            ignores.iter().any(|entry| entry == ignored),
            ".jscpd.json should ignore {ignored}: {ignores:?}"
        );
    }
}
