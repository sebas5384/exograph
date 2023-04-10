use anyhow::Result;
use clap::Command;
use exo_sql::schema::issue::WithIssues;
use exo_sql::{schema::spec::SchemaSpec, Database};
use std::{io::Write, path::PathBuf};

use heck::ToUpperCamelCase;

use exo_sql::schema::issue::Issue;
use exo_sql::{PhysicalColumn, PhysicalColumnType, PhysicalTable};

use crate::commands::command::{database_arg, get, output_arg, CommandDefinition};
use crate::util::open_file_for_output;

pub(super) struct ImportCommandDefinition {}

impl CommandDefinition for ImportCommandDefinition {
    fn command(&self) -> clap::Command {
        Command::new("import")
            .about("Create exograph model file based on a database schema")
            .arg(database_arg())
            .arg(output_arg())
    }

    /// Create a exograph model file based on a database schema
    fn execute(&self, matches: &clap::ArgMatches) -> Result<()> {
        let output: Option<PathBuf> = get(matches, "output");
        // Create runtime and make the rest of this an async block
        // (then block on it)
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_io()
            .build()
            .unwrap();

        let mut issues = Vec::new();
        let mut schema = rt.block_on(import_schema())?;
        let mut model = schema.value.to_model();

        issues.append(&mut schema.issues);
        issues.append(&mut model.issues);

        let mut buffer: Box<dyn Write> = open_file_for_output(output.as_deref())?;
        buffer.write_all(schema.value.to_model().value.as_bytes())?;

        for issue in &issues {
            eprintln!("{issue}");
        }

        if let Some(output) = &output {
            eprintln!("\nExograph model written to `{}`", output.display());
        }

        Ok(())
    }
}

async fn import_schema() -> Result<WithIssues<SchemaSpec>> {
    let database = Database::from_env(Some(1))?; // TODO: error handling here
    let client = database.get_client().await?;
    let schema = SchemaSpec::from_db(&client).await?;
    Ok(schema)
}

pub trait ToModel {
    fn to_model(&self) -> WithIssues<String>;
}

/// Converts the name of a SQL table to a exograph model name (for example, concert_artist -> ConcertArtist).
fn to_model_name(name: &str) -> String {
    name.to_upper_camel_case()
}

impl ToModel for SchemaSpec {
    /// Converts the schema specification to a exograph file.
    fn to_model(&self) -> WithIssues<String> {
        let mut issues = Vec::new();
        let stmt = self
            .tables
            .iter()
            .map(|table| {
                let mut model = table.to_model();
                issues.append(&mut model.issues);
                format!("{}\n\n", model.value)
            })
            .collect();

        WithIssues {
            value: stmt,
            issues,
        }
    }
}

impl ToModel for PhysicalTable {
    /// Converts the table specification to a exograph model.
    fn to_model(&self) -> WithIssues<String> {
        let mut issues = Vec::new();

        let table_annot = format!("@table(\"{}\")", self.name);
        let column_stmts = self
            .columns
            .iter()
            .map(|c| {
                let mut model = c.to_model();
                issues.append(&mut model.issues);
                format!("  {}\n", model.value)
            })
            .collect::<String>();

        // not a robust check
        if self.name.ends_with('s') {
            issues.push(Issue::Hint(format!(
                "model name `{}` should be changed to singular",
                to_model_name(&self.name)
            )));
        }

        WithIssues {
            value: format!(
                "{}\nmodel {} {{\n{}}}",
                table_annot,
                to_model_name(&self.name),
                column_stmts
            ),
            issues,
        }
    }
}

impl ToModel for PhysicalColumn {
    /// Converts the column specification to a exograph model.
    fn to_model(&self) -> WithIssues<String> {
        let mut issues = Vec::new();

        let pk_str = if self.is_pk { " @pk" } else { "" };
        let autoinc_str = if self.is_auto_increment {
            " = autoIncrement()"
        } else {
            ""
        };

        let (mut data_type, annots) = self.typ.to_model();
        if let PhysicalColumnType::ColumnReference { ref_table_name, .. } = &self.typ {
            data_type = to_model_name(&data_type);

            issues.push(Issue::Hint(format!(
                "consider adding a field to `{}` of type `[{}]` to create a one-to-many relationship",
                ref_table_name, to_model_name(&self.table_name),
            )));
        }

        if self.is_nullable {
            data_type += "?"
        }

        WithIssues {
            value: format!(
                "{}: {}{}{}",
                self.name,
                data_type + &annots,
                autoinc_str,
                pk_str,
            ),
            issues: Vec::new(),
        }
    }
}
