use axum::{
    async_trait,
    extract::{FromRequestParts, TypedHeader},
    headers::{authorization::Bearer, Authorization},
    http::{request::Parts, StatusCode},
    RequestPartsExt,
};
use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::{interval, Duration as TokioDuration};

#[derive(Debug, Serialize, Deserialize)]
pub enum TokenType {
    Access,
    Refresh,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,           // User ID
    pub exp: i64,              // Expiration time
    pub iat: i64,              // Issued at
    pub token_type: TokenType, // Token type (access or refresh)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jti: Option<String>, // JWT ID (for blacklisting)
}

#[derive(Debug, Clone)]
pub struct AuthUser {
    pub user_id: String,
}

#[derive(Clone)]
pub struct JwtAuth {
    encoding_key: EncodingKey,
    decoding_key: DecodingKey,
    blacklist: Arc<Mutex<HashMap<String, i64>>>,
    access_token_duration: Duration,
    refresh_token_duration: Duration,
}

impl JwtAuth {
    pub fn new(secret: &[u8]) -> Self {
        let jwt_auth = Self {
            encoding_key: EncodingKey::from_secret(secret),
            decoding_key: DecodingKey::from_secret(secret),
            blacklist: Arc::new(Mutex::new(HashMap::new())),
            access_token_duration: Duration::hours(1), // 1 hour for access tokens
            refresh_token_duration: Duration::days(7), // 7 days for refresh tokens
        };

        // Start the cleanup task
        jwt_auth.start_cleanup_task();
        jwt_auth
    }

    fn start_cleanup_task(&self) {
        let blacklist = self.blacklist.clone();
        tokio::spawn(async move {
            let mut interval = interval(TokioDuration::from_secs(3600)); // Run cleanup every hour
            loop {
                interval.tick().await;
                let mut blacklist_guard = blacklist.lock().await;
                let now = Utc::now().timestamp();
                blacklist_guard.retain(|_, exp| *exp > now);
            }
        });
    }

    pub async fn create_tokens(
        &self,
        user_id: &str,
    ) -> Result<(String, String, i64), jsonwebtoken::errors::Error> {
        let now = Utc::now();
        let access_token_exp = (now + self.access_token_duration).timestamp();
        let refresh_token_exp = (now + self.refresh_token_duration).timestamp();

        // Create access token
        let access_claims = Claims {
            sub: user_id.to_string(),
            exp: access_token_exp,
            iat: now.timestamp(),
            token_type: TokenType::Access,
            jti: Some(uuid::Uuid::new_v4().to_string()),
        };

        // Create refresh token
        let refresh_claims = Claims {
            sub: user_id.to_string(),
            exp: refresh_token_exp,
            iat: now.timestamp(),
            token_type: TokenType::Refresh,
            jti: Some(uuid::Uuid::new_v4().to_string()),
        };

        let access_token = encode(&Header::default(), &access_claims, &self.encoding_key)?;

        let refresh_token = encode(&Header::default(), &refresh_claims, &self.encoding_key)?;

        Ok((access_token, refresh_token, access_token_exp))
    }

    pub async fn refresh_access_token(
        &self,
        refresh_token: &str,
    ) -> Result<(String, i64), (StatusCode, String)> {
        // Verify refresh token
        let refresh_claims = self.verify_token(refresh_token).await?;

        match refresh_claims.token_type {
            TokenType::Refresh => {
                // Create new access token
                let now = Utc::now();
                let exp = (now + self.access_token_duration).timestamp();

                let access_claims = Claims {
                    sub: refresh_claims.sub,
                    exp,
                    iat: now.timestamp(),
                    token_type: TokenType::Access,
                    jti: Some(uuid::Uuid::new_v4().to_string()),
                };

                let access_token = encode(&Header::default(), &access_claims, &self.encoding_key)
                    .map_err(|e| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("Failed to create token: {}", e),
                    )
                })?;

                Ok((access_token, exp))
            }
            TokenType::Access => Err((StatusCode::BAD_REQUEST, "Invalid token type".to_string())),
        }
    }

    pub async fn blacklist_token(&self, token: &str) -> Result<(), (StatusCode, String)> {
        let claims = self.verify_token(token).await?;

        if let Some(jti) = claims.jti {
            let mut blacklist = self.blacklist.lock().await;
            blacklist.insert(jti, claims.exp);
            Ok(())
        } else {
            Err((
                StatusCode::BAD_REQUEST,
                "Token does not support blacklisting".to_string(),
            ))
        }
    }

    pub async fn verify_token(&self, token: &str) -> Result<Claims, (StatusCode, String)> {
        // Decode token
        let token_data = decode::<Claims>(token, &self.decoding_key, &Validation::default())
            .map_err(|_| (StatusCode::UNAUTHORIZED, "Invalid token".to_string()))?;

        // Check if token is blacklisted
        if let Some(jti) = &token_data.claims.jti {
            let blacklist = self.blacklist.lock().await;
            if blacklist.contains_key(jti) {
                return Err((
                    StatusCode::UNAUTHORIZED,
                    "Token has been revoked".to_string(),
                ));
            }
        }

        // Check expiration (even though jwt library does this, we want a specific error)
        let now = Utc::now().timestamp();
        if token_data.claims.exp < now {
            return Err((StatusCode::UNAUTHORIZED, "Token has expired".to_string()));
        }

        Ok(token_data.claims)
    }

    // // Cleanup expired blacklisted tokens - can be called manually if needed
    // pub async fn cleanup_blacklist(&self) {
    //     let mut blacklist = self.blacklist.lock().await;
    //     let now = Utc::now().timestamp();
    //     blacklist.retain(|_, exp| *exp > now);
    // }
}

#[async_trait]
impl<S> FromRequestParts<S> for AuthUser
where
    S: Send + Sync,
{
    type Rejection = (StatusCode, String);

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        // Extract bearer token
        let TypedHeader(Authorization(bearer)) = parts
            .extract::<TypedHeader<Authorization<Bearer>>>()
            .await
            .map_err(|_| {
                (
                    StatusCode::UNAUTHORIZED,
                    "Missing authorization header".to_string(),
                )
            })?;

        // Get JWT auth instance
        let jwt_auth = parts.extensions.get::<JwtAuth>().ok_or((
            StatusCode::INTERNAL_SERVER_ERROR,
            "JWT auth not configured".to_string(),
        ))?;

        // Verify token and get claims
        let claims = jwt_auth.verify_token(bearer.token()).await?;

        // Ensure it's an access token
        match claims.token_type {
            TokenType::Access => Ok(AuthUser {
                user_id: claims.sub,
            }),
            TokenType::Refresh => Err((
                StatusCode::UNAUTHORIZED,
                "Cannot use refresh token for authentication".to_string(),
            )),
        }
    }
}
