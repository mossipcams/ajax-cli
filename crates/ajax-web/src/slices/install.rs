//! Installable PWA shell.

use crate::adapters::assets;

pub use crate::adapters::assets::StaticAsset;

pub fn pwa_shell() -> String {
    assets::pwa_shell_html()
}

pub fn app_version() -> &'static str {
    assets::app_version()
}

pub fn static_asset(path: &str) -> Option<StaticAsset> {
    assets::static_asset(path)
}

#[cfg(test)]
mod tests {
    //! These tests verify the *serving contract* of the bundled Svelte shell:
    //! the static HTML mount point, the asset routes, the preserved visual
    //! language, and the service-worker cleanup guarantees. The browser's
    //! runtime behavior (rendering, routing, polling, confirmations, prompts)
    //! is covered by the Vitest component/unit suite under `web/src`, so these
    //! tests deliberately do not grep the minified bundle for implementation
    //! detail — that coupling moved out with the legacy hand-written `app.js`.
    use super::{app_version, pwa_shell, static_asset};
    use crate::adapters::assets as asset_adapter;

    #[test]
    fn install_slice_delegates_to_assets_adapter() {
        let from_install = static_asset("/app.css").unwrap();
        let from_assets = asset_adapter::static_asset("/app.css").unwrap();

        assert_eq!(from_install.content_type, from_assets.content_type);
        assert_eq!(from_install.body, from_assets.body);
    }

    #[test]
    fn shell_is_the_bundled_svelte_mount_point() {
        let shell = pwa_shell();

        assert!(shell.contains("<!doctype html>"));
        assert!(shell.contains("name=\"viewport\""));
        assert!(shell.contains("width=device-width"));
        assert!(shell.contains("name=\"ajax-app-version\""));
        // The build-time placeholder is replaced with the live version.
        assert!(shell.contains(app_version()));
        assert!(!shell.contains("__AJAX_APP_VERSION__"));
        // One local module script and one local stylesheet.
        assert!(shell.contains("src=\"/app.js\""));
        assert!(shell.contains("href=\"/app.css\""));
        assert!(shell.contains("type=\"module\""));
        assert!(shell.contains("href=\"/manifest.webmanifest\""));
        assert!(shell.contains("href=\"/icons/icon-192.png\""));
        // Svelte mounts into this single node.
        assert!(shell.contains("id=\"app\""));
    }

    #[test]
    fn shell_no_longer_carries_the_legacy_imperative_dom() {
        let shell = pwa_shell();
        // The hand-built container shell is gone; everything below the mount
        // point is rendered client-side by Svelte components.
        for legacy in [
            "class=\"cockpit-chrome\"",
            "id=\"inbox\"",
            "id=\"repos\"",
            "id=\"new-task-row\"",
            "id=\"settings-view\"",
            "id=\"connection-status\"",
            "id=\"task-detail\"",
            "id=\"pwa-warning\"",
            "id=\"attention-summary\"",
        ] {
            assert!(
                !shell.contains(legacy),
                "static shell should no longer hardcode legacy node {legacy}"
            );
        }
    }

    #[test]
    fn manifest_is_served_as_an_installable_pwa_manifest() {
        let manifest = static_asset("/manifest.webmanifest").unwrap();
        assert_eq!(
            manifest.content_type,
            "application/manifest+json; charset=utf-8"
        );
        assert!(std::str::from_utf8(manifest.body)
            .unwrap()
            .contains("\"display\""));
    }

    #[test]
    fn service_worker_is_a_self_unregistering_cleanup_only_worker() {
        let worker = static_asset("/sw.js").unwrap();
        assert_eq!(worker.content_type, "text/javascript; charset=utf-8");
        let text = std::str::from_utf8(worker.body).unwrap();
        assert!(text.contains("self.registration.unregister"));
        assert!(!text.contains("addEventListener(\"fetch\""));
        assert!(!text.contains("addEventListener(\"push\""));
        assert!(!text.contains("notificationclick"));
        assert!(!text.contains("showNotification"));
        assert!(!text.contains("caches.open"));
        assert!(!text.contains("IndexedDB"));
    }

    #[test]
    fn icons_are_served_as_png_from_the_bundle() {
        for path in [
            "/icons/icon-192.png",
            "/icons/icon-512.png",
            "/icons/icon-maskable-512.png",
            "/icons/apple-touch-icon.png",
        ] {
            let icon = static_asset(path).unwrap_or_else(|| panic!("missing icon route: {path}"));
            assert_eq!(icon.content_type, "image/png", "{path}");
            assert!(
                icon.body
                    .starts_with(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]),
                "{path} is not a PNG"
            );
        }
    }

    #[test]
    fn stylesheet_preserves_the_safari_first_visual_language() {
        // Compare without internal spaces so the assertions survive CSS
        // minification (`scrollbar-width:none` vs `scrollbar-width: none`).
        let css = std::str::from_utf8(static_asset("/app.css").unwrap().body).unwrap();
        let compact = css.replace(' ', "").to_ascii_lowercase();

        assert!(compact.contains(".cockpit-chrome"));
        assert!(compact.contains("env(safe-area-inset-top)"));
        assert!(compact.contains("env(safe-area-inset-bottom)"));
        assert!(compact.contains("scrollbar-width:none"));
        assert!(compact.contains("::-webkit-scrollbar"));
        // Inputs stay >= 16px so iOS Safari does not zoom on focus.
        assert!(compact.contains("font-size:16px"));
        // Mid-century-modern walnut palette tokens.
        for hex in ["#f4eee0", "#251e1a", "#c9a24a", "#367069", "#bc5c3e"] {
            assert!(compact.contains(hex), "css missing palette token: {hex}");
        }
        // Full-height layouts must use dynamic units, never 100vh, on iOS.
        assert!(!compact.contains("100vh"));
    }

    #[test]
    fn bundle_targets_the_same_origin_api_and_never_registers_a_worker() {
        let script = std::str::from_utf8(static_asset("/app.js").unwrap().body).unwrap();
        assert!(!script.is_empty());
        // String literals survive minification — assert the same-origin API
        // surface the client speaks to.
        for endpoint in [
            "/api/cockpit",
            "/api/operations",
            "/api/server/restart",
            "/answer",
            "#/settings",
            "request_id",
            "no-store",
        ] {
            assert!(
                script.contains(endpoint),
                "bundle missing API usage {endpoint}"
            );
        }
        // Safari-first: never register a service worker, never use push.
        assert!(!script.contains("serviceWorker.register"));
        assert!(!script.contains("pushManager.subscribe"));
        assert!(!script.contains("/api/push"));
    }
}
