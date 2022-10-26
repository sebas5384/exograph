use super::interceptor_weaver::{self, OperationKind};
use core_model::mapped_arena::MappedArena;
use core_model_builder::{
    error::ModelBuildingError, plugin::SubsystemBuilder, typechecker::typ::Type,
};
use core_plugin::serializable_system::SerializableSubsystem;
use core_plugin::serializable_system::SerializableSystem;
use core_plugin::system_serializer::SystemSerializer;

/// Build a [ModelSystem] given an [AstSystem].
///
/// First, it type checks the input [AstSystem] to produce typechecked types.
/// Next, it resolves the typechecked types. Resolving a type entails consuming annotations and finalizing information such as table and column names.
/// Finally, it builds the model type through a series of builders.
///
/// Each builder implements the following pattern:
/// - build_shallow: Build relevant shallow types.
///   Each shallow type in marked as primitive and thus holds just the name and notes if it is an input type.
/// - build_expanded: Fully expand the previously created shallow type as well as any other dependent objects (such as Query and Mutation)
///
/// This two pass method allows dealing with cycles.
/// In the first shallow pass, each builder iterates over resolved types and create a placeholder model type.
/// In the second expand pass, each builder again iterates over resolved types and expand each model type
/// (this is done in place, so references created from elsewhere remain valid). Since all model
/// types have been created in the first pass, the expansion pass can refer to other types (which may still be
/// shallow if hasn't had its chance in the iteration, but will expand when its turn comes in).
pub fn build(typechecked_system: MappedArena<Type>) -> Result<Vec<u8>, ModelBuildingError> {
    let base_system = core_model_builder::builder::system_builder::build(&typechecked_system)?;

    let postgres_subsystem_builder = postgres_model_builder::PostgresSubsystemBuilder {};
    let deno_subsystem_builder = deno_model_builder::DenoSubsystemBuilder {};
    let wasm_subsystem_builder = wasm_model_builder::WasmSubsystemBuilder {};

    let subsystem_builders: Vec<&dyn SubsystemBuilder> = vec![
        &postgres_subsystem_builder,
        &deno_subsystem_builder,
        &wasm_subsystem_builder,
    ];

    let mut subsystem_interceptions = vec![];
    let mut query_names = vec![];
    let mut mutation_names = vec![];

    // We must enumerate() over the result of running each builder, since that will filter out any
    // subsystem that don't need serialization (empty subsystems). This will ensure that we assign
    // the correct subsystem indices (which will be eventually used to dispatch interceptors to the
    // correct subsystem)
    let subsystems: Vec<SerializableSubsystem> = subsystem_builders
        .iter()
        .flat_map(|builder| builder.build(&typechecked_system, &base_system))
        .enumerate()
        .map(|(subsystem_index, build_info)| {
            let build_info = build_info?;
            subsystem_interceptions.push((subsystem_index, build_info.interceptions));
            query_names.extend(build_info.query_names);
            mutation_names.extend(build_info.mutation_names);

            Ok(SerializableSubsystem {
                id: build_info.id,
                subsystem_index,
                serialized_subsystem: build_info.serialized_subsystem,
            })
        })
        .collect::<Result<Vec<_>, ModelBuildingError>>()?;

    let query_interception_map =
        interceptor_weaver::weave(&query_names, &subsystem_interceptions, OperationKind::Query);

    let mutation_interception_map = interceptor_weaver::weave(
        &mutation_names,
        &subsystem_interceptions,
        OperationKind::Mutation,
    );

    let system = SerializableSystem {
        subsystems,
        query_interception_map,
        mutation_interception_map,
    };

    system.serialize().map_err(ModelBuildingError::Serialize)
}