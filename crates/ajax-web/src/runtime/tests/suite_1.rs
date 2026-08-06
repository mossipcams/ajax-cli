use super::*;

#[test]
fn axum_api_access_policy_classifies_public_and_protected_routes() {
    use super::ApiAccess;

    for (method, path) in [
        ("GET", "/"),
        ("GET", "/index.html"),
        ("GET", "/app.js"),
        ("GET", "/terminal.js"),
        ("GET", "/api/health"),
        ("POST", "/api/session"),
    ] {
        assert_eq!(
            super::api_access_policy(method, path),
            ApiAccess::Public,
            "{method} {path}"
        );
    }

    for (method, path) in [
        ("GET", "/api/session"),
        ("GET", "/api/cockpit"),
        ("GET", "/api/version"),
        ("GET", "/api/settings/web-session"),
        ("PUT", "/api/settings/web-session"),
        ("GET", "/api/push/vapid"),
        ("POST", "/api/push/subscribe"),
        ("DELETE", "/api/push/subscribe"),
        ("POST", "/api/push/test"),
        ("POST", "/api/server/restart"),
        ("POST", "/api/server/test-in-stable"),
        ("GET", "/api/dev-deploy"),
        ("POST", "/api/dev-deploy"),
        ("POST", "/api/operations"),
        ("POST", "/api/tasks"),
        ("GET", "/api/tasks/web%2Ffix-login"),
        ("GET", "/api/tasks/web%2Ffix-login/terminal"),
    ] {
        assert_eq!(
            super::api_access_policy(method, path),
            ApiAccess::BrowserSessionRequired,
            "{method} {path}"
        );
    }
}

#[tokio::test]
async fn axum_router_serves_static_shell_and_cockpit_json() {
    let context = CommandContext::new(Config::default(), InMemoryRegistry::default());
    let state = super::WebAppState::new(
        context,
        OkRunner,
        TestBridge::default(),
        scratch_dir("axum-static"),
    );
    let session_cookie = browser_session_cookie(&state);
    let app = super::axum_app(state);

    let shell = get_public(&app, "/").await;
    assert_eq!(shell.status(), StatusCode::OK);
    assert_eq!(shell.headers()["content-type"], "text/html; charset=utf-8");
    assert_eq!(shell.headers()["cache-control"], "no-store");
    let shell_body = to_bytes(shell.into_body(), usize::MAX).await.unwrap();
    assert!(std::str::from_utf8(&shell_body)
        .unwrap()
        .contains("Ajax Cockpit"));

    let cockpit = get(&app, &session_cookie, "/api/cockpit").await;
    assert_eq!(cockpit.status(), StatusCode::OK);
    assert_eq!(
        cockpit.headers()["content-type"],
        "application/json; charset=utf-8"
    );
    assert_eq!(cockpit.headers()["cache-control"], "no-store");
    assert_eq!(json_of(cockpit).await["cards"], serde_json::json!([]));

    let missing_api = get(&app, &session_cookie, "/api/missing").await;
    assert_eq!(missing_api.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        missing_api.headers()["content-type"],
        "application/json; charset=utf-8"
    );
    assert_eq!(missing_api.headers()["cache-control"], "no-store");
    let missing_api_body = to_bytes(missing_api.into_body(), usize::MAX).await.unwrap();
    assert!(!std::str::from_utf8(&missing_api_body)
        .unwrap()
        .contains("Ajax Cockpit"));

    for path in [
        "/manifest.webmanifest",
        "/sw.js",
        "/icons/icon-192.png",
        "/icons/icon-512.png",
        "/icons/icon-maskable-512.png",
        "/icons/apple-touch-icon.png",
    ] {
        let retired_asset = get_public(&app, path).await;
        assert_eq!(retired_asset.status(), StatusCode::NOT_FOUND, "{path}");
        assert_eq!(
            retired_asset.headers()["content-type"],
            "text/plain; charset=utf-8",
            "{path}"
        );
    }

    let missing_asset = get_public(&app, "/does-not-exist.css").await;
    assert_eq!(missing_asset.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        missing_asset.headers()["content-type"],
        "text/plain; charset=utf-8"
    );
    assert_eq!(missing_asset.headers()["cache-control"], "no-store");
    let missing_asset_body = to_bytes(missing_asset.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(
        std::str::from_utf8(&missing_asset_body).unwrap(),
        "not found"
    );
}

#[tokio::test]
async fn web_session_preference_api_is_authenticated_and_persistent() {
    let context = CommandContext::new(Config::default(), InMemoryRegistry::default());
    let (state, cookie, app) = app_with(context, TestBridge::default(), "web-session-pref-api");

    assert_eq!(
        get_public(&app, "/api/settings/web-session").await.status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        json_of(get(&app, &cookie, "/api/settings/web-session").await).await["enabled"],
        false
    );

    let response = app
        .clone()
        .oneshot(
            authenticated_request(&cookie, "/api/settings/web-session")
                .method("PUT")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"enabled":true}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert!(state.web_session_preference.enabled());
    assert_eq!(
        json_of(get(&app, &cookie, "/api/settings/web-session").await).await["enabled"],
        true
    );
}

#[tokio::test]
async fn static_shell_assets_are_no_store_and_gzipped() {
    let context = CommandContext::new(Config::default(), InMemoryRegistry::default());
    let (_state, cookie, app) = app_with(context, TestBridge::default(), "axum-static-cache-gzip");

    let version = crate::slices::install::app_version();
    for path in ["/app.js", "/app.css", "/terminal.js"] {
        for request_path in [path, &format!("{path}?v={version}")] {
            let response = get_public(&app, request_path).await;
            assert_eq!(response.status(), StatusCode::OK, "{request_path}");
            let cache_control = response.headers()["cache-control"]
                .to_str()
                .unwrap_or_default();
            assert_eq!(cache_control, "no-store", "{request_path} must not cache");
            assert!(
                response.headers().get("etag").is_none(),
                "{request_path} must not carry an ETag"
            );
            assert!(
                !cache_control.contains("immutable"),
                "{request_path} must not claim immutability"
            );
        }
    }

    // HTML shell and API remain no-store (do not get the immutable cache).
    let shell = get_public(&app, "/").await;
    assert_eq!(shell.status(), StatusCode::OK);
    assert_eq!(shell.headers()["cache-control"], "no-store");

    let cockpit = get(&app, &cookie, "/api/cockpit").await;
    assert_eq!(cockpit.status(), StatusCode::OK);
    assert_eq!(cockpit.headers()["cache-control"], "no-store");

    // Negotiated gzip applies to compressible static JS when requested.
    let app_js_gz = app
        .clone()
        .oneshot(
            AxumRequest::builder()
                .uri("/app.js")
                .header("accept-encoding", "gzip")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(app_js_gz.status(), StatusCode::OK);
    assert_eq!(app_js_gz.headers()["content-encoding"], "gzip");
}

#[tokio::test]
async fn static_shell_assets_ignore_if_none_match() {
    let context = CommandContext::new(Config::default(), InMemoryRegistry::default());
    let (_state, _cookie, app) = app_with(context, TestBridge::default(), "axum-static-etag");

    let etag = format!("W/\"{}\"", crate::slices::install::app_version());

    for path in ["/app.js", "/app.css", "/terminal.js"] {
        let baseline = get_public(&app, path).await;
        assert_eq!(baseline.status(), StatusCode::OK, "{path}");
        assert_eq!(
            baseline.headers()["cache-control"],
            "no-store",
            "{path} cache-control"
        );
        assert!(
            baseline.headers().get("etag").is_none(),
            "{path} must not carry an ETag"
        );
        let baseline_body = to_bytes(baseline.into_body(), usize::MAX).await.unwrap();

        // Matching If-None-Match must still return 200 with a body, never 304.
        let matched = app
            .clone()
            .oneshot(
                AxumRequest::builder()
                    .uri(path)
                    .header("if-none-match", &etag)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(matched.status(), StatusCode::OK, "{path} if-none-match");
        assert_eq!(
            matched.headers()["cache-control"],
            "no-store",
            "{path} if-none-match cache-control"
        );
        assert!(
            matched.headers().get("etag").is_none(),
            "{path} if-none-match must not carry an ETag"
        );
        let matched_body = to_bytes(matched.into_body(), usize::MAX).await.unwrap();
        assert_eq!(matched_body, baseline_body, "{path} if-none-match body");

        // Stale If-None-Match: same no-store 200 with a non-empty body.
        let stale = app
            .clone()
            .oneshot(
                AxumRequest::builder()
                    .uri(path)
                    .header("if-none-match", "W/\"stale\"")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(stale.status(), StatusCode::OK, "{path} stale");
        assert_eq!(
            stale.headers()["cache-control"],
            "no-store",
            "{path} stale cache-control"
        );
        assert!(
            stale.headers().get("etag").is_none(),
            "{path} stale must not carry an ETag"
        );
        let stale_body = to_bytes(stale.into_body(), usize::MAX).await.unwrap();
        assert_eq!(stale_body, baseline_body, "{path} stale body");
    }

    // gzip + If-None-Match must still return 200 with a body, never 304.
    let gz_matched = app
        .clone()
        .oneshot(
            AxumRequest::builder()
                .uri("/app.js")
                .header("accept-encoding", "gzip")
                .header("if-none-match", &etag)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        gz_matched.status(),
        StatusCode::OK,
        "/app.js gzip+if-none-match"
    );
    assert_eq!(
        gz_matched.headers()["cache-control"],
        "no-store",
        "/app.js gzip+if-none-match cache-control"
    );
    assert!(
        gz_matched.headers().get("etag").is_none(),
        "/app.js gzip+if-none-match must not carry an ETag"
    );
    assert_eq!(gz_matched.headers()["content-encoding"], "gzip");
    let gz_matched_body = to_bytes(gz_matched.into_body(), usize::MAX).await.unwrap();
    assert!(
        !gz_matched_body.is_empty(),
        "/app.js gzip+if-none-match body must not be empty"
    );

    let gz_ok = app
        .clone()
        .oneshot(
            AxumRequest::builder()
                .uri("/app.js")
                .header("accept-encoding", "gzip")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(gz_ok.status(), StatusCode::OK, "/app.js gzip 200");
    assert_eq!(gz_ok.headers()["cache-control"], "no-store");
    assert!(
        gz_ok.headers().get("etag").is_none(),
        "/app.js gzip 200 must not carry an ETag"
    );

    // The HTML shell keeps no-store and gains no ETag.
    let shell = get_public(&app, "/").await;
    assert_eq!(shell.status(), StatusCode::OK);
    assert!(
        shell.headers().get("etag").is_none(),
        "shell must not carry an ETag"
    );
    assert_eq!(shell.headers()["cache-control"], "no-store");
}

#[tokio::test]
async fn shell_uses_bare_static_asset_urls() {
    let context = CommandContext::new(Config::default(), InMemoryRegistry::default());
    let (_state, _cookie, app) =
        app_with(context, TestBridge::default(), "axum-static-version-urls");

    let shell_body = to_bytes(get_public(&app, "/").await.into_body(), usize::MAX)
        .await
        .unwrap();
    let shell = std::str::from_utf8(&shell_body).unwrap();
    assert!(
        shell.contains("src=\"/app.js\""),
        "shell must load app.js at a bare URL"
    );
    assert!(
        shell.contains("href=\"/app.css\""),
        "shell must load app.css at a bare URL"
    );
    assert!(
        !shell.contains("src=\"/app.js?"),
        "shell must not cache-bust app.js with a query string"
    );
    assert!(
        !shell.contains("href=\"/app.css?"),
        "shell must not cache-bust app.css with a query string"
    );

    let app_js_body = to_bytes(get_public(&app, "/app.js").await.into_body(), usize::MAX)
        .await
        .unwrap();
    let app_js = std::str::from_utf8(&app_js_body).unwrap();
    assert!(
        app_js.contains("import(\"./terminal.js\")"),
        "app.js must keep the deferred terminal.js import at a bare URL"
    );
    for versioned_edge in [
        "\"./app.js?v=",
        "\"./terminal.js?v=",
        "import(\"./terminal.js?v=",
    ] {
        assert!(
            !app_js.contains(versioned_edge),
            "served app.js must not rewrite module edges with {versioned_edge}"
        );
    }
}

#[tokio::test]
async fn axum_api_routes_require_browser_session_cookie_except_health() {
    let context = CommandContext::new(Config::default(), InMemoryRegistry::default());
    let state = super::WebAppState::new(
        context,
        OkRunner,
        TestBridge::default(),
        scratch_dir("axum-api-session"),
    );
    let app = super::axum_app(state);

    let shell = get_public(&app, "/").await;
    let session_cookie = set_cookie_pair(&shell);
    assert!(session_cookie.starts_with("ajax_browser_session="));

    assert_eq!(
        get_public(&app, "/api/health").await.status(),
        StatusCode::OK
    );
    assert_eq!(
        get_public(&app, "/api/cockpit").await.status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        get(&app, &session_cookie, "/api/cockpit").await.status(),
        StatusCode::OK
    );
}

#[tokio::test]
async fn axum_browser_session_renewal_bootstraps_api_access() {
    let context = CommandContext::new(Config::default(), InMemoryRegistry::default());
    let state = super::WebAppState::new(
        context,
        OkRunner,
        TestBridge::default(),
        scratch_dir("axum-session-renewal"),
    );
    let app = super::axum_app(state);

    assert_eq!(
        get_public(&app, "/api/cockpit").await.status(),
        StatusCode::UNAUTHORIZED
    );

    let renewal = app
        .clone()
        .oneshot(
            AxumRequest::builder()
                .method("POST")
                .uri("/api/session")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(renewal.status(), StatusCode::OK);
    let session_cookie = set_cookie_pair(&renewal);
    assert!(session_cookie.starts_with("ajax_browser_session="));

    assert_eq!(
        get(&app, &session_cookie, "/api/cockpit").await.status(),
        StatusCode::OK
    );
}

#[tokio::test]
async fn axum_session_renewal_response_is_cookie_json_without_shared_state() {
    let context = CommandContext::new(Config::default(), InMemoryRegistry::default());
    let state = super::WebAppState::new(
        context,
        OkRunner,
        TestBridge::default(),
        scratch_dir("axum-session-boundary"),
    );
    let response = super::browser_session_json_response(&state.browser_session);

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers()["content-type"],
        "application/json; charset=utf-8"
    );
    assert_eq!(response.headers()["cache-control"], "no-store");
    assert!(response.headers()["set-cookie"]
        .to_str()
        .unwrap()
        .starts_with("ajax_browser_session="));
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&body).unwrap(),
        serde_json::json!({ "ok": true })
    );
    let guard = state.shared();
    assert_eq!(guard.revision, 0);
    assert!(!guard.bridge.refreshed);
    assert_eq!(guard.bridge.operate_count, 0);
    assert_eq!(guard.bridge.start_count, 0);
}

#[tokio::test]
async fn cloudflare_access_enabled_rejects_missing_jwt_on_protected_routes() {
    let context = CommandContext::new(Config::default(), InMemoryRegistry::default());
    let state = super::WebAppState::new(
        context,
        OkRunner,
        TestBridge::default(),
        scratch_dir("cf-access-missing"),
    )
    .with_cloudflare_access_for_test(cloudflare_access_config_for_test(None));
    let cookie = browser_session_cookie(&state);
    let app = super::axum_app(state);

    let response = get(&app, &cookie, "/api/cockpit").await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let body = json_of(response).await;
    assert_eq!(body["ok"], false);
    assert!(body["error"]
        .as_str()
        .unwrap_or_default()
        .contains("Cloudflare Access"));
}

#[tokio::test]
async fn cloudflare_access_enabled_accepts_valid_jwt_on_protected_routes() {
    let context = CommandContext::new(Config::default(), InMemoryRegistry::default());
    let state = super::WebAppState::new(
        context,
        OkRunner,
        TestBridge::default(),
        scratch_dir("cf-access-valid"),
    )
    .with_cloudflare_access_for_test(cloudflare_access_config_for_test(None));
    let cookie = browser_session_cookie(&state);
    let app = super::axum_app(state);
    let token = cloudflare_access_token_for_test("operator@example.com");

    let response = get_with_access(&app, &cookie, "/api/cockpit", &token).await;

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn tls_listener_idle_tcp_connection_does_not_block_health_request() {
    let state = super::WebAppState::new(
        CommandContext::new(Config::default(), InMemoryRegistry::default()),
        OkRunner,
        TestBridge::default(),
        scratch_dir("tls-idle-health"),
    );
    let identity = crate::adapters::tls::load_or_create_identity(&state.state_dir).unwrap();
    let tls_config = crate::adapters::tls::tls_server_config(&identity).unwrap();
    let tcp_listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let address = tcp_listener.local_addr().unwrap();
    let (accepted_tls_tx, accepted_tls_rx) = tokio::sync::mpsc::channel(1024);
    let tls_listener = super::TlsListener {
        listener: tcp_listener,
        acceptor: tokio_rustls::TlsAcceptor::from(tls_config),
        accepted_tls_tx,
        accepted_tls_rx,
    };
    let server = tokio::spawn(async move {
        axum::serve(tls_listener, super::axum_app(state))
            .await
            .unwrap();
    });

    let idle_connection = tokio::net::TcpStream::connect(address).await.unwrap();
    let health =
        tokio::time::timeout(Duration::from_millis(500), tls_get(address, "/api/health")).await;

    drop(idle_connection);
    server.abort();

    let response = health.expect("health request timed out").unwrap();
    assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
    assert!(response.contains(r#"{"ok":true}"#), "{response}");
}

#[derive(Debug)]
struct AcceptAnyServerCert;

impl rustls::client::danger::ServerCertVerifier for AcceptAnyServerCert {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        vec![
            rustls::SignatureScheme::RSA_PKCS1_SHA256,
            rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
            rustls::SignatureScheme::RSA_PSS_SHA256,
            rustls::SignatureScheme::ED25519,
        ]
    }
}

async fn tls_get(address: std::net::SocketAddr, path: &str) -> std::io::Result<String> {
    let path = path.to_string();
    tokio::task::spawn_blocking(move || tls_get_blocking(address, &path))
        .await
        .unwrap()
}

fn tls_get_blocking(address: std::net::SocketAddr, path: &str) -> std::io::Result<String> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .unwrap()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(AcceptAnyServerCert))
        .with_no_client_auth();
    let server_name = rustls::pki_types::ServerName::try_from("localhost").unwrap();
    let connection = rustls::ClientConnection::new(Arc::new(config), server_name)
        .map_err(std::io::Error::other)?;
    let stream = std::net::TcpStream::connect(address)?;
    let mut stream = rustls::StreamOwned::new(connection, stream);
    stream.write_all(
        format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n").as_bytes(),
    )?;
    let mut response = Vec::new();
    stream.read_to_end(&mut response)?;
    Ok(String::from_utf8_lossy(&response).into_owned())
}
