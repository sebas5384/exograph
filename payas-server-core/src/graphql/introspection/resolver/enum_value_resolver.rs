use async_graphql_parser::types::EnumValueDefinition;
use async_trait::async_trait;
use serde_json::Value;

use payas_core_resolver::request_context::RequestContext;
use payas_core_resolver::validation::field::ValidatedField;

use crate::graphql::execution::field_resolver::FieldResolver;
use crate::graphql::execution::system_context::SystemContext;
use crate::graphql::execution_error::ExecutionError;

#[async_trait]
impl FieldResolver<Value, ExecutionError, SystemContext> for EnumValueDefinition {
    async fn resolve_field<'e>(
        &'e self,
        field: &ValidatedField,
        _system_context: &'e SystemContext,
        _request_context: &'e RequestContext<'e>,
    ) -> Result<Value, ExecutionError> {
        match field.name.as_str() {
            "name" => Ok(Value::String(self.value.node.as_str().to_owned())),
            "description" => Ok(self
                .description
                .clone()
                .map(|v| Value::String(v.node))
                .unwrap_or(Value::Null)),
            "isDeprecated" => Ok(Value::Bool(false)), // TODO
            "deprecationReason" => Ok(Value::Null),   // TODO
            "__typename" => Ok(Value::String("__EnumValue".to_string())),
            field_name => Err(ExecutionError::InvalidField(
                field_name.to_owned(),
                "EnumValueDefinition",
            )),
        }
    }
}
