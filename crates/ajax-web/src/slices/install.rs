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
    fn install_slice_serves_pwa_shell_and_assets() {
        let shell = pwa_shell();

        assert!(shell.contains("<!doctype html>"));
        assert!(shell.contains("name=\"viewport\""));
        assert!(shell.contains("name=\"ajax-app-version\""));
        assert!(shell.contains(app_version()));
        assert!(!shell.contains("__AJAX_APP_VERSION__"));
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
            "class=\"cockpit-chrome\"",
            "class=\"page-lead\"",
            "id=\"status-line\"",
            "id=\"new-task-row\"",
            "id=\"inbox\"",
            "id=\"repos\"",
            "id=\"empty-state\"",
            "id=\"new-task-sheet\"",
            "value=\"cursor\"",
            "id=\"task-detail\"",
            "id=\"settings-view\"",
            "id=\"settings-link\"",
            "id=\"restart-server\"",
            "rel=\"apple-touch-icon\"",
            "href=\"/icons/icon-192.png\"",
        ] {
            assert!(shell.contains(expected), "shell missing {expected}");
        }
        for removed in [
            "id=\"offline-banner\"",
            "id=\"notify-button\"",
            "id=\"refresh-button\"",
            "id=\"new-task-button\"",
            "id=\"tidy-button\"",
            "id=\"help-button\"",
            "id=\"help-sheet\"",
            "id=\"alerts-banner\"",
            "id=\"repair-pwa\"",
            "id=\"repair-status\"",
            // PWA fully retired in favour of Safari-first: the standalone
            // warning and the attention-summary grid are gone.
            "id=\"pwa-warning\"",
            "id=\"attention-summary\"",
        ] {
            assert!(
                !shell.contains(removed),
                "shell should no longer contain {removed}"
            );
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
    fn pwa_stylesheet_pins_top_banners_inside_safe_area_chrome() {
        let css = std::str::from_utf8(static_asset("/app.css").unwrap().body).unwrap();
        let lowered = css.to_ascii_lowercase();

        assert!(
            lowered.contains(".cockpit-chrome"),
            "css must group top banners with the header chrome"
        );
        assert!(
            lowered.contains(".cockpit-chrome") && lowered.contains("env(safe-area-inset-top)"),
            "sticky cockpit chrome must respect the iOS status-bar inset"
        );
    }

    #[test]
    fn pwa_stylesheet_hides_scrollbars_while_preserving_overflow_scrolling() {
        let css = std::str::from_utf8(static_asset("/app.css").unwrap().body).unwrap();
        let lowered = css.to_ascii_lowercase();

        assert!(
            lowered.contains("scrollbar-width: none"),
            "css should hide scrollbars for Firefox and modern Safari"
        );
        assert!(
            lowered.contains("::-webkit-scrollbar"),
            "css should hide the iOS overlay scrollbar"
        );
    }

    #[test]
    fn pwa_stylesheet_uses_mid_century_modern_palette_and_no_monospace_body() {
        let css = std::str::from_utf8(static_asset("/app.css").unwrap().body).unwrap();
        let lowered = css.to_ascii_lowercase();

        // Refined walnut palette: cream ink, walnut tint, mustard, teal,
        // terracotta. Mustard is unchanged from the original variant.
        for hex in ["#f4eee0", "#251e1a", "#c9a24a", "#367069", "#bc5c3e"] {
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
    fn browser_shell_is_local_only_and_service_worker_is_non_critical_cleanup() {
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
        assert!(script.contains("#/settings"));
        assert!(script.contains("/api/server/restart"));
        assert!(
            script.contains("/answer"),
            "dashboard approvals should use the guarded answer endpoint"
        );
        assert!(
            !script.contains("Type your response"),
            "free-form browser input should stay out of the dashboard"
        );

        let worker = std::str::from_utf8(static_asset("/sw.js").unwrap().body).unwrap();
        assert!(worker.contains("self.registration.unregister"));
        assert!(
            !worker.contains("addEventListener(\"fetch\""),
            "service worker must not intercept fetches"
        );
        assert!(!worker.contains("addEventListener(\"push\""));
        assert!(!worker.contains("notificationclick"));
        assert!(!worker.contains("showNotification"));
        assert!(!worker.contains("caches.open"));
        assert!(!worker.contains("IndexedDB"));
        assert!(!worker.contains("sync"));
    }

    #[test]
    fn pwa_destructive_confirm_stays_stable_without_flashy_animation() {
        let script = std::str::from_utf8(static_asset("/app.js").unwrap().body).unwrap();
        let css = std::str::from_utf8(static_asset("/app.css").unwrap().body).unwrap();

        assert!(
            script.contains("const CONFIRM_TIMEOUT_MS = 8000"),
            "drop confirm window should stay open long enough on mobile"
        );
        assert!(
            script.contains("pendingConfirmByKey"),
            "confirm state must survive cockpit refresh re-renders"
        );
        assert!(
            script.contains("function applyPendingConfirm"),
            "rebuilt action buttons must restore an in-flight confirm"
        );
        assert!(
            !css.contains("animation: pulse infinite"),
            "confirming destructive actions should not flash"
        );
        assert!(
            css.contains(".action {\n  flex: 0 0 auto;\n  background: transparent;\n  border: 1px solid var(--rule-strong);\n  border-radius: 999px;"),
            "task action buttons should use pill geometry"
        );
        assert!(
            css.contains(
                ".action.primary {\n  background: var(--teal);\n  border-color: var(--teal);"
            ),
            "primary actions should be the filled teal pill"
        );
    }

    #[test]
    fn browser_shell_removes_notification_opt_in_and_standalone_warning() {
        let shell = pwa_shell();
        let script = std::str::from_utf8(static_asset("/app.js").unwrap().body).unwrap();

        // PWA is fully retired: there is no standalone mode left to warn about.
        assert!(
            !shell.contains("id=\"pwa-warning\""),
            "shell must not carry the retired standalone warning"
        );
        for gone in [
            "function syncStandaloneWarning",
            "Ajax works best in Safari on iOS",
        ] {
            assert!(
                !script.contains(gone),
                "app.js must not contain retired standalone-warning code: {gone}"
            );
        }

        for forbidden in [
            "Notification.requestPermission",
            "PushManager",
            "pushManager.subscribe",
            "/api/push/config",
            "/api/push/subscribe",
            "Add Ajax to your Home Screen to enable alerts",
            "Turn on alerts",
        ] {
            assert!(
                !script.contains(forbidden),
                "app.js must not contain notification opt-in code: {forbidden}"
            );
        }
    }

    #[test]
    fn browser_shell_does_not_register_service_worker_on_boot() {
        let script = std::str::from_utf8(static_asset("/app.js").unwrap().body).unwrap();

        assert!(
            script.contains("unregisterExistingServiceWorkers"),
            "app.js should clean up stale workers from older PWA builds"
        );
        assert!(
            !script.contains("serviceWorker.register"),
            "Safari-first shell should not register a service worker"
        );
        assert!(!script.contains("updateViaCache"));
    }

    #[test]
    fn browser_script_refreshes_after_resume_without_service_worker_updates() {
        let script = std::str::from_utf8(static_asset("/app.js").unwrap().body).unwrap();

        for expected in [
            "const loadedAppVersion",
            "ajax-app-version",
            "function refreshAfterResume",
            "window.addEventListener(\"pageshow\"",
            "window.addEventListener(\"focus\"",
        ] {
            assert!(script.contains(expected), "app.js missing {expected}");
        }

        assert!(!script.contains("registration.update()"));
        assert!(!script.contains("updateViaCache"));
    }

    #[test]
    fn pwa_settings_exposes_connection_diagnostics() {
        let shell = pwa_shell();
        let script = std::str::from_utf8(static_asset("/app.js").unwrap().body).unwrap();

        for expected in [
            "id=\"run-diagnostics\"",
            "id=\"copy-diagnostics\"",
            "id=\"diagnostics-output\"",
            "Diagnostics",
        ] {
            assert!(shell.contains(expected), "shell missing {expected}");
        }

        for expected in [
            "function runDiagnostics",
            "browser_mode",
            "backend_url",
            "navigator.onLine",
            "navigator.serviceWorker.controller",
            "loadedAppVersion",
            "server_version",
            "last_successful_connection_at",
            "last_fetch_error",
            "last_fetch_status",
            "\"/api/health\"",
            "\"/api/version\"",
            "\"/api/cockpit\"",
            "`/api/tasks/${encodeURIComponent(detailHandle)}`",
            "status",
            "error",
        ] {
            assert!(script.contains(expected), "app.js missing {expected}");
        }
    }

    #[test]
    fn browser_shell_exposes_connection_recovery_controls() {
        let shell = pwa_shell();
        let script = std::str::from_utf8(static_asset("/app.js").unwrap().body).unwrap();

        for expected in [
            "id=\"connection-status\"",
            "id=\"connection-retry\"",
            "id=\"connection-reload\"",
            "id=\"connection-copy-diagnostics\"",
            "id=\"connection-health-link\"",
        ] {
            assert!(shell.contains(expected), "shell missing {expected}");
        }

        for expected in [
            "connected",
            "checking",
            "reconnecting",
            "disconnected",
            "backend unreachable",
            "stale session",
            "function setConnectionState",
            "function copyDiagnostics",
        ] {
            assert!(script.contains(expected), "app.js missing {expected}");
        }
    }

    #[test]
    fn browser_script_forces_health_check_on_resume_events() {
        let script = std::str::from_utf8(static_asset("/app.js").unwrap().body).unwrap();

        for expected in [
            "function forceBackendHealthCheck",
            "forceBackendHealthCheck(\"initial\")",
            "forceBackendHealthCheck(\"online\")",
            "forceBackendHealthCheck(\"visibilitychange\")",
            "forceBackendHealthCheck(\"pageshow\")",
            "forceBackendHealthCheck(\"focus\")",
            "refreshCurrentRoute({ forceHealth: true })",
        ] {
            assert!(script.contains(expected), "app.js missing {expected}");
        }
    }

    #[test]
    fn browser_shell_uses_safari_layout_basics() {
        let shell = pwa_shell();
        let css = std::str::from_utf8(static_asset("/app.css").unwrap().body).unwrap();

        assert!(shell.contains(
            "name=\"viewport\" content=\"width=device-width, initial-scale=1, viewport-fit=cover\""
        ));
        assert!(shell.contains("id=\"bottom-nav\""));
        assert!(
            css.contains("min-height: 16px") || css.contains("font-size: 16px"),
            "inputs must stay at least 16px to avoid iOS focus zoom"
        );
        assert!(css.contains("env(safe-area-inset-bottom)"));
        assert!(!css.contains("100vh"));
    }

    #[test]
    fn dashboard_splits_inbox_from_calm_task_list_with_status_counts() {
        let script = std::str::from_utf8(static_asset("/app.js").unwrap().body).unwrap();

        // Inbox ("Needs you") cards up top, lightweight task rows below, each
        // section carrying its own count chip; tapping either opens detail.
        for expected in [
            "function renderInbox",
            "function inboxCard",
            "function renderTasks",
            "function taskRow",
            "function sectionHead",
            "\"Needs you\"",
            "inbox-card",
            "task-row",
            "data-open-task",
            "const STATUS_META",
        ] {
            assert!(script.contains(expected), "app.js missing {expected}");
        }

        // The retired buggy attention-summary grid (Title-cased ui_state
        // comparisons that never matched) must be gone.
        for gone in [
            "function renderAttentionSummary",
            "function copyTaskSummary",
            "data-copy-summary",
        ] {
            assert!(
                !script.contains(gone),
                "app.js must not contain retired list code: {gone}"
            );
        }
    }

    #[test]
    fn terminal_details_expose_focused_mobile_shortcuts() {
        let script = std::str::from_utf8(static_asset("/app.js").unwrap().body).unwrap();

        for expected in [
            "terminal-shortcuts",
            "Continue",
            "Approve plan",
            "Run tests",
            "Show diff",
            "Stop task",
            "Restart task",
            "Copy last error",
            "Copy visible output",
            "function runTerminalShortcut",
        ] {
            assert!(script.contains(expected), "app.js missing {expected}");
        }
    }

    #[test]
    fn settings_do_not_expose_pwa_repair_action() {
        let shell = pwa_shell();
        let script = std::str::from_utf8(static_asset("/app.js").unwrap().body).unwrap();

        for forbidden in ["id=\"repair-pwa\"", "id=\"repair-status\"", "Repair PWA"] {
            assert!(
                !shell.contains(forbidden),
                "shell must not contain {forbidden}"
            );
        }

        for forbidden in [
            "function repairPwa",
            "Repairing PWA",
            "window.location.replace(`/?repair=${Date.now()}`)",
        ] {
            assert!(
                !script.contains(forbidden),
                "app.js must not contain {forbidden}"
            );
        }
    }

    #[test]
    fn dashboard_detail_view_uses_operator_cards_instead_of_pane_log() {
        let script = std::str::from_utf8(static_asset("/app.js").unwrap().body).unwrap();

        for expected in [
            "Current status",
            "Needs from you",
            "Best next step",
            "Recent milestones",
            "View terminal details",
        ] {
            assert!(script.contains(expected), "app.js missing {expected}");
        }

        for removed in [
            "Pane is quiet.",
            "Pinned to bottom",
            "Type your response",
            "Send to agent",
        ] {
            assert!(
                !script.contains(removed),
                "app.js should not foreground pane UI copy {removed}"
            );
        }
    }
}
