// Copyright Exograph, Inc. All rights reserved.
//
// Use of this software is governed by the Business Source License
// included in the LICENSE file at the root of this repository.
//
// As of the Change Date specified in that file, in accordance with
// the Business Source License, use of this software will be governed
// by the Apache License, Version 2.0.

use super::column_spec::{ColumnSpec, ColumnTypeSpec};
use super::op::SchemaOp;
use super::statement::SchemaStatement;
use std::collections::{HashMap, HashSet};

use crate::database_error::DatabaseError;
use deadpool_postgres::Client;

use super::constraint::{sorted_comma_list, Constraints};
use super::issue::WithIssues;

#[derive(Debug)]
pub struct TableSpec {
    pub name: String,
    pub columns: Vec<ColumnSpec>,
}

impl TableSpec {
    pub fn new(name: impl Into<String>, columns: Vec<ColumnSpec>) -> Self {
        Self {
            name: name.into(),
            columns,
        }
    }

    fn named_unique_constraints(&self) -> HashMap<&String, HashSet<String>> {
        self.columns.iter().fold(HashMap::new(), |mut map, c| {
            {
                for name in c.unique_constraints.iter() {
                    let entry: &mut HashSet<String> = map.entry(name).or_insert_with(HashSet::new);
                    (*entry).insert(c.name.clone());
                }
            }
            map
        })
    }

    /// Creates a new table specification from an SQL table.
    pub(super) async fn from_live_db(
        client: &Client,
        table_name: &str,
    ) -> Result<WithIssues<TableSpec>, DatabaseError> {
        // Query to get a list of columns in the table
        let columns_query = format!(
            "SELECT column_name FROM information_schema.columns WHERE table_name = '{table_name}'",
        );

        let mut issues = Vec::new();

        let constraints = Constraints::from_live_db(client, table_name).await?;

        let mut column_type_mapping = HashMap::new();

        for foreign_constraint in constraints.foreign_constraints.iter() {
            // Assumption that there is only one column in the foreign key (for now a correct assumption since we don't support composite keys)
            let self_column_name = foreign_constraint.self_columns.iter().next().unwrap();
            let foreign_pk_column_name = foreign_constraint.foreign_columns.iter().next().unwrap();

            let mut column = ColumnSpec::from_live_db(
                client,
                table_name,
                foreign_pk_column_name,
                true,
                None,
                vec![],
            )
            .await?;
            issues.append(&mut column.issues);

            if let Some(spec) = column.value {
                column_type_mapping.insert(
                    self_column_name.clone(),
                    ColumnTypeSpec::ColumnReference {
                        foreign_table_name: foreign_constraint.foreign_table.clone(),
                        foreign_pk_column_name: foreign_pk_column_name.clone(),
                        foreign_pk_type: Box::new(spec.typ),
                    },
                );
            }
        }

        let mut columns = Vec::new();
        for row in client.query(columns_query.as_str(), &[]).await? {
            let name: String = row.get("column_name");

            let unique_constraint_names: Vec<_> = constraints
                .uniques
                .iter()
                .flat_map(|unique| {
                    if unique.columns.contains(&name) {
                        Some(unique.constraint_name.clone())
                    } else {
                        None
                    }
                })
                .collect();

            let mut column = ColumnSpec::from_live_db(
                client,
                table_name,
                &name,
                constraints.primary_key.columns.contains(&name),
                column_type_mapping.get(&name).cloned(),
                unique_constraint_names,
            )
            .await?;
            issues.append(&mut column.issues);

            if let Some(spec) = column.value {
                columns.push(spec);
            }
        }

        Ok(WithIssues {
            value: TableSpec {
                name: table_name.to_string(),
                columns,
            },
            issues,
        })
    }

    /// Get any extensions this table may depend on.
    pub fn get_required_extensions(&self) -> HashSet<String> {
        let mut required_extensions = HashSet::new();

        for col_spec in self.columns.iter() {
            if let ColumnTypeSpec::Uuid = col_spec.typ {
                required_extensions.insert("pgcrypto".to_string());
            }
        }

        required_extensions
    }

    pub fn diff<'a>(&'a self, new: &'a Self) -> Vec<SchemaOp<'a>> {
        let existing_columns = &self.columns;
        let new_columns = &new.columns;

        let existing_column_map: HashMap<_, _> = existing_columns
            .iter()
            .map(|c| (c.name.clone(), c))
            .collect();
        let new_column_map: HashMap<_, _> =
            new_columns.iter().map(|c| (c.name.clone(), c)).collect();

        let mut changes = vec![];

        for existing_column in self.columns.iter() {
            let new_column = new_column_map.get(&existing_column.name);

            match new_column {
                Some(new_column) => {
                    changes.extend(existing_column.diff(new_column, self, new));
                }
                None => {
                    // column was removed
                    changes.push(SchemaOp::DeleteColumn {
                        table: self,
                        column: existing_column,
                    });
                }
            }
        }

        for new_column in new.columns.iter() {
            let existing_column = existing_column_map.get(&new_column.name);

            if existing_column.is_none() {
                // new column
                changes.push(SchemaOp::CreateColumn {
                    table: new,
                    column: new_column,
                });
            }
        }

        for (constraint_name, _column_names) in self.named_unique_constraints().iter() {
            if !new.named_unique_constraints().contains_key(constraint_name) {
                // constraint deletion
                changes.push(SchemaOp::RemoveUniqueConstraint {
                    table: new,
                    constraint: constraint_name.to_string(),
                });
            }
        }

        for (new_constraint_name, new_constraint_column_names) in
            new.named_unique_constraints().iter()
        {
            let existing_constraints = self.named_unique_constraints();
            let existing_constraint_column_names = existing_constraints.get(new_constraint_name);

            match existing_constraint_column_names {
                Some(existing_constraint_column_names) => {
                    if existing_constraint_column_names != new_constraint_column_names {
                        // constraint modification, so remove the old constraint and add the new one
                        changes.push(SchemaOp::RemoveUniqueConstraint {
                            table: new,
                            constraint: new_constraint_name.to_string(),
                        });
                        changes.push(SchemaOp::CreateUniqueConstraint {
                            table: new,
                            constraint_name: new_constraint_name.to_string(),
                            columns: new_constraint_column_names.clone(),
                        });
                    }
                }
                None => {
                    // new constraint
                    changes.push(SchemaOp::CreateUniqueConstraint {
                        table: new,
                        constraint_name: new_constraint_name.to_string(),
                        columns: new_constraint_column_names.clone(),
                    });
                }
            }
        }

        changes
    }

    /// Converts the table specification to SQL statements.
    pub(super) fn creation_sql(&self) -> SchemaStatement {
        let mut post_statements = Vec::new();
        let column_stmts: String = self
            .columns
            .iter()
            .map(|c| {
                let mut s = c.to_sql(&self.name);
                post_statements.append(&mut s.post_statements);
                s.statement
            })
            .collect::<Vec<_>>()
            .join(",\n\t");

        for (unique_constraint_name, columns) in self.named_unique_constraints().iter() {
            let columns_part = sorted_comma_list(columns, true);

            post_statements.push(format!(
                "ALTER TABLE \"{}\" ADD CONSTRAINT \"{}\" UNIQUE ({});",
                self.name, unique_constraint_name, columns_part
            ));
        }

        SchemaStatement {
            statement: format!("CREATE TABLE \"{}\" (\n\t{}\n);", self.name, column_stmts),
            pre_statements: vec![],
            post_statements,
        }
    }

    pub(super) fn deletion_sql(&self) -> SchemaStatement {
        let mut pre_statements = vec![];
        for (unique_constraint_name, _) in self.named_unique_constraints().iter() {
            pre_statements.push(format!(
                "ALTER TABLE \"{}\" DROP CONSTRAINT \"{}\";",
                self.name, unique_constraint_name
            ));
        }

        SchemaStatement {
            statement: format!("DROP TABLE \"{}\" CASCADE;", self.name),
            pre_statements,
            post_statements: vec![],
        }
    }
}
