//! Static browser shell asset embedding and lookup mechanisms.

use std::sync::OnceLock;

pub struct StaticAsset {
    pub content_type: &'static str,
    pub body: &'static [u8],
}

pub fn browser_shell_html() -> String {
    let version = app_version();
    let mut html =
        include_str!("../../web/dist/index.html").replace("__AJAX_APP_VERSION__", version);
    // iOS PWA: launch splash is theme-color #161616, then the bare document
    // paints white until /app.css loads. Keep the first paint dark even when
    // CSS is slow or the radio blips.
    if !html.contains("ajax-boot-paint") {
        html = html.replacen(
            "<head>",
            "<head>\n  <style id=\"ajax-boot-paint\">html,body,#app{background:#161616;color:#e6e6e6;margin:0;min-height:100%}</style>",
            1,
        );
    }
    html
}

/// Fingerprint the embedded shell assets into the version string.
///
/// This keeps the runtime version stable within a build while still changing
/// whenever any shipped shell asset changes.
pub fn shell_version_from_assets(
    index_html: &[u8],
    app_js: &[u8],
    app_css: &[u8],
    terminal_js: &[u8],
) -> String {
    // FNV-1a: stable across toolchain versions (DefaultHasher is not).
    // Process all asset bytes sequentially for a single combined fingerprint.
    const FNV_OFFSET: u64 = 14695981039346656037;
    const FNV_PRIME: u64 = 1099511628211;
    let mut hash: u64 = FNV_OFFSET;
    for asset in [index_html, app_js, app_css, terminal_js] {
        for &byte in asset {
            hash ^= byte as u64;
            hash = hash.wrapping_mul(FNV_PRIME);
        }
    }
    format!("{}-{:016x}", env!("CARGO_PKG_VERSION"), hash)
}

/// Build identifier for the served browser shell.
///
/// Combines the crate version with a fingerprint of the embedded shell assets,
/// so the value changes on every release *and* on any edit to the HTML/JS/CSS
/// bundle. The mobile client polls `/api/version` and reloads when this differs
/// from the version it booted with.
pub fn app_version() -> &'static str {
    static VERSION: OnceLock<String> = OnceLock::new();
    VERSION.get_or_init(|| {
        shell_version_from_assets(
            include_bytes!("../../web/dist/index.html"),
            include_bytes!("../../web/dist/app.js"),
            include_bytes!("../../web/dist/app.css"),
            include_bytes!("../../web/dist/terminal.js"),
        )
    })
}

pub fn static_asset(path: &str) -> Option<StaticAsset> {
    match path {
        "/app.css" => Some(StaticAsset {
            content_type: "text/css; charset=utf-8",
            body: include_bytes!("../../web/dist/app.css"),
        }),
        "/app.js" => Some(StaticAsset {
            content_type: "text/javascript; charset=utf-8",
            body: include_bytes!("../../web/dist/app.js"),
        }),
        "/terminal.js" => Some(StaticAsset {
            content_type: "text/javascript; charset=utf-8",
            body: include_bytes!("../../web/dist/terminal.js"),
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{app_version, browser_shell_html, shell_version_from_assets, static_asset};

    #[test]
    fn assets_adapter_embeds_browser_shell() {
        assert!(browser_shell_html().contains("<!doctype html>"));
    }

    #[test]
    fn static_assets_are_raw_and_use_one_bare_module_graph() {
        let raw_app = include_bytes!("../../web/dist/app.js");
        let raw_terminal = include_bytes!("../../web/dist/terminal.js");

        let app = static_asset("/app.js").expect("app.js must exist");
        let terminal = static_asset("/terminal.js").expect("terminal.js must exist");

        assert_eq!(app.body, raw_app);
        assert_eq!(terminal.body, raw_terminal);

        for (path, body) in [("/app.js", app.body), ("/terminal.js", terminal.body)] {
            let body = std::str::from_utf8(body).expect("chunk must be utf8");
            for versioned_edge in [
                "\"./app.js?v=",
                "\"./terminal.js?v=",
                "import(\"./terminal.js?v=",
            ] {
                assert!(
                    !body.contains(versioned_edge),
                    "{path} must not rewrite module edges with {versioned_edge}"
                );
            }
            for sibling in ["./app.js", "./terminal.js"] {
                if body.contains(sibling) {
                    assert!(
                        body.contains(&format!("\"{sibling}\"")),
                        "{path} must keep bare sibling import {sibling}"
                    );
                    assert!(
                        !body.contains(&format!("\"{sibling}?v=")),
                        "{path} must not version sibling import {sibling}"
                    );
                }
            }
        }

        let terminal_body = std::str::from_utf8(terminal.body).expect("utf8");
        assert!(
            terminal_body.contains("\"./app.js\""),
            "terminal.js must back-import the entry chunk at the bare URL"
        );
    }

    #[test]
    fn app_version_is_stable_and_carries_crate_version() {
        let version = app_version();
        assert!(version.starts_with(env!("CARGO_PKG_VERSION")));
        // Fingerprint suffix keeps it stable within a build.
        assert_eq!(version, app_version());
    }

    #[test]
    fn app_version_changes_when_any_embedded_asset_changes() {
        let baseline = shell_version_from_assets(
            b"<!doctype html>",
            b"console.log('a');",
            b"body { color: black; }",
            b"/* terminal a */",
        );
        let changed = shell_version_from_assets(
            b"<!doctype html>",
            b"console.log('b');",
            b"body { color: black; }",
            b"/* terminal a */",
        );

        assert_ne!(baseline, changed);
    }

    #[test]
    fn assets_adapter_serves_stylesheet() {
        let asset = static_asset("/app.css").unwrap();
        assert_eq!(asset.content_type, "text/css; charset=utf-8");
        assert!(!asset.body.is_empty());
    }

    #[test]
    fn assets_adapter_serves_app_script_asset() {
        let asset = static_asset("/app.js").unwrap();
        assert_eq!(asset.content_type, "text/javascript; charset=utf-8");
        assert!(!asset.body.is_empty());
    }

    #[test]
    fn assets_adapter_serves_terminal_script_asset() {
        let asset = static_asset("/terminal.js").unwrap();
        assert_eq!(asset.content_type, "text/javascript; charset=utf-8");
        assert!(!asset.body.is_empty());
    }
}
