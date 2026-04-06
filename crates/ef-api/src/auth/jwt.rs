use chrono::{Duration, Utc};
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// JWT claims payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    /// Subject — tenant id
    pub sub: Uuid,
    /// Tenant name (convenience)
    pub tenant: String,
    /// Issued at (Unix timestamp)
    pub iat: i64,
    /// Expiry (Unix timestamp)
    pub exp: i64,
}

/// Create a signed JWT for the given tenant.
pub fn generate_token(
    tenant_id: Uuid,
    tenant_name: &str,
    secret: &str,
    expiry_seconds: u64,
) -> Result<String, jsonwebtoken::errors::Error> {
    let now = Utc::now();
    let exp = now + Duration::seconds(expiry_seconds as i64);

    let claims = Claims {
        sub: tenant_id,
        tenant: tenant_name.to_string(),
        iat: now.timestamp(),
        exp: exp.timestamp(),
    };

    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
}

/// Validate a JWT and return the decoded claims.
pub fn validate_token(token: &str, secret: &str) -> Result<Claims, jsonwebtoken::errors::Error> {
    let data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::default(),
    )?;
    Ok(data.claims)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_token() {
        let id = Uuid::new_v4();
        let secret = "test-secret-key";
        let token = generate_token(id, "acme", secret, 3600).unwrap();
        let claims = validate_token(&token, secret).unwrap();
        assert_eq!(claims.sub, id);
        assert_eq!(claims.tenant, "acme");
    }

    #[test]
    fn reject_bad_secret() {
        let id = Uuid::new_v4();
        let token = generate_token(id, "acme", "secret-a", 3600).unwrap();
        assert!(validate_token(&token, "secret-b").is_err());
    }
}
