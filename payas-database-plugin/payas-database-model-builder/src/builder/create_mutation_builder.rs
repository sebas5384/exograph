//! Build mutation input types associated with creation (<Type>CreationInput) and
//! the create mutations (create<Type>, and create<Type>s)

use super::naming::{ToGqlMutationNames, ToGqlTypeNames};
use payas_core_model_builder::builder::type_builder::ResolvedTypeEnv;
use payas_model::model::mapped_arena::{MappedArena, SerializableSlabIndex};
use payas_model::model::types::GqlType;
use payas_model::model::{GqlCompositeType, GqlCompositeTypeKind, GqlTypeKind};

use payas_model::model::operation::{
    CreateDataParameter, CreateDataParameterTypeWithModifier, DatabaseMutationKind,
};

use super::mutation_builder::{DataParamBuilder, MutationBuilder};
use super::system_builder::SystemContextBuilding;
use super::Builder;
use payas_core_model_builder::builder::resolved_builder::{ResolvedCompositeType, ResolvedType};

pub struct CreateMutationBuilder;

impl Builder for CreateMutationBuilder {
    fn type_names(
        &self,
        resolved_composite_type: &ResolvedCompositeType,
        models: &MappedArena<ResolvedType>,
    ) -> Vec<String> {
        let mut field_types = self.data_param_field_type_names(resolved_composite_type, models);
        field_types.push(Self::data_param_type_name(resolved_composite_type));
        field_types
    }

    fn build_expanded(&self, resolved_env: &ResolvedTypeEnv, building: &mut SystemContextBuilding) {
        for (_, model_type) in building.database_types.iter() {
            if let GqlTypeKind::Composite(GqlCompositeType {
                kind: GqlCompositeTypeKind::Persistent { .. },
                ..
            }) = &model_type.kind
            {
                for (existing_id, expanded_kind) in self.expanded_data_type(
                    model_type,
                    resolved_env,
                    building,
                    Some(model_type),
                    None,
                ) {
                    building.mutation_types[existing_id].kind =
                        GqlTypeKind::Composite(expanded_kind);
                }
            }
        }
        for (_, model_type) in building.database_types.iter() {
            if let GqlTypeKind::Composite(GqlCompositeType {
                kind: GqlCompositeTypeKind::Persistent { .. },
                ..
            }) = &model_type.kind
            {
                let model_type_id = building
                    .database_types
                    .get_id(model_type.name.as_str())
                    .unwrap();

                for mutation in self.build_mutations(model_type_id, model_type, building) {
                    building.mutations.add(&mutation.name.to_owned(), mutation);
                }
            }
        }
    }
}

impl MutationBuilder for CreateMutationBuilder {
    fn single_mutation_name(model_type: &GqlType) -> String {
        model_type.pk_create()
    }

    fn single_mutation_kind(
        _model_type_id: SerializableSlabIndex<GqlType>,
        model_type: &GqlType,
        building: &SystemContextBuilding,
    ) -> DatabaseMutationKind {
        DatabaseMutationKind::Create(Self::data_param(model_type, building, false))
    }

    fn multi_mutation_name(model_type: &GqlType) -> String {
        model_type.collection_create()
    }

    fn multi_mutation_kind(
        _model_type_id: SerializableSlabIndex<GqlType>,
        model_type: &GqlType,
        building: &SystemContextBuilding,
    ) -> DatabaseMutationKind {
        DatabaseMutationKind::Create(Self::data_param(model_type, building, true))
    }
}

impl DataParamBuilder<CreateDataParameter> for CreateMutationBuilder {
    fn mark_fields_optional() -> bool {
        false
    }

    fn base_data_type_name(model_type_name: &str) -> String {
        model_type_name.creation_type()
    }

    fn data_param(
        model_type: &GqlType,
        building: &SystemContextBuilding,
        array: bool,
    ) -> CreateDataParameter {
        let data_param_type_name = Self::base_data_type_name(&model_type.name);
        let data_param_type_id = building
            .mutation_types
            .get_id(&data_param_type_name)
            .unwrap();

        CreateDataParameter {
            name: "data".to_string(),
            typ: CreateDataParameterTypeWithModifier {
                type_name: data_param_type_name,
                type_id: data_param_type_id,
                array_input: array,
            },
        }
    }
}