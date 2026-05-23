//! Web companion runtime wiring.

use ajax_core::{commands::CommandContext, registry::Registry};

use crate::slices::{cockpit, install};

pub struct Request<'a> {
    pub method: &'a str,
    pub path: &'a str,
    pub body: &'a str,
}

pub struct Response {
    pub status_code: u16,
    pub content_type: &'static str,
    pub body: Vec<u8>,
}

#[derive(Debug)]
pub enum RouteError {
    Json(serde_json::Error),
}

pub fn route<R: Registry>(
    request: Request<'_>,
    context: &CommandContext<R>,
) -> Result<Response, RouteError> {
    let path = request.path.split('?').next().unwrap_or(request.path);
    match (request.method, path) {
        ("GET", "/") => Ok(Response {
            status_code: 200,
            content_type: "text/html; charset=utf-8",
            body: install::pwa_shell().as_bytes().to_vec(),
        }),
        ("GET", "/api/cockpit") => Ok(Response {
            status_code: 200,
            content_type: "application/json; charset=utf-8",
            body: cockpit::browser_cockpit_json(context)
                .map_err(RouteError::Json)?
                .into_bytes(),
        }),
        ("GET", asset_path) => match install::static_asset(asset_path) {
            Some(asset) => Ok(Response {
                status_code: 200,
                content_type: asset.content_type,
                body: asset.body.to_vec(),
            }),
            None => Ok(text_response(404, "not found")),
        },
        _ => Ok(text_response(405, "method not allowed")),
    }
}

fn text_response(status_code: u16, body: &str) -> Response {
    Response {
        status_code,
        content_type: "text/plain; charset=utf-8",
        body: body.as_bytes().to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use super::{route, Request};
    use ajax_core::{commands::CommandContext, config::Config, registry::InMemoryRegistry};

    #[test]
    fn runtime_routes_to_vertical_slices() {
        let context = CommandContext::new(Config::default(), InMemoryRegistry::default());

        let shell = route(
            Request {
                method: "GET",
                path: "/",
                body: "",
            },
            &context,
        )
        .unwrap();
        assert_eq!(shell.status_code, 200);
        assert_eq!(shell.content_type, "text/html; charset=utf-8");
        assert!(std::str::from_utf8(&shell.body)
            .unwrap()
            .contains("Ajax Cockpit"));

        let cockpit = route(
            Request {
                method: "GET",
                path: "/api/cockpit",
                body: "",
            },
            &context,
        )
        .unwrap();
        assert_eq!(cockpit.status_code, 200);
        assert_eq!(cockpit.content_type, "application/json; charset=utf-8");
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&cockpit.body).unwrap()["cards"],
            serde_json::json!([])
        );
    }
}
