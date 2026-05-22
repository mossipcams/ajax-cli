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
}
