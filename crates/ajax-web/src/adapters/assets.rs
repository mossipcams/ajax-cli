//! Static PWA asset embedding and lookup mechanisms.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::OnceLock;

pub struct StaticAsset {
    pub content_type: &'static str,
    pub body: &'static [u8],
}

pub fn pwa_shell_html() -> String {
    include_str!("../../web/dist/index.html").replace("__AJAX_APP_VERSION__", app_version())
}

/// Build identifier for the served PWA shell.
///
/// Combines the crate version with a fingerprint of the embedded shell assets,
/// so the value changes on every release *and* on any edit to the HTML/JS/CSS
/// or service worker. The mobile client polls `/api/version` and reloads when
/// this differs from the version it booted with — the only way an installed
/// iOS standalone PWA, which has no reload affordance and resumes a frozen
/// snapshot, will ever pick up a new shell without being deleted and re-added.
pub fn app_version() -> &'static str {
    static VERSION: OnceLock<String> = OnceLock::new();
    VERSION.get_or_init(|| {
        let mut hasher = DefaultHasher::new();
        for asset in [
            include_bytes!("../../web/dist/index.html").as_slice(),
            include_bytes!("../../web/dist/app.js").as_slice(),
            include_bytes!("../../web/dist/app.css").as_slice(),
            include_bytes!("../../web/dist/sw.js").as_slice(),
        ] {
            asset.hash(&mut hasher);
        }
        format!("{}-{:016x}", env!("CARGO_PKG_VERSION"), hasher.finish())
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
        "/manifest.webmanifest" => Some(StaticAsset {
            content_type: "application/manifest+json; charset=utf-8",
            body: include_bytes!("../../web/dist/manifest.webmanifest"),
        }),
        "/sw.js" => Some(StaticAsset {
            content_type: "text/javascript; charset=utf-8",
            body: include_bytes!("../../web/dist/sw.js"),
        }),
        "/icons/icon-192.png" => Some(StaticAsset {
            content_type: "image/png",
            body: include_bytes!("../../web/dist/icons/icon-192.png"),
        }),
        "/icons/icon-512.png" => Some(StaticAsset {
            content_type: "image/png",
            body: include_bytes!("../../web/dist/icons/icon-512.png"),
        }),
        "/icons/icon-maskable-512.png" => Some(StaticAsset {
            content_type: "image/png",
            body: include_bytes!("../../web/dist/icons/icon-maskable-512.png"),
        }),
        "/icons/apple-touch-icon.png" => Some(StaticAsset {
            content_type: "image/png",
            body: include_bytes!("../../web/dist/icons/apple-touch-icon.png"),
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{app_version, pwa_shell_html, static_asset};

    #[test]
    fn assets_adapter_embeds_pwa_shell() {
        assert!(pwa_shell_html().contains("<!doctype html>"));
    }

    #[test]
    fn app_version_is_stable_and_carries_crate_version() {
        let version = app_version();
        assert!(version.starts_with(env!("CARGO_PKG_VERSION")));
        // Fingerprint suffix keeps it stable within a build.
        assert_eq!(version, app_version());
    }

    #[test]
    fn assets_adapter_serves_stylesheet() {
        let asset = static_asset("/app.css").unwrap();
        assert_eq!(asset.content_type, "text/css; charset=utf-8");
        assert!(!asset.body.is_empty());
    }
}
