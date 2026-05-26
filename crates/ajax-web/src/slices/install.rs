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
        assert!(shell.contains(
            r#"content="width=device-width, initial-scale=1, maximum-scale=1, user-scalable=no, viewport-fit=cover""#
        ));
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

        // Eames/Braun palette tokens — these aren't enforced as exact-value
        // checks; they confirm the cream/walnut/mustard/teal/terracotta family
        // is present in the stylesheet rather than the prior dark terminal
        // hex set.
        for hex in ["#f2ebdc", "#2a2522", "#c9a24a", "#2e5e5a", "#b7553a"] {
            assert!(
                lowered.contains(hex),
                "css missing MCM palette token: {hex}"
            );
        }

        // Body text must read as a geometric/grotesk sans, not the prior
        // mono-only stack.
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
        assert!(worker.contains("ajax-cockpit-v16"));
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
    fn manifest_and_shell_metadata_are_installable_and_consistent() {
        let shell = pwa_shell();
        let manifest: serde_json::Value =
            serde_json::from_slice(static_asset("/manifest.webmanifest").unwrap().body).unwrap();
        let theme = manifest["theme_color"].as_str().unwrap();

        assert_eq!(manifest["id"], "/");
        assert_eq!(manifest["start_url"], "/");
        assert_eq!(manifest["scope"], "/");
        assert_eq!(manifest["display"], "standalone");
        assert!(manifest["background_color"].as_str().is_some());
        assert!(
            shell.contains(&format!("name=\"theme-color\" content=\"{theme}\"")),
            "shell theme-color must match manifest theme_color"
        );
        assert!(shell.contains("name=\"apple-mobile-web-app-capable\""));
        assert!(shell.contains("name=\"apple-mobile-web-app-status-bar-style\""));
        assert!(shell.contains("rel=\"apple-touch-icon\""));

        let icons = manifest["icons"].as_array().unwrap();
        assert!(icons.iter().any(|icon| icon["purpose"] == "maskable"));
    }

    #[test]
    fn service_worker_has_update_safe_navigation_fallback() {
        let worker = std::str::from_utf8(static_asset("/sw.js").unwrap().body).unwrap();

        assert!(worker.contains("ajax-cockpit-v16"));
        assert!(worker.contains("request.mode === \"navigate\""));
        assert!(worker.contains("caches.match(\"/\")"));
        assert!(worker.contains("key !== CACHE"));
        assert!(worker.contains("caches.delete(key)"));
        assert!(worker.contains("url.pathname.startsWith(\"/api/\")"));
        assert!(!worker.contains("IndexedDB"));
        assert!(!worker.contains("sync"));
    }

    #[test]
    fn pwa_blocks_mobile_zoom_and_keyboard_jump_patterns() {
        let shell = pwa_shell();
        assert!(shell.contains(
            r#"<meta name="viewport" content="width=device-width, initial-scale=1, maximum-scale=1, user-scalable=no, viewport-fit=cover">"#
        ));

        let css = std::str::from_utf8(static_asset("/app.css").unwrap().body).unwrap();
        assert!(
            css.contains("input,\ntextarea,\nselect,\nbutton"),
            "form controls should share the mobile-safe font-size rule"
        );
        assert!(
            css.contains("font-size: 16px;"),
            "mobile browsers zoom focused form controls below 16px"
        );

        let script = std::str::from_utf8(static_asset("/app.js").unwrap().body).unwrap();
        assert!(
            !script.contains(".focus()"),
            "new-task sheet must not autofocus on mobile"
        );
    }

    #[test]
    fn pwa_blocks_ios_safari_pinch_zoom_gestures() {
        let css = std::str::from_utf8(static_asset("/app.css").unwrap().body).unwrap();
        assert!(
            css.contains("touch-action: manipulation;"),
            "Safari PWAs need touch-action hardening in addition to the viewport tag"
        );

        let script = std::str::from_utf8(static_asset("/app.js").unwrap().body).unwrap();
        for expected in [
            "gesturestart",
            "gesturechange",
            "gestureend",
            "touchmove",
            "event.touches.length > 1",
            "passive: false",
            "preventDefault()",
        ] {
            assert!(script.contains(expected), "app.js missing {expected}");
        }
    }

    #[test]
    fn pwa_script_guards_actions_with_request_ids_and_local_status() {
        let script = std::str::from_utf8(static_asset("/app.js").unwrap().body).unwrap();

        for expected in [
            "const inFlightActions = new Map();",
            "const actionStates = new Map();",
            "function actionKey(handle, action)",
            "function requestId()",
            "request_id",
            "operator_token",
            "mutationInFlight",
            "setActionState",
            "matchingActionButtons",
            "if (inFlightActions.has(key)) return;",
            "status: \"sending\"",
            "status: \"succeeded\"",
            "status: \"failed\"",
        ] {
            assert!(script.contains(expected), "app.js missing {expected}");
        }
        assert!(
            !script.contains("pendingActions"),
            "the browser must not persist or replay pending mutations"
        );
    }

    #[test]
    fn pwa_script_tokens_all_mutable_routes() {
        let script = std::str::from_utf8(static_asset("/app.js").unwrap().body).unwrap();

        assert!(script.contains("localStorage.getItem(OPERATOR_TOKEN_KEY)"));
        assert!(script.contains("localStorage.setItem(OPERATOR_TOKEN_KEY"));
        assert!(script.contains("/api/operations"));
        assert!(script.contains("/api/tasks"));
        assert!(script.contains("/api/push/subscribe"));
        assert!(script.contains("subscription: subscription.toJSON()"));
        assert!(script.contains("body: JSON.stringify({ repo, title, agent, request_id: requestId(), operator_token: token })"));
    }

    #[test]
    fn service_worker_deep_links_task_notifications_without_api_caching() {
        let worker = std::str::from_utf8(static_asset("/sw.js").unwrap().body).unwrap();

        assert!(worker.contains("ajax-cockpit-v16"));
        assert!(worker.contains("url.pathname.startsWith(\"/api/\")"));
        assert!(worker.contains("task_handle"));
        assert!(worker.contains("encodeURIComponent(data.task_handle)"));
        assert!(worker.contains("client.navigate(target)"));
        assert!(!worker.contains("\"/api/cockpit\""));
        assert!(!worker.contains("\"/api/actions\""));
        assert!(!worker.contains("\"/api/operations\""));
        assert!(!worker.contains("\"/api/tasks\""));
        assert!(!worker.contains("\"/api/push/subscribe\""));
    }
}
