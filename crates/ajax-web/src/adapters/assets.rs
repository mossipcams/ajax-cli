//! Static PWA asset embedding and lookup mechanisms.

pub struct StaticAsset {
    pub content_type: &'static str,
    pub body: &'static [u8],
}

pub fn pwa_shell_html() -> &'static str {
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
    use super::{pwa_shell_html, static_asset};

    #[test]
    fn assets_adapter_embeds_pwa_shell() {
        assert!(pwa_shell_html().contains("<!doctype html>"));
    }

    #[test]
    fn assets_adapter_serves_stylesheet() {
        let asset = static_asset("/app.css").unwrap();
        assert_eq!(asset.content_type, "text/css; charset=utf-8");
        assert!(!asset.body.is_empty());
    }
}
