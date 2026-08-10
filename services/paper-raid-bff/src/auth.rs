use std::{sync::Arc, time::Duration};

use axum::http::{header, HeaderMap, HeaderValue};
use base64::{
    engine::general_purpose::{STANDARD as BASE64, URL_SAFE_NO_PAD},
    Engine as _,
};
use chrono::{DateTime, Utc};
use rand::{rngs::OsRng, RngCore};
use ring::{
    aead::{self, Aad, LessSafeKey, Nonce, UnboundKey},
    rand::{SecureRandom, SystemRandom},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};
use subtle::ConstantTimeEq;
use uuid::Uuid;

use crate::{
    access,
    config::{AlphaIdentity, DurableQuotaConfig},
    error::AppError,
};

pub const SESSION_COOKIE: &str = "paper_raid_session";
pub const CSRF_HEADER: &str = "x-paper-raid-csrf";
const COOKIE_AAD: &[u8] = b"paper-raid-bff/session-cookie/v1";

#[derive(Clone)]
pub struct SessionStore {
    pool: PgPool,
    cipher: Arc<LessSafeKey>,
    ttl: Duration,
    public_origin: String,
    mutation_quota: Option<DurableQuotaConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SessionTicket {
    schema: String,
    session_id: Uuid,
    subject_id: String,
    generation: i64,
    expires_at_unix: i64,
}

#[derive(Clone)]
pub struct AuthenticatedSession {
    pub session_id: Uuid,
    pub identity: AlphaIdentity,
}

pub struct SessionIssue {
    pub cookie: HeaderValue,
    pub csrf: String,
}

impl SessionStore {
    pub fn new(
        pool: PgPool,
        key_bytes: &[u8; 32],
        ttl: Duration,
        public_origin: &url::Url,
        mutation_quota: Option<DurableQuotaConfig>,
    ) -> Result<Self, String> {
        let key = UnboundKey::new(&aead::AES_256_GCM, key_bytes)
            .map_err(|_| "session key is not valid for AES-256-GCM".to_string())?;
        Ok(Self {
            pool,
            cipher: Arc::new(LessSafeKey::new(key)),
            ttl,
            public_origin: public_origin.as_str().trim_end_matches('/').to_string(),
            mutation_quota,
        })
    }

    pub async fn issue(&self, identity: &AlphaIdentity) -> Result<SessionIssue, AppError> {
        self.issue_inner(identity, None).await
    }

    pub async fn issue_at_generation(
        &self,
        identity: &AlphaIdentity,
        expected_generation: i64,
    ) -> Result<SessionIssue, AppError> {
        self.issue_inner(identity, Some(expected_generation)).await
    }

    async fn issue_inner(
        &self,
        identity: &AlphaIdentity,
        expected_generation: Option<i64>,
    ) -> Result<SessionIssue, AppError> {
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "INSERT INTO paper_raid_bff_session_generation(subject_id, generation) \
             VALUES ($1, 0) ON CONFLICT (subject_id) DO NOTHING",
        )
        .bind(&identity.subject_id)
        .execute(&mut *tx)
        .await?;
        let generation: i64 = sqlx::query_scalar(
            "SELECT generation FROM paper_raid_bff_session_generation WHERE subject_id = $1 FOR UPDATE",
        )
        .bind(&identity.subject_id)
        .fetch_one(&mut *tx)
        .await?;
        if expected_generation.is_some_and(|expected| expected != generation) {
            return Err(AppError::Unauthorized);
        }

        let session_id = Uuid::new_v4();
        let csrf = random_secret();
        let csrf_hash = digest(csrf.as_bytes());
        let expires_at =
            Utc::now() + chrono::Duration::from_std(self.ttl).map_err(|_| AppError::Internal)?;
        sqlx::query(
            "INSERT INTO paper_raid_bff_sessions \
             (session_id, subject_id, generation, csrf_hash, expires_at) \
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(session_id)
        .bind(&identity.subject_id)
        .bind(generation)
        .bind(csrf_hash.as_slice())
        .bind(expires_at)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;

        let ticket = SessionTicket {
            schema: "paper-raid-bff.session.v1".into(),
            session_id,
            subject_id: identity.subject_id.clone(),
            generation,
            expires_at_unix: expires_at.timestamp(),
        };
        Ok(SessionIssue {
            cookie: self.cookie_for_ticket(&ticket)?,
            csrf,
        })
    }

    pub async fn authenticate(
        &self,
        headers: &HeaderMap,
        identity_lookup: impl FnOnce(&str) -> Option<AlphaIdentity>,
    ) -> Result<AuthenticatedSession, AppError> {
        let (session_id, subject_id) = self.authenticate_subject(headers).await?;
        let identity = identity_lookup(&subject_id).ok_or(AppError::Unauthorized)?;
        Ok(AuthenticatedSession {
            session_id,
            identity,
        })
    }

    pub async fn authenticate_subject(
        &self,
        headers: &HeaderMap,
    ) -> Result<(Uuid, String), AppError> {
        let encrypted = cookie_value(headers, SESSION_COOKIE).ok_or(AppError::Unauthorized)?;
        let ticket = self.open_ticket(encrypted)?;
        if ticket.schema != "paper-raid-bff.session.v1"
            || ticket.expires_at_unix <= Utc::now().timestamp()
        {
            return Err(AppError::Unauthorized);
        }
        let row = sqlx::query(
            "SELECT s.generation, s.csrf_hash, s.expires_at, s.revoked_at, g.generation AS current_generation \
             FROM paper_raid_bff_sessions s \
             JOIN paper_raid_bff_session_generation g ON g.subject_id = s.subject_id \
             WHERE s.session_id = $1 AND s.subject_id = $2",
        )
        .bind(ticket.session_id)
        .bind(&ticket.subject_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(AppError::Unauthorized)?;

        let generation: i64 = row.try_get("generation").map_err(|_| AppError::Internal)?;
        let current_generation: i64 = row
            .try_get("current_generation")
            .map_err(|_| AppError::Internal)?;
        let expires_at: DateTime<Utc> =
            row.try_get("expires_at").map_err(|_| AppError::Internal)?;
        let revoked_at: Option<DateTime<Utc>> =
            row.try_get("revoked_at").map_err(|_| AppError::Internal)?;
        if generation != ticket.generation
            || current_generation != ticket.generation
            || revoked_at.is_some()
            || expires_at <= Utc::now()
        {
            return Err(AppError::Unauthorized);
        }
        sqlx::query(
            "UPDATE paper_raid_bff_sessions SET last_seen_at = now() WHERE session_id = $1",
        )
        .bind(ticket.session_id)
        .execute(&self.pool)
        .await?;
        Ok((ticket.session_id, ticket.subject_id))
    }

    pub fn validate_origin(&self, headers: &HeaderMap) -> Result<(), AppError> {
        let origin = headers
            .get(header::ORIGIN)
            .and_then(|value| value.to_str().ok())
            .ok_or(AppError::Forbidden)?;
        if !bool::from(origin.as_bytes().ct_eq(self.public_origin.as_bytes())) {
            return Err(AppError::Forbidden);
        }
        Ok(())
    }

    pub async fn consume_csrf(
        &self,
        headers: &HeaderMap,
        session: &AuthenticatedSession,
    ) -> Result<String, AppError> {
        self.validate_origin(headers)?;
        let presented = headers
            .get(CSRF_HEADER)
            .and_then(|value| value.to_str().ok())
            .ok_or(AppError::Forbidden)?;
        if presented.is_empty() || presented.len() > 128 {
            return Err(AppError::Forbidden);
        }
        if let Some(quota) = self.mutation_quota {
            access::enforce_mutation_quota(&self.pool, quota, &session.identity.subject_id).await?;
        }

        let old_hash = digest(presented.as_bytes());
        let next_csrf = random_secret();
        let next_hash = digest(next_csrf.as_bytes());
        let mut tx = self.pool.begin().await?;
        let result = sqlx::query(
            "UPDATE paper_raid_bff_sessions SET csrf_hash = $1, last_seen_at = now() \
             WHERE session_id = $2 AND csrf_hash = $3 AND revoked_at IS NULL AND expires_at > now()",
        )
        .bind(next_hash.as_slice())
        .bind(session.session_id)
        .bind(old_hash.as_slice())
        .execute(&mut *tx)
        .await?;
        if result.rows_affected() != 1 {
            return Err(AppError::Conflict("csrf_replayed".into()));
        }
        sqlx::query("INSERT INTO paper_raid_bff_csrf_uses(session_id, token_hash) VALUES ($1, $2)")
            .bind(session.session_id)
            .bind(old_hash.as_slice())
            .execute(&mut *tx)
            .await
            .map_err(|error| {
                if error
                    .as_database_error()
                    .is_some_and(|db| db.is_unique_violation())
                {
                    AppError::Conflict("csrf_replayed".into())
                } else {
                    AppError::from(error)
                }
            })?;
        tx.commit().await?;

        Ok(next_csrf)
    }

    /// Recovers a fresh synchronizer token after a response was lost between
    /// the database rotation commit and delivery to the browser. The stable
    /// encrypted session cookie remains valid; exact Origin and SameSite
    /// protections gate this recovery operation.
    pub async fn refresh_csrf(
        &self,
        headers: &HeaderMap,
        session: &AuthenticatedSession,
    ) -> Result<String, AppError> {
        self.validate_origin(headers)?;
        let next_csrf = random_secret();
        let next_hash = digest(next_csrf.as_bytes());
        let mut tx = self.pool.begin().await?;
        let current_hash: Vec<u8> = sqlx::query_scalar(
            "SELECT csrf_hash FROM paper_raid_bff_sessions \
             WHERE session_id = $1 AND revoked_at IS NULL AND expires_at > now() FOR UPDATE",
        )
        .bind(session.session_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(AppError::Unauthorized)?;
        if current_hash.len() != 32 {
            return Err(AppError::Internal);
        }
        sqlx::query(
            "INSERT INTO paper_raid_bff_csrf_uses(session_id, token_hash) VALUES ($1, $2) \
             ON CONFLICT DO NOTHING",
        )
        .bind(session.session_id)
        .bind(&current_hash)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "UPDATE paper_raid_bff_sessions SET csrf_hash = $1, last_seen_at = now() \
             WHERE session_id = $2",
        )
        .bind(next_hash.as_slice())
        .bind(session.session_id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(next_csrf)
    }

    pub async fn revoke_subject(&self, subject_id: &str) -> Result<(), AppError> {
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "INSERT INTO paper_raid_bff_session_generation(subject_id, generation) VALUES ($1, 1) \
             ON CONFLICT (subject_id) DO UPDATE SET generation = paper_raid_bff_session_generation.generation + 1, updated_at = now()",
        )
        .bind(subject_id)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "UPDATE paper_raid_bff_sessions SET revoked_at = now() \
             WHERE subject_id = $1 AND revoked_at IS NULL",
        )
        .bind(subject_id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub fn expired_cookie(&self) -> HeaderValue {
        HeaderValue::from_static(
            "paper_raid_session=; Path=/; HttpOnly; SameSite=Strict; Max-Age=0",
        )
    }

    fn cookie_for_ticket(&self, ticket: &SessionTicket) -> Result<HeaderValue, AppError> {
        let value = self.seal_ticket(ticket)?;
        HeaderValue::from_str(&format!(
            "{SESSION_COOKIE}={value}; Path=/; HttpOnly; SameSite=Strict; Max-Age={}",
            self.ttl.as_secs()
        ))
        .map_err(|_| AppError::Internal)
    }

    fn seal_ticket(&self, ticket: &SessionTicket) -> Result<String, AppError> {
        let mut nonce_bytes = [0_u8; 12];
        SystemRandom::new()
            .fill(&mut nonce_bytes)
            .map_err(|_| AppError::Internal)?;
        let nonce = Nonce::assume_unique_for_key(nonce_bytes);
        let mut payload = serde_json::to_vec(ticket).map_err(|_| AppError::Internal)?;
        self.cipher
            .seal_in_place_append_tag(nonce, Aad::from(COOKIE_AAD), &mut payload)
            .map_err(|_| AppError::Internal)?;
        let mut encoded = nonce_bytes.to_vec();
        encoded.extend_from_slice(&payload);
        Ok(URL_SAFE_NO_PAD.encode(encoded))
    }

    fn open_ticket(&self, value: &str) -> Result<SessionTicket, AppError> {
        let mut encrypted = URL_SAFE_NO_PAD
            .decode(value)
            .map_err(|_| AppError::Unauthorized)?;
        if encrypted.len() < 12 + aead::AES_256_GCM.tag_len() {
            return Err(AppError::Unauthorized);
        }
        let nonce_bytes: [u8; 12] = encrypted[..12]
            .try_into()
            .map_err(|_| AppError::Unauthorized)?;
        let plaintext = self
            .cipher
            .open_in_place(
                Nonce::assume_unique_for_key(nonce_bytes),
                Aad::from(COOKIE_AAD),
                &mut encrypted[12..],
            )
            .map_err(|_| AppError::Unauthorized)?;
        serde_json::from_slice(plaintext).map_err(|_| AppError::Unauthorized)
    }
}

fn random_secret() -> String {
    let mut bytes = [0_u8; 32];
    OsRng.fill_bytes(&mut bytes);
    BASE64.encode(bytes)
}

fn digest(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

fn cookie_value<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|cookies| cookies.split(';'))
        .filter_map(|cookie| cookie.trim().split_once('='))
        .find_map(|(key, value)| (key == name).then_some(value))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::AlphaIdentity, db};
    use url::Url;

    #[tokio::test]
    async fn real_postgres_lost_rotation_refresh_restart_and_revoke() {
        let Ok(database_url) = std::env::var("PAPER_RAID_BFF_TEST_DATABASE_URL") else {
            eprintln!("PAPER_RAID_BFF_TEST_DATABASE_URL is unset; real PostgreSQL gate skipped");
            return;
        };
        let pool = db::connect(&database_url)
            .await
            .expect("connect test PostgreSQL");
        db::migrate(&pool).await.expect("migrate test PostgreSQL");
        let subject = format!("auth-test-{}", Uuid::new_v4());
        let identity = AlphaIdentity::test_identity(&subject, Uuid::new_v4(), Uuid::new_v4());
        let origin = Url::parse("http://127.0.0.1:7020").expect("origin");
        let key = [23_u8; 32];
        let store = SessionStore::new(pool.clone(), &key, Duration::from_secs(3600), &origin, None)
            .expect("session store");
        let first = store.issue(&identity).await.expect("issue session");
        let cookie = first
            .cookie
            .to_str()
            .expect("cookie header")
            .split(';')
            .next()
            .expect("cookie pair")
            .to_string();
        let mut auth_headers = HeaderMap::new();
        auth_headers.insert(
            header::COOKIE,
            HeaderValue::from_str(&cookie).expect("cookie"),
        );
        let session = store
            .authenticate(&auth_headers, |_| Some(identity.clone()))
            .await
            .expect("authenticate");

        let mut mutation_headers = auth_headers.clone();
        mutation_headers.insert(
            header::ORIGIN,
            HeaderValue::from_static("http://127.0.0.1:7020"),
        );
        mutation_headers.insert(
            CSRF_HEADER,
            HeaderValue::from_str(&first.csrf).expect("csrf"),
        );
        let lost_next = store
            .consume_csrf(&mutation_headers, &session)
            .await
            .expect("commit rotation before simulated response loss");
        assert_ne!(lost_next, first.csrf);

        let restarted =
            SessionStore::new(pool.clone(), &key, Duration::from_secs(3600), &origin, None)
                .expect("restart session store");
        let recovered_session = restarted
            .authenticate(&auth_headers, |_| Some(identity.clone()))
            .await
            .expect("stable cookie survives lost CSRF response");
        assert!(matches!(
            restarted
                .consume_csrf(&mutation_headers, &recovered_session)
                .await,
            Err(AppError::Conflict(_))
        ));
        let recovered_csrf = restarted
            .refresh_csrf(&mutation_headers, &recovered_session)
            .await
            .expect("same-origin refresh recovers CSRF");
        mutation_headers.insert(
            CSRF_HEADER,
            HeaderValue::from_str(&recovered_csrf).expect("recovered csrf"),
        );
        restarted
            .consume_csrf(&mutation_headers, &recovered_session)
            .await
            .expect("recovered csrf is accepted once");

        restarted
            .revoke_subject(&identity.subject_id)
            .await
            .expect("revoke subject");
        assert!(matches!(
            restarted
                .authenticate(&auth_headers, |_| Some(identity.clone()))
                .await,
            Err(AppError::Unauthorized)
        ));
        assert!(matches!(
            restarted.issue_at_generation(&identity, 0).await,
            Err(AppError::Unauthorized)
        ));
    }
}
