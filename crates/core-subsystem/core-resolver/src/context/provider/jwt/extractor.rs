use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use tokio::sync::OnceCell;
use tracing::warn;

use crate::context::context_extractor::ContextExtractor;
use crate::context::error::ContextExtractionError;
use crate::context::provider::jwt::authenticator;
use crate::context::request::Request;
use crate::context::RequestContext;

use super::JwtAuthenticator;

pub struct JwtExtractor {
    jwt_authenticator: Arc<Option<JwtAuthenticator>>,
    extracted_claims: OnceCell<Value>,
}

impl JwtExtractor {
    pub fn new(jwt_authenticator: Arc<Option<JwtAuthenticator>>) -> Self {
        Self {
            jwt_authenticator,
            extracted_claims: OnceCell::new(),
        }
    }

    async fn extract_authentication(
        &self,
        request: &(dyn Request + Send + Sync),
    ) -> Result<Value, ContextExtractionError> {
        if let Some(jwt_authenticator) = self.jwt_authenticator.as_ref() {
            jwt_authenticator.extract_authentication(request).await
        } else {
            warn!(
                "{} or {} is not set, not parsing JWT tokens",
                authenticator::EXO_JWT_SECRET,
                authenticator::EXO_JWKS_ENDPOINT
            );
            Ok(serde_json::Value::Null)
        }
    }
}

#[async_trait]
impl ContextExtractor for JwtExtractor {
    fn annotation_name(&self) -> &str {
        "jwt"
    }

    async fn extract_context_field(
        &self,
        key: &str,
        _request_context: &RequestContext,
        request: &(dyn Request + Send + Sync),
    ) -> Result<Option<Value>, ContextExtractionError> {
        Ok(self
            .extracted_claims
            .get_or_try_init(|| async { self.extract_authentication(request).await })
            .await?
            .get(key)
            .cloned())
    }
}