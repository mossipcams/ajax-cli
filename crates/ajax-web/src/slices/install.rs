//! Installable PWA shell.

pub struct StaticAsset {
    pub content_type: &'static str,
    pub body: &'static [u8],
}

pub fn pwa_shell() -> &'static str {
    include_str!("../../web/index.html")
}

pub fn static_asset(path: &str) -> Option<StaticAsset> {
    match path {
        "/app.css" => Some(StaticAsset {
            content_type: "text/css; charset=utf-8",
            body: include_bytes!("../../web/app.css"),
        }),
        "/app.js" => Some(StaticAsset {
            content_type: "text/javascript; charset=utf-8",
            body: include_bytes!("../../web/app.js"),
        }),
        "/manifest.webmanifest" => Some(StaticAsset {
            content_type: "application/manifest+json; charset=utf-8",
            body: include_bytes!("../../web/manifest.webmanifest"),
        }),
        "/sw.js" => Some(StaticAsset {
            content_type: "text/javascript; charset=utf-8",
            body: include_bytes!("../../web/sw.js"),
        }),
        "/icons/icon-192.png" => Some(StaticAsset {
            content_type: "image/png",
            body: include_bytes!("../../web/icons/icon-192.png"),
        }),
        "/icons/icon-512.png" => Some(StaticAsset {
            content_type: "image/png",
            body: include_bytes!("../../web/icons/icon-512.png"),
        }),
        "/icons/icon-maskable-512.png" => Some(StaticAsset {
            content_type: "image/png",
            body: include_bytes!("../../web/icons/icon-maskable-512.png"),
        }),
        "/icons/apple-touch-icon.png" => Some(StaticAsset {
            content_type: "image/png",
            body: include_bytes!("../../web/icons/apple-touch-icon.png"),
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{pwa_shell, static_asset};

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
    fn pwa_shell_is_local_only_and_service_worker_caches_only_static_shell() {
        let shell = pwa_shell();
        assert!(!shell.contains("fonts.googleapis.com"));
        assert!(!shell.contains("fonts.gstatic.com"));

        let script = std::str::from_utf8(static_asset("/app.js").unwrap().body).unwrap();
        assert!(script.contains("Action failed"));
        assert!(script.contains("network error"));

        let worker = std::str::from_utf8(static_asset("/sw.js").unwrap().body).unwrap();
        assert!(worker.contains("ajax-cockpit-v12"));
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
}
