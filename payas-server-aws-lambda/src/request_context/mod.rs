pub mod cookie;
pub mod header;
pub mod jwt;

use payas_server_core::{
    request_context::{BoxedParsedContext, RequestContext, UserRequestContext},
    SystemContext,
};

use self::{cookie::CookieProcessor, header::HeaderProcessor, jwt::JwtAuthenticator};

pub trait LambdaContextProducer {
    fn parse_context(
        &self,
        request: &lambda_http::Request,
    ) -> Result<BoxedParsedContext, ContextProducerError>;
}
pub enum ContextProducerError {
    Unauthorized,
    Malformed,
    Unknown,
}

pub struct LambdaRequestContextProducer {
    producers: Vec<Box<dyn LambdaContextProducer + Send + Sync>>,
}

impl LambdaRequestContextProducer {
    pub fn new() -> Self {
        Self {
            producers: vec![
                Box::new(CookieProcessor),
                Box::new(JwtAuthenticator::new_from_env()),
                Box::new(HeaderProcessor),
            ],
        }
    }

    /// Generates request context
    pub fn generate_request_context<'a>(
        &'a self,
        request: &lambda_http::Request,
        system_context: &'a SystemContext,
    ) -> Result<RequestContext, ContextProducerError> {
        let parsed_contexts = self
            .producers
            .iter()
            .map(|producer| {
                // create parsed context
                producer.parse_context(request)
            })
            .collect::<Result<Vec<_>, ContextProducerError>>()?; // emit errors if we encounter any while gathering context

        Ok(RequestContext::User(
            UserRequestContext::from_parsed_contexts(parsed_contexts, system_context),
        ))
    }
}

impl Default for LambdaRequestContextProducer {
    fn default() -> Self {
        Self::new()
    }
}