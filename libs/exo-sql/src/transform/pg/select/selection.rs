// Copyright Exograph, Inc. All rights reserved.
//
// Use of this software is governed by the Business Source License
// included in the LICENSE file at the root of this repository.
//
// As of the Change Date specified in that file, in accordance with
// the Business Source License, use of this software will be governed
// by the Apache License, Version 2.0.

use crate::{
    sql::{
        json_agg::JsonAgg,
        json_object::{JsonObject, JsonObjectElement},
        predicate::ConcretePredicate,
    },
    transform::pg::{Postgres, SelectionLevel},
    AliasedSelectionElement, Column, Database, ManyToOne, OneToMany, RelationId, Selection,
    SelectionCardinality, SelectionElement,
};

pub enum SelectionSQL {
    Single(Column),
    Seq(Vec<Column>),
}

impl Selection {
    pub fn to_sql(&self, select_transformer: &Postgres, database: &Database) -> SelectionSQL {
        match self {
            Selection::Seq(seq) => SelectionSQL::Seq(
                seq.iter()
                    .map(
                        |AliasedSelectionElement {
                             alias: _alias,
                             column,
                         }| column.to_sql(select_transformer, database),
                    )
                    .collect(),
            ),
            Selection::Json(seq, cardinality) => {
                let object_elems = seq
                    .iter()
                    .map(|AliasedSelectionElement { alias, column }| {
                        JsonObjectElement::new(
                            alias.clone(),
                            column.to_sql(select_transformer, database),
                        )
                    })
                    .collect();

                let json_obj = Column::JsonObject(JsonObject(object_elems));

                match cardinality {
                    SelectionCardinality::One => SelectionSQL::Single(json_obj),
                    SelectionCardinality::Many => {
                        SelectionSQL::Single(Column::JsonAgg(JsonAgg(Box::new(json_obj))))
                    }
                }
            }
        }
    }

    pub fn selection_aggregate(
        &self,
        select_transformer: &Postgres,
        database: &Database,
    ) -> Vec<Column> {
        match self.to_sql(select_transformer, database) {
            SelectionSQL::Single(elem) => vec![elem],
            SelectionSQL::Seq(elems) => elems,
        }
    }
}

impl SelectionElement {
    pub fn to_sql(&self, transformer: &Postgres, database: &Database) -> Column {
        match self {
            SelectionElement::Physical(column_id) => Column::physical(*column_id, None),
            SelectionElement::Function {
                function_name,
                column_id,
            } => Column::Function {
                function_name: function_name.clone(),
                column_id: *column_id,
            },
            SelectionElement::Constant(s) => Column::Constant(s.clone()),
            SelectionElement::Object(elements) => {
                let elements = elements
                    .iter()
                    .map(|(alias, column)| {
                        JsonObjectElement::new(
                            alias.to_owned(),
                            column.to_sql(transformer, database),
                        )
                    })
                    .collect();
                Column::JsonObject(JsonObject(elements))
            }
            SelectionElement::SubSelect(relation_id, select) => {
                let (self_column_id, foreign_column_id) = match relation_id {
                    RelationId::OneToMany(relation_id) => {
                        let OneToMany {
                            self_pk_column_id,
                            foreign_column_id,
                        } = relation_id.deref(database);
                        (self_pk_column_id, foreign_column_id)
                    }
                    RelationId::ManyToOne(relation_id) => {
                        let ManyToOne {
                            self_column_id,
                            foreign_pk_column_id,
                            ..
                        } = relation_id.deref(database);
                        (self_column_id, foreign_pk_column_id)
                    }
                };

                let subselect_predicate = Some(ConcretePredicate::Eq(
                    Column::physical(self_column_id, None),
                    Column::physical(foreign_column_id, None),
                ));

                Column::SubSelect(Box::new(transformer.compute_select(
                    select,
                    subselect_predicate,
                    SelectionLevel::Nested,
                    false,
                    database,
                )))
            }
        }
    }
}
