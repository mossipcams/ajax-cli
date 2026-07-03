use axum::http::HeaderMap;
use jsonwebtoken::{
    decode, decode_header,
    jwk::{AlgorithmParameters, JwkSet, KeyAlgorithm},
    Algorithm, DecodingKey, Validation,
};
use serde::Deserialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    sync::Arc,
};

use crate::WebError;

const ACCESS_JWT_HEADER: &str = "cf-access-jwt-assertion";

#[derive(Clone)]
pub(crate) struct CloudflareAccessConfig {
    issuer: Arc<str>,
    audience: Arc<str>,
    keys: Arc<BTreeMap<String, AccessKey>>,
    allowed_emails: Option<Arc<BTreeSet<String>>>,
}

#[derive(Clone)]
struct AccessKey {
    algorithm: Algorithm,
    decoding_key: DecodingKey,
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum CloudflareAccessError {
    MissingToken,
    InvalidToken,
    Forbidden,
}

#[derive(Debug, Deserialize)]
struct CloudflareAccessClaims {
    email: Option<String>,
    #[serde(rename = "type")]
    token_type: Option<String>,
}

impl CloudflareAccessConfig {
    pub(crate) fn from_env() -> Result<Option<Self>, WebError> {
        let issuer = optional_env("AJAX_CF_ACCESS_ISSUER")?;
        let audience = optional_env("AJAX_CF_ACCESS_AUD")?;
        let jwks = optional_env("AJAX_CF_ACCESS_JWKS")?;
        let jwks_file = optional_env("AJAX_CF_ACCESS_JWKS_FILE")?;
        if issuer.is_none() && audience.is_none() && jwks.is_none() && jwks_file.is_none() {
            return Ok(None);
        }
        let issuer = issuer.ok_or_else(|| {
            WebError::CommandFailed(
                "Cloudflare Access config requires AJAX_CF_ACCESS_ISSUER".to_string(),
            )
        })?;
        let audience = audience.ok_or_else(|| {
            WebError::CommandFailed(
                "Cloudflare Access config requires AJAX_CF_ACCESS_AUD".to_string(),
            )
        })?;
        let jwks_json = match (jwks, jwks_file) {
            (Some(json), _) => json,
            (None, Some(path)) => fs::read_to_string(&path).map_err(|error| {
                WebError::CommandFailed(format!(
                    "failed to read AJAX_CF_ACCESS_JWKS_FILE {path}: {error}"
                ))
            })?,
            (None, None) => {
                return Err(WebError::CommandFailed(
                    "Cloudflare Access config requires AJAX_CF_ACCESS_JWKS or AJAX_CF_ACCESS_JWKS_FILE"
                        .to_string(),
                ));
            }
        };
        let allowed_emails = optional_env("AJAX_CF_ACCESS_ALLOWED_EMAILS")?
            .and_then(|value| parse_allowed_emails(&value));
        Self::from_jwks_json(issuer, audience, &jwks_json, allowed_emails).map(Some)
    }

    fn from_jwks_json(
        issuer: String,
        audience: String,
        jwks_json: &str,
        allowed_emails: Option<BTreeSet<String>>,
    ) -> Result<Self, WebError> {
        let jwks: JwkSet = serde_json::from_str(jwks_json).map_err(|error| {
            WebError::CommandFailed(format!("invalid Cloudflare Access JWKS JSON: {error}"))
        })?;
        let mut keys = BTreeMap::new();
        for jwk in jwks.keys {
            let Some(kid) = jwk.common.key_id.clone() else {
                continue;
            };
            let Some(algorithm) = jwk_algorithm(&jwk) else {
                continue;
            };
            let decoding_key = DecodingKey::from_jwk(&jwk).map_err(|error| {
                WebError::CommandFailed(format!(
                    "invalid Cloudflare Access JWKS key {kid}: {error}"
                ))
            })?;
            keys.insert(
                kid,
                AccessKey {
                    algorithm,
                    decoding_key,
                },
            );
        }
        if keys.is_empty() {
            return Err(WebError::CommandFailed(
                "Cloudflare Access JWKS did not contain a usable RS256 key".to_string(),
            ));
        }
        Ok(Self {
            issuer: Arc::from(issuer),
            audience: Arc::from(audience),
            keys: Arc::new(keys),
            allowed_emails: allowed_emails.map(Arc::new),
        })
    }

    #[cfg(test)]
    pub(crate) fn hmac_for_test(
        issuer: impl Into<String>,
        audience: impl Into<String>,
        secret: &[u8],
        allowed_emails: Option<BTreeSet<String>>,
    ) -> Self {
        let mut keys = BTreeMap::new();
        keys.insert(
            "test-key".to_string(),
            AccessKey {
                algorithm: Algorithm::HS256,
                decoding_key: DecodingKey::from_secret(secret),
            },
        );
        Self {
            issuer: Arc::from(issuer.into()),
            audience: Arc::from(audience.into()),
            keys: Arc::new(keys),
            allowed_emails: allowed_emails.map(Arc::new),
        }
    }

    pub(crate) fn verify_headers(&self, headers: &HeaderMap) -> Result<(), CloudflareAccessError> {
        let token = headers
            .get(ACCESS_JWT_HEADER)
            .ok_or(CloudflareAccessError::MissingToken)?
            .to_str()
            .map_err(|_| CloudflareAccessError::InvalidToken)?;
        let header = decode_header(token).map_err(|_| CloudflareAccessError::InvalidToken)?;
        let kid = header
            .kid
            .as_deref()
            .ok_or(CloudflareAccessError::InvalidToken)?;
        let key = self
            .keys
            .get(kid)
            .ok_or(CloudflareAccessError::InvalidToken)?;
        if header.alg != key.algorithm {
            return Err(CloudflareAccessError::InvalidToken);
        }

        let mut validation = Validation::new(key.algorithm);
        validation.validate_nbf = true;
        validation.set_audience(&[self.audience.as_ref()]);
        validation.set_issuer(&[self.issuer.as_ref()]);
        validation.set_required_spec_claims(&["exp", "nbf", "iss", "aud"]);
        let token = decode::<CloudflareAccessClaims>(token, &key.decoding_key, &validation)
            .map_err(|_| CloudflareAccessError::InvalidToken)?;
        if token.claims.token_type.as_deref() != Some("app") {
            return Err(CloudflareAccessError::InvalidToken);
        }
        if let Some(allowed_emails) = self.allowed_emails.as_ref() {
            let email = token
                .claims
                .email
                .as_deref()
                .ok_or(CloudflareAccessError::Forbidden)?
                .to_ascii_lowercase();
            if !allowed_emails.contains(&email) {
                return Err(CloudflareAccessError::Forbidden);
            }
        }
        Ok(())
    }
}

impl CloudflareAccessError {
    pub(crate) fn status_code(&self) -> u16 {
        match self {
            Self::MissingToken | Self::InvalidToken => 401,
            Self::Forbidden => 403,
        }
    }

    pub(crate) fn client_message(&self) -> &'static str {
        match self {
            Self::MissingToken => "Cloudflare Access token required",
            Self::InvalidToken => "Cloudflare Access token invalid",
            Self::Forbidden => "Cloudflare Access identity not allowed",
        }
    }
}

fn optional_env(name: &str) -> Result<Option<String>, WebError> {
    match env::var(name) {
        Ok(value) if value.trim().is_empty() => Ok(None),
        Ok(value) => Ok(Some(value)),
        Err(env::VarError::NotPresent) => Ok(None),
        Err(env::VarError::NotUnicode(_)) => Err(WebError::CommandFailed(format!(
            "{name} must contain valid unicode"
        ))),
    }
}

fn parse_allowed_emails(value: &str) -> Option<BTreeSet<String>> {
    let emails = value
        .split(',')
        .map(|email| email.trim().to_ascii_lowercase())
        .filter(|email| !email.is_empty())
        .collect::<BTreeSet<_>>();
    (!emails.is_empty()).then_some(emails)
}

fn jwk_algorithm(jwk: &jsonwebtoken::jwk::Jwk) -> Option<Algorithm> {
    match jwk.common.key_algorithm {
        Some(KeyAlgorithm::RS256) => Some(Algorithm::RS256),
        Some(_) => None,
        None => match jwk.algorithm {
            AlgorithmParameters::RSA(_) => Some(Algorithm::RS256),
            _ => None,
        },
    }
}
