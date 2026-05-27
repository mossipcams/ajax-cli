//! Installable PWA shell.

use crate::adapters::assets;

pub use crate::adapters::assets::StaticAsset;

pub fn pwa_shell() -> &'static str {
    assets::pwa_shell_html()
}

pub fn static_asset(path: &str) -> Option<StaticAsset> {
    assets::static_asset(path)
}

#[cfg(test)]
mod tests {
    use super::{pwa_shell, static_asset};
    use crate::adapters::assets as asset_adapter;

    #[test]
    fn install_slice_delegates_to_assets_adapter() {
        let from_install = static_asset("/app.css").unwrap();
        let from_assets = asset_adapter::static_asset("/app.css").unwrap();

        assert_eq!(from_install.content_type, from_assets.content_type);
        assert_eq!(from_install.body, from_assets.body);
    }

    #[test]
    fn install_slice_serves_pwa_shell_and_assets() {
        let shell = pwa_shell();

        assert!(shell.contains("<!doctype html>"));
        assert!(shell.contains("name=\"viewport\""));
        assert!(shell.contains("href=\"/app.css\""));
        assert!(shell.contains("src=\"/app.js\""));
        assert!(shell.contains("href=\"/manifest.webmanifest\""));

        let manifest = static_asset("/manifest.webmanifest").unwrap();
        assert_eq!(
            manifest.content_type,
            "application/manifest+json; charset=utf-8"
        );
        assert!(std::str::from_utf8(manifest.body)
            .unwrap()
            .contains("\"display\""));

        let service_worker = static_asset("/sw.js").unwrap();
        assert_eq!(
            service_worker.content_type,
            "text/javascript; charset=utf-8"
        );
        assert!(std::str::from_utf8(service_worker.body)
            .unwrap()
            .contains("self.addEventListener"));
    }

    #[test]
    fn install_slice_serves_real_mobile_cockpit_shell_and_icons() {
        let shell = pwa_shell();

        for expected in [
            "id=\"offline-banner\"",
            "id=\"status-line\"",
            "id=\"notify-button\"",
            "id=\"refresh-button\"",
            "id=\"inbox\"",
            "id=\"repos\"",
            "id=\"empty-state\"",
            "id=\"new-task-button\"",
            "id=\"new-task-sheet\"",
            "id=\"task-detail\"",
            "rel=\"apple-touch-icon\"",
            "href=\"/icons/icon-192.png\"",
        ] {
            assert!(shell.contains(expected), "shell missing {expected}");
        }

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
    fn pwa_stylesheet_uses_mid_century_modern_palette_and_no_monospace_body() {
        let css = std::str::from_utf8(static_asset("/app.css").unwrap().body).unwrap();
        let lowered = css.to_ascii_lowercase();

        for hex in ["#f2ebdc", "#2a2522", "#c9a24a", "#2e5e5a", "#b7553a"] {
            assert!(
                lowered.contains(hex),
                "css missing MCM palette token: {hex}"
            );
        }

        assert!(
            !lowered.contains("jetbrains mono"),
            "body should no longer rely on JetBrains Mono"
        );
        assert!(
            !lowered.contains("berkeley mono"),
            "body should no longer rely on Berkeley Mono"
        );
    }

    #[test]
    fn pwa_shell_is_local_only_and_service_worker_caches_only_static_shell() {
        let shell = pwa_shell();
        assert!(!shell.contains("fonts.googleapis.com"));
        assert!(!shell.contains("fonts.gstatic.com"));

        let script = std::str::from_utf8(static_asset("/app.js").unwrap().body).unwrap();
        assert!(script.contains("Action failed"));
        assert!(script.contains("network error"));
        assert!(
            script.contains("/api/tasks"),
            "missing POST start endpoint usage"
        );

        let worker = std::str::from_utf8(static_asset("/sw.js").unwrap().body).unwrap();
        assert!(worker.contains("ajax-cockpit-v15"));
        assert!(worker.contains("url.pathname.startsWith(\"/api/\")"));
        for cached in [
            "\"/\"",
            "\"/app.css\"",
            "\"/app.js\"",
            "\"/manifest.webmanifest\"",
            "\"/sw.js\"",
            "\"/icons/icon-192.png\"",
            "\"/icons/icon-512.png\"",
            "\"/icons/icon-maskable-512.png\"",
            "\"/icons/apple-touch-icon.png\"",
        ] {
            assert!(
                worker.contains(cached),
                "service worker does not cache {cached}"
            );
        }
        assert!(!worker.contains("IndexedDB"));
        assert!(!worker.contains("sync"));
    }

    #[test]
    fn pwa_exposes_visible_notification_opt_in_with_environment_guidance() {
        let shell = pwa_shell();
        let script = std::str::from_utf8(static_asset("/app.js").unwrap().body).unwrap();

        assert!(
            shell.contains("id=\"notify-button\""),
            "shell must include the alerts control"
        );
        assert!(
            !shell.contains("id=\"notify-button\" type=\"button\" class=\"pill\" hidden>Alerts"),
            "alerts control must not start permanently hidden in the shell"
        );

        for expected in [
            "function notificationEnvironment()",
            "function syncNotificationUi()",
            "Add Ajax to your Home Screen to enable alerts.",
            "Alerts on",
            "Alerts blocked",
        ] {
            assert!(script.contains(expected), "app.js missing {expected}");
        }
    }
}
