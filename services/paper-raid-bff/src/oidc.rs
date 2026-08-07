use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use url::Url;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OidcProviderConfig {
    pub issuer: String,
    pub authorization_endpoint: Url,
    pub token_endpoint: Url,
    pub jwks_uri: Url,
    pub client_id: String,
    pub redirect_uri: Url,
    pub scopes: Vec<String>,
    pub clock_skew_seconds: i64,
    pub max_token_age_seconds: i64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OidcProviderConfigInput {
    issuer: String,
    authorization_endpoint: String,
    token_endpoint: String,
    jwks_uri: String,
    client_id: String,
    redirect_uri: String,
    scopes: Vec<String>,
    clock_skew_seconds: i64,
    max_token_age_seconds: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum OidcAudience {
    One(String),
    Many(Vec<String>),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerifiedOidcClaims {
    pub iss: String,
    pub sub: String,
    pub aud: OidcAudience,
    pub azp: Option<String>,
    pub exp: i64,
    pub iat: i64,
    pub nonce: String,
    pub name: Option<String>,
    pub preferred_username: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OidcIdentity {
    pub subject_id: String,
    pub display_name: String,
}

impl OidcProviderConfig {
    pub fn parse_json(raw: &str, public_origin: &Url) -> Result<Self, String> {
        let input: OidcProviderConfigInput = serde_json::from_str(raw)
            .map_err(|_| "PAPER_RAID_BFF_OIDC_CONFIG_JSON is invalid".to_string())?;
        parse_https_url("issuer", &input.issuer)?;
        let authorization_endpoint =
            parse_https_url("authorization_endpoint", &input.authorization_endpoint)?;
        let token_endpoint = parse_https_url("token_endpoint", &input.token_endpoint)?;
        let jwks_uri = parse_https_url("jwks_uri", &input.jwks_uri)?;
        validate_opaque("client_id", &input.client_id, 256)?;
        let redirect_uri =
            Url::parse(&input.redirect_uri).map_err(|_| "redirect_uri is invalid".to_string())?;
        validate_redirect_uri(&redirect_uri, public_origin)?;
        let scopes = validate_scopes(input.scopes)?;
        if !(0..=120).contains(&input.clock_skew_seconds) {
            return Err("clock_skew_seconds must be in 0..=120".to_string());
        }
        if !(60..=900).contains(&input.max_token_age_seconds) {
            return Err("max_token_age_seconds must be in 60..=900".to_string());
        }
        Ok(Self {
            issuer: input.issuer,
            authorization_endpoint,
            token_endpoint,
            jwks_uri,
            client_id: input.client_id,
            redirect_uri,
            scopes,
            clock_skew_seconds: input.clock_skew_seconds,
            max_token_age_seconds: input.max_token_age_seconds,
        })
    }

    pub fn validate_verified_claims(
        &self,
        claims: &VerifiedOidcClaims,
        expected_nonce: &str,
        now: DateTime<Utc>,
    ) -> Result<OidcIdentity, String> {
        if claims.iss != self.issuer {
            return Err("OIDC issuer mismatch".to_string());
        }
        let audiences = match &claims.aud {
            OidcAudience::One(value) => vec![value.as_str()],
            OidcAudience::Many(values) if !values.is_empty() => {
                values.iter().map(String::as_str).collect()
            }
            OidcAudience::Many(_) => return Err("OIDC audience is empty".to_string()),
        };
        if !audiences.iter().any(|audience| *audience == self.client_id) {
            return Err("OIDC audience mismatch".to_string());
        }
        let unique_audiences = audiences.iter().copied().collect::<BTreeSet<_>>();
        if unique_audiences.len() != audiences.len() {
            return Err("OIDC audiences must be unique".to_string());
        }
        if audiences.len() > 1 && claims.azp.as_deref() != Some(self.client_id.as_str()) {
            return Err("OIDC azp is required for multiple audiences".to_string());
        }
        if claims
            .azp
            .as_deref()
            .is_some_and(|azp| azp != self.client_id)
        {
            return Err("OIDC azp mismatch".to_string());
        }
        if expected_nonce.len() < 32
            || claims.nonce.len() != expected_nonce.len()
            || !bool::from(claims.nonce.as_bytes().ct_eq(expected_nonce.as_bytes()))
        {
            return Err("OIDC nonce mismatch".to_string());
        }
        validate_subject(&claims.sub)?;
        let now = now.timestamp();
        if claims.exp < now - self.clock_skew_seconds {
            return Err("OIDC token is expired".to_string());
        }
        if claims.iat > now + self.clock_skew_seconds {
            return Err("OIDC token was issued in the future".to_string());
        }
        if now - claims.iat > self.max_token_age_seconds + self.clock_skew_seconds {
            return Err("OIDC token is older than the configured maximum".to_string());
        }
        if claims.exp <= claims.iat {
            return Err("OIDC token lifetime is invalid".to_string());
        }
        if claims.exp - claims.iat > self.max_token_age_seconds + (2 * self.clock_skew_seconds) {
            return Err("OIDC token lifetime exceeds the configured maximum".to_string());
        }
        let display_name = claims
            .name
            .as_deref()
            .or(claims.preferred_username.as_deref())
            .ok_or_else(|| "OIDC display name claim is missing".to_string())?;
        validate_display_name(display_name)?;
        Ok(OidcIdentity {
            subject_id: stable_subject_id(&claims.iss, &claims.sub),
            display_name: display_name.to_string(),
        })
    }
}

fn parse_https_url(field: &str, raw: &str) -> Result<Url, String> {
    let url = Url::parse(raw).map_err(|_| format!("{field} is invalid"))?;
    if url.scheme() != "https"
        || url.cannot_be_a_base()
        || url.username() != ""
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(format!("{field} must be a plain HTTPS URL"));
    }
    Ok(url)
}

fn validate_redirect_uri(redirect_uri: &Url, public_origin: &Url) -> Result<(), String> {
    let expected = public_origin
        .join("oidc/callback")
        .map_err(|_| "public origin cannot form an OIDC callback".to_string())?;
    if redirect_uri != &expected {
        return Err("redirect_uri must equal the public origin OIDC callback".to_string());
    }
    let loopback = matches!(
        redirect_uri.host_str(),
        Some("localhost" | "127.0.0.1" | "::1")
    );
    if redirect_uri.scheme() != "https" && !(redirect_uri.scheme() == "http" && loopback) {
        return Err("redirect_uri must use HTTPS except on loopback".to_string());
    }
    Ok(())
}

fn validate_scopes(scopes: Vec<String>) -> Result<Vec<String>, String> {
    if scopes.is_empty() || scopes.len() > 16 {
        return Err("OIDC scopes must contain 1..=16 entries".to_string());
    }
    let mut unique = BTreeSet::new();
    for scope in &scopes {
        if scope.is_empty()
            || scope.len() > 64
            || scope.bytes().any(|byte| {
                !(byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-'))
            })
            || !unique.insert(scope.clone())
        {
            return Err("OIDC scopes must be unique opaque ASCII tokens".to_string());
        }
    }
    if !unique.contains("openid") {
        return Err("OIDC scopes must include openid".to_string());
    }
    Ok(scopes)
}

fn validate_subject(subject: &str) -> Result<(), String> {
    if subject.is_empty()
        || subject.len() > 255
        || subject.bytes().any(|byte| byte.is_ascii_control())
    {
        return Err("OIDC subject is invalid".to_string());
    }
    Ok(())
}

fn validate_opaque(field: &str, value: &str, max: usize) -> Result<(), String> {
    if value.is_empty()
        || value.len() > max
        || value.bytes().any(|byte| {
            !(byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-'))
        })
    {
        return Err(format!("{field} must be an opaque ASCII identifier"));
    }
    Ok(())
}

fn validate_display_name(value: &str) -> Result<(), String> {
    if value.trim().is_empty() || value.chars().count() > 80 || value.chars().any(char::is_control)
    {
        return Err("OIDC display name must contain 1..=80 printable characters".to_string());
    }
    Ok(())
}

fn stable_subject_id(issuer: &str, subject: &str) -> String {
    let issuer_hash = hex::encode(Sha256::digest(issuer.as_bytes()));
    let subject_hash = hex::encode(Sha256::digest(subject.as_bytes()));
    format!("oidc:{}:{}", &issuer_hash[..16], &subject_hash[..32])
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    fn config() -> OidcProviderConfig {
        OidcProviderConfig::parse_json(
            &serde_json::json!({
                "issuer":"https://identity.example.test/tenant-a",
                "authorization_endpoint":"https://identity.example.test/tenant-a/authorize",
                "token_endpoint":"https://identity.example.test/tenant-a/token",
                "jwks_uri":"https://identity.example.test/tenant-a/keys",
                "client_id":"paper-raid-beta",
                "redirect_uri":"https://research.example.test/oidc/callback",
                "scopes":["openid","profile"],
                "clock_skew_seconds":60,
                "max_token_age_seconds":600
            })
            .to_string(),
            &Url::parse("https://research.example.test/").unwrap(),
        )
        .unwrap()
    }

    fn claims() -> VerifiedOidcClaims {
        VerifiedOidcClaims {
            iss: "https://identity.example.test/tenant-a".into(),
            sub: "user-123".into(),
            aud: OidcAudience::One("paper-raid-beta".into()),
            azp: None,
            exp: 1_700,
            iat: 1_000,
            nonce: "n".repeat(32),
            name: Some("Researcher One".into()),
            preferred_username: None,
        }
    }

    #[test]
    fn provider_config_is_https_and_callback_bound() {
        let config = config();
        assert_eq!(config.scopes, ["openid", "profile"]);
        let invalid = serde_json::json!({
            "issuer":"http://identity.example.test",
            "authorization_endpoint":"https://identity.example.test/authorize",
            "token_endpoint":"https://identity.example.test/token",
            "jwks_uri":"https://identity.example.test/keys",
            "client_id":"paper-raid-beta",
            "redirect_uri":"https://attacker.example.test/callback",
            "scopes":["openid"],
            "clock_skew_seconds":60,
            "max_token_age_seconds":600
        });
        assert!(OidcProviderConfig::parse_json(
            &invalid.to_string(),
            &Url::parse("https://research.example.test/").unwrap()
        )
        .is_err());
    }

    #[test]
    fn verified_claims_bind_issuer_audience_nonce_and_time() {
        let now = Utc.timestamp_opt(1_500, 0).unwrap();
        let config = config();
        let identity = config
            .validate_verified_claims(&claims(), &"n".repeat(32), now)
            .unwrap();
        assert!(identity.subject_id.starts_with("oidc:"));
        assert!(!identity.subject_id.contains("user-123"));
        assert_eq!(identity.display_name, "Researcher One");

        let mut issuer_mismatch = claims();
        issuer_mismatch.iss = "https://identity.example.test/tenant-b".into();
        assert!(config
            .validate_verified_claims(&issuer_mismatch, &"n".repeat(32), now)
            .is_err());

        let mut audience_mismatch = claims();
        audience_mismatch.aud = OidcAudience::One("another-client".into());
        assert!(config
            .validate_verified_claims(&audience_mismatch, &"n".repeat(32), now)
            .is_err());

        assert!(config
            .validate_verified_claims(&claims(), &"x".repeat(32), now)
            .is_err());
    }

    #[test]
    fn multiple_audiences_require_matching_authorized_party() {
        let now = Utc.timestamp_opt(1_500, 0).unwrap();
        let config = config();
        let mut claims = claims();
        claims.aud = OidcAudience::Many(vec!["paper-raid-beta".into(), "api".into()]);
        assert!(config
            .validate_verified_claims(&claims, &"n".repeat(32), now)
            .is_err());
        claims.azp = Some("paper-raid-beta".into());
        assert!(config
            .validate_verified_claims(&claims, &"n".repeat(32), now)
            .is_ok());
        claims.aud = OidcAudience::Many(vec!["paper-raid-beta".into(), "paper-raid-beta".into()]);
        assert!(config
            .validate_verified_claims(&claims, &"n".repeat(32), now)
            .is_err());
    }

    #[test]
    fn stale_future_and_control_character_claims_fail_closed() {
        let now = Utc.timestamp_opt(1_500, 0).unwrap();
        let config = config();

        let mut stale = claims();
        stale.iat = 700;
        assert!(config
            .validate_verified_claims(&stale, &"n".repeat(32), now)
            .is_err());

        let mut future = claims();
        future.iat = 1_700;
        assert!(config
            .validate_verified_claims(&future, &"n".repeat(32), now)
            .is_err());

        let mut excessive_lifetime = claims();
        excessive_lifetime.exp = 1_800;
        assert!(config
            .validate_verified_claims(&excessive_lifetime, &"n".repeat(32), now)
            .is_err());

        let mut control = claims();
        control.name = Some("Researcher\nAdmin".into());
        assert!(config
            .validate_verified_claims(&control, &"n".repeat(32), now)
            .is_err());
    }
}
