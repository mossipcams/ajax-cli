//! HTTP request and response transport mechanisms.

use crate::WebError;
use axum::{
    http::{header, HeaderName, HeaderValue, StatusCode},
    response::{IntoResponse, Response as AxumResponse},
};

const CONTENT_SECURITY_POLICY: &str = concat!(
    "default-src 'self'; ",
    "base-uri 'none'; ",
    "frame-ancestors 'none'; ",
    "form-action 'none'; ",
    "object-src 'none'; ",
    "connect-src 'self' ws: wss:; ",
    "script-src 'self' 'wasm-unsafe-eval'; ",
    "style-src 'self' 'unsafe-inline'; ",
    "img-src 'self' data:; ",
    "font-src 'self' data:"
);

#[derive(Clone)]
pub struct Response {
    pub status_code: u16,
    pub content_type: &'static str,
    pub body: Vec<u8>,
}

impl Response {
    pub(crate) fn into_axum_response(self) -> AxumResponse {
        bytes_axum_response(self.status_code, self.content_type, self.body)
    }
}

pub fn json_response(status_code: u16, value: serde_json::Value) -> Result<Response, WebError> {
    Ok(Response {
        status_code,
        content_type: "application/json; charset=utf-8",
        body: serde_json::to_vec(&value)
            .map_err(|error| WebError::JsonSerialization(error.to_string()))?,
    })
}

pub fn response_from_web_error(error: WebError, request_id: Option<&str>) -> Response {
    let mut value = serde_json::json!({
        "ok": false,
        "error": error.to_string(),
    });
    if let Some(request_id) = request_id {
        value["request_id"] = serde_json::Value::String(request_id.to_string());
    }
    Response {
        status_code: 500,
        content_type: "application/json; charset=utf-8",
        body: serde_json::to_vec(&value).unwrap_or_else(|_| b"{\"ok\":false}".to_vec()),
    }
}

pub fn operation_response_with_request_id(
    mut response: Response,
    request_id: Option<&str>,
) -> Response {
    let Some(request_id) = request_id else {
        return response;
    };
    if response.content_type != "application/json; charset=utf-8" {
        return response;
    }
    let Ok(mut value) = serde_json::from_slice::<serde_json::Value>(&response.body) else {
        return response;
    };
    value["request_id"] = serde_json::Value::String(request_id.to_string());
    if let Ok(body) = serde_json::to_vec(&value) {
        response.body = body;
    }
    response
}

pub fn html_response(body: Vec<u8>) -> AxumResponse {
    bytes_axum_response(200, "text/html; charset=utf-8", body)
}

pub fn text_axum_response(status_code: u16, body: &str) -> AxumResponse {
    bytes_axum_response(
        status_code,
        "text/plain; charset=utf-8",
        body.as_bytes().to_vec(),
    )
}

pub fn json_value_response(status_code: u16, value: serde_json::Value) -> AxumResponse {
    match serde_json::to_vec(&value) {
        Ok(body) => bytes_axum_response(status_code, "application/json; charset=utf-8", body),
        Err(error) => web_error_response(WebError::JsonSerialization(error.to_string())),
    }
}

pub fn bytes_axum_response(
    status_code: u16,
    content_type: &'static str,
    body: Vec<u8>,
) -> AxumResponse {
    let status = StatusCode::from_u16(status_code).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let mut response = (status, body).into_response();
    response
        .headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    apply_no_store(&mut response);
    apply_security_headers(&mut response);
    response
}

/// Version-busted static shell asset response (JS/CSS). These filenames are
/// content-fingerprinted via `?v=<app_version()>`, so they can be long-cached
/// by browsers/CDNs as immutable. The HTML shell and `/api/*` keep `no-store`.
pub fn static_bytes_axum_response(content_type: &'static str, body: Vec<u8>) -> AxumResponse {
    let mut response = (StatusCode::OK, body).into_response();
    response
        .headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    apply_immutable_cache(&mut response);
    apply_security_headers(&mut response);
    response
}

pub fn apply_immutable_cache(response: &mut AxumResponse) {
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=31536000, immutable"),
    );
}

pub fn web_error_response(error: WebError) -> AxumResponse {
    json_value_response(
        500,
        serde_json::json!({
            "ok": false,
            "error": error.to_string(),
        }),
    )
}

pub fn apply_no_store(response: &mut AxumResponse) {
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
}

fn apply_security_headers(response: &mut AxumResponse) {
    let headers = response.headers_mut();
    headers.insert(
        HeaderName::from_static("x-content-type-options"),
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        HeaderName::from_static("referrer-policy"),
        HeaderValue::from_static("no-referrer"),
    );
    headers.insert(
        HeaderName::from_static("permissions-policy"),
        HeaderValue::from_static("camera=(), microphone=(), geolocation=()"),
    );
    headers.insert(
        HeaderName::from_static("content-security-policy"),
        HeaderValue::from_static(CONTENT_SECURITY_POLICY),
    );
}

#[cfg(test)]
mod tests {
    use super::{json_response, json_value_response};

    #[test]
    fn http_adapter_builds_json_responses() {
        let json = json_response(200, serde_json::json!({ "ok": true })).unwrap();
        assert_eq!(json.status_code, 200);
        assert_eq!(json.content_type, "application/json; charset=utf-8");
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&json.body).unwrap(),
            serde_json::json!({ "ok": true })
        );
    }

    #[test]
    fn axum_responses_include_security_headers() {
        let response = json_value_response(200, serde_json::json!({ "ok": true }));

        assert_eq!(response.headers()["x-content-type-options"], "nosniff");
        assert_eq!(response.headers()["referrer-policy"], "no-referrer");
        assert_eq!(
            response.headers()["permissions-policy"],
            "camera=(), microphone=(), geolocation=()"
        );
        assert!(response.headers()["content-security-policy"]
            .to_str()
            .unwrap()
            .contains("frame-ancestors 'none'"));
    }
}
