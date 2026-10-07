use crate::adapters::assets;

pub use crate::adapters::assets::StaticAsset;

pub fn browser_shell() -> String {
    assets::browser_shell_html()
}

pub fn app_version() -> &'static str {
    assets::app_version()
}

pub fn static_asset(path: &str) -> Option<StaticAsset> {
    assets::static_asset(path)
}

#[cfg(test)]
mod tests {
    use super::{app_version, browser_shell, static_asset};
    use crate::adapters::assets as asset_adapter;

    #[test]
    fn install_slice_delegates_to_assets_adapter() {
        let from_install = static_asset("/app.css").unwrap();
        let from_assets = asset_adapter::static_asset("/app.css").unwrap();

        assert_eq!(from_install.content_type, from_assets.content_type);
        assert_eq!(from_install.body, from_assets.body);
    }

    #[test]
    fn shell_is_the_bundled_react_mount_point() {
        let shell = browser_shell();

        assert!(shell.contains("<!doctype html>"));
        assert!(shell.contains("name=\"viewport\""));
        assert!(shell.contains("width=device-width"));
        assert!(shell.contains("name=\"ajax-app-version\""));
        assert!(shell.contains(app_version()));
        assert!(!shell.contains("__AJAX_APP_VERSION__"));
        assert!(shell.contains("src=\"/app.js\""));
        assert!(shell.contains("href=\"/app.css\""));
        assert!(!shell.contains("src=\"/app.js?"));
        assert!(!shell.contains("href=\"/app.css?"));
        assert!(shell.contains("type=\"module\""));
        assert!(!shell.contains("src=\"/terminal.js\""));
        assert!(!shell.contains("href=\"/terminal.js\""));
        assert!(shell.contains("id=\"app\""));
        assert!(
            shell.contains("ajax-boot-paint"),
            "shell must paint dark before /app.css arrives"
        );
        assert!(
            !shell.contains("ajax-retire-sw"),
            "shell must not carry retired PWA service-worker cleanup"
        );
    }

    #[test]
    fn shell_no_longer_carries_the_legacy_imperative_dom() {
        let shell = browser_shell();
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
    fn retired_pwa_install_assets_are_absent() {
        let shell = browser_shell();
        for retired in [
            "href=\"/manifest.webmanifest\"",
            "rel=\"apple-touch-icon\"",
            "href=\"/icons/icon-192.png\"",
        ] {
            assert!(
                !shell.contains(retired),
                "browser shell should not advertise retired install asset: {retired}"
            );
        }
        for path in [
            "/manifest.webmanifest",
            "/sw.js",
            "/icons/icon-192.png",
            "/icons/icon-512.png",
            "/icons/icon-maskable-512.png",
            "/icons/apple-touch-icon.png",
        ] {
            assert!(static_asset(path).is_none(), "{path} should be absent");
        }
    }

    #[test]
    fn shell_advertises_safe_pwa_browser_metadata_without_install_surface() {
        let shell = browser_shell();
        for meta in [
            "name=\"color-scheme\"",
            "name=\"theme-color\"",
            "name=\"mobile-web-app-capable\"",
            "apple-mobile-web-app-capable",
            "apple-mobile-web-app-title",
            "apple-mobile-web-app-status-bar-style",
        ] {
            assert!(
                shell.contains(meta),
                "browser shell should include safe PWA metadata: {meta}"
            );
        }
    }

    #[test]
    fn stylesheet_preserves_the_safari_first_visual_language() {
        let css = std::str::from_utf8(static_asset("/app.css").unwrap().body).unwrap();
        let compact = css.replace([' ', '"'], "").to_ascii_lowercase();

        assert!(compact.contains(".cockpit-chrome"));
        assert!(compact.contains("env(safe-area-inset-top)"));
        assert!(compact.contains("env(safe-area-inset-bottom)"));
        assert!(compact.contains("scrollbar-width:none"));
        assert!(compact.contains("::-webkit-scrollbar"));
        assert!(compact.contains("html.keyboard-open.app-viewport"));
        assert!(compact.contains("position:fixed"));
        assert!(compact.contains("height:var(--app-band-height"));
        assert!(compact.contains("font-size:16px"));
        for hex in ["#e6e6e6", "#87afd7", "#d7af5f", "#d78787", "#87af87"] {
            assert!(compact.contains(hex), "css missing palette token: {hex}");
        }
        assert!(!compact.contains("100vh"));
    }

    #[test]
    fn terminal_chunk_is_embedded_and_distinct_from_app() {
        let app = std::str::from_utf8(static_asset("/app.js").unwrap().body).unwrap();
        let term = std::str::from_utf8(static_asset("/terminal.js").unwrap().body).unwrap();
        assert!(!app.is_empty());
        assert!(!term.is_empty());
        assert_ne!(app, term);
        assert!(
            term.contains("xterm") || term.contains("XTerm") || term.contains("FitAddon"),
            "terminal.js should carry the xterm surface"
        );
    }

    #[test]
    fn bundle_targets_the_same_origin_api_and_never_registers_a_worker() {
        let app = std::str::from_utf8(static_asset("/app.js").unwrap().body).unwrap();
        let term = std::str::from_utf8(static_asset("/terminal.js").unwrap().body).unwrap();
        assert!(!app.is_empty());
        assert!(!term.is_empty());
        for endpoint in [
            "/api/cockpit",
            "/api/operations",
            "/api/push",
            "/api/server/test-in-stable",
            "#/settings",
            "request_id",
            "no-store",
        ] {
            assert!(
                app.contains(endpoint),
                "app.js missing API usage {endpoint}"
            );
        }
        assert!(
            app.contains("pushManager.subscribe"),
            "app.js missing declarative push subscribe"
        );
        assert!(
            !app.contains("FitAddon"),
            "app.js must not embed the xterm FitAddon (belongs in terminal.js)"
        );
        for script in [app, term] {
            assert!(!script.contains("serviceWorker"));
            assert!(!script.contains("/answer"));
            assert!(!script.contains("/input"));
        }
        assert!(
            !term.contains("pushManager.subscribe"),
            "terminal.js must not embed declarative push subscribe"
        );
    }
}
