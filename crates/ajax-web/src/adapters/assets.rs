//! Static browser shell asset embedding and lookup mechanisms.

use std::sync::OnceLock;

pub struct StaticAsset {
    pub content_type: &'static str,
    pub body: &'static [u8],
}

pub fn browser_shell_html() -> String {
    let version = app_version();
    let mut html = include_str!("../../web/dist/index.html")
        .replace("__AJAX_APP_VERSION__", version)
        .replace("src=\"/app.js\"", &format!("src=\"/app.js?v={version}\""))
        .replace(
            "href=\"/app.css\"",
            &format!("href=\"/app.css?v={version}\""),
        );
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
            body: versioned_app_js(),
        }),
        "/terminal.js" => Some(StaticAsset {
            content_type: "text/javascript; charset=utf-8",
            body: versioned_terminal_js(),
        }),
        _ => None,
    }
}

/// Rewrite every reference to a sibling chunk so it carries `?v=`.
///
/// Browsers key ES modules by full URL. If one chunk is fetched as
/// `/app.js?v=X` (from the shell) and another chunk imports it as bare
/// `./app.js`, the module graph is evaluated **twice** — two React instances,
/// which surfaces as an invalid-hook-call crash the moment the lazy terminal
/// chunk loads. Every cross-chunk edge must therefore agree on the query.
fn version_chunk_refs(raw: &'static [u8], what: &str) -> &'static [u8] {
    let version = app_version();
    let mut rewritten = std::str::from_utf8(raw)
        .unwrap_or_else(|_| panic!("embedded {what} must be utf8"))
        .to_string();
    for sibling in ["./app.js", "./terminal.js"] {
        rewritten = rewritten.replace(
            &format!("\"{sibling}\""),
            &format!("\"{sibling}?v={version}\""),
        );
    }
    Box::leak(rewritten.into_bytes().into_boxed_slice())
}

/// Served `/app.js` body. The fingerprint in `app_version` is still computed
/// from the raw embedded bytes, so this rewrite does not feed back into the
/// version string.
fn versioned_app_js() -> &'static [u8] {
    static VERSIONED: OnceLock<&'static [u8]> = OnceLock::new();
    VERSIONED.get_or_init(|| version_chunk_refs(include_bytes!("../../web/dist/app.js"), "app.js"))
}

/// Served `/terminal.js` body. The lazy chunk imports shared modules back out
/// of the entry chunk (`from "./app.js"`); that edge needs the same `?v=` the
/// shell used, or the entry is instantiated a second time.
fn versioned_terminal_js() -> &'static [u8] {
    static VERSIONED: OnceLock<&'static [u8]> = OnceLock::new();
    VERSIONED.get_or_init(|| {
        version_chunk_refs(include_bytes!("../../web/dist/terminal.js"), "terminal.js")
    })
}

#[cfg(test)]
mod tests {
    use super::{app_version, browser_shell_html, shell_version_from_assets, static_asset};

    #[test]
    fn assets_adapter_embeds_browser_shell() {
        assert!(browser_shell_html().contains("<!doctype html>"));
    }

    /// Browsers key ES modules by URL. If the shell loads `/app.js?v=X` but the
    /// lazy terminal chunk imports bare `./app.js`, the entry is evaluated a
    /// second time — two React instances, and the task route dies with an
    /// invalid-hook-call the moment the terminal loads. Every cross-chunk edge
    /// must carry the same `?v=`.
    #[test]
    fn served_chunks_never_reference_a_sibling_without_the_version_query() {
        let version = app_version();
        for path in ["/app.js", "/terminal.js"] {
            let asset = static_asset(path).expect("asset must exist");
            let body = std::str::from_utf8(asset.body).expect("chunk must be utf8");
            for sibling in ["./app.js", "./terminal.js"] {
                assert!(
                    !body.contains(&format!("\"{sibling}\"")),
                    "{path} references bare {sibling} — that URL mismatch \
                     instantiates the module graph twice"
                );
                let versioned = format!("\"{sibling}?v={version}\"");
                if body.contains(sibling) {
                    assert!(
                        body.contains(&versioned),
                        "{path} references {sibling} without the live version query"
                    );
                }
            }
        }
    }

    /// The lazy chunk's back-import of the entry is the edge that regressed.
    #[test]
    fn terminal_chunk_imports_the_entry_with_the_version_query() {
        let version = app_version();
        let terminal = static_asset("/terminal.js").expect("terminal chunk");
        let body = std::str::from_utf8(terminal.body).expect("utf8");
        assert!(
            body.contains(&format!("\"./app.js?v={version}\"")),
            "terminal.js must import the entry chunk at the versioned URL"
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
