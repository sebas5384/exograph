use std::{
    env,
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
    time::SystemTime,
};

use anyhow::{anyhow, Result};
use clap::{Arg, Command};
use commands::{dev::DevCommand, test::TestCommand, yolo::YoloCommand};
use tokio::sync::{
    broadcast::{Receiver, Sender},
    Mutex,
};

use crate::commands::{build::BuildCommand, new::NewCommand, schema};

mod commands;
pub(crate) mod util;

const DEFAULT_MODEL_FILE: &str = "index.exo";

lazy_static::lazy_static! {
    pub static ref SIGINT: (Sender<()>, Mutex<Receiver<()>>) = {
        let (tx, rx) = tokio::sync::broadcast::channel(1);
        (tx, Mutex::new(rx))
    };
}

pub static EXIT_ON_SIGINT: AtomicBool = AtomicBool::new(true);

fn model_file_arg() -> Arg {
    Arg::new("model")
        .help("The path to the Exograph model file.")
        .hide_default_value(false)
        .required(false)
        .value_parser(clap::value_parser!(PathBuf))
        .default_value(DEFAULT_MODEL_FILE)
        .index(1)
}

fn new_project_arg() -> Arg {
    Arg::new("path")
        .help("Create a new project")
        .long_help("Create a new project in the given path.")
        .required(true)
        .value_parser(clap::value_parser!(PathBuf))
        .index(1)
}

fn database_arg() -> Arg {
    Arg::new("database")
        .help("The PostgreSQL database connection string to use. If not specified, the program will attempt to read it from the environment (`EXO_POSTGRES_URL`).")
        .long("database")
        .required(false)
}

fn output_arg() -> Arg {
    Arg::new("output")
        .help("Output file path")
        .help("If specified, the output will be written to this file path instead of stdout.")
        .short('o')
        .long("output")
        .required(false)
        .value_parser(clap::value_parser!(PathBuf))
        .num_args(1)
}

fn port_arg() -> Arg {
    Arg::new("port")
        .help("Listen port")
        .long_help("The port the server should listen for HTTP requests on.")
        .short('p')
        .long("port")
        .required(false)
        .value_parser(clap::value_parser!(u32))
        .num_args(1)
}

fn main() -> Result<()> {
    let system_start_time = SystemTime::now();

    // register a sigint handler
    ctrlc::set_handler(move || {
        // set SIGINT event when receiving signal
        let _ = SIGINT.0.send(());

        // exit if EXIT_ON_SIGINT is set
        // code may set this to be false if they have resources to
        // clean up before exiting
        if EXIT_ON_SIGINT.load(Ordering::SeqCst) {
            std::process::exit(0);
        }
    })
    .expect("Error setting Ctrl-C handler");

    let matches = Command::new("Exograph")
        .version(env!("CARGO_PKG_VERSION"))
        .disable_help_subcommand(true)
        .subcommand_required(true)
        .arg_required_else_help(true)
        .subcommand(
            Command::new("build")
                .about("Build exograph server binary")
                .arg(model_file_arg()),
        ).subcommand(
            Command::new("new")
                .about("Create a new Exograph project")
                .arg(new_project_arg()))
        .subcommand(
            Command::new("schema")
                .about("Create, migrate, import, and verify database schema")
                .subcommand_required(true)
                .arg_required_else_help(true)
                .subcommand(
                    Command::new("create")
                        .about("Create a database schema from a Exograph model")
                        .arg(model_file_arg())
                        .arg(output_arg())
                )
                .subcommand(
                    Command::new("verify")
                        .about("Verify that the database schema is compatible with a Exograph model")
                        .arg(model_file_arg())
                        .arg(database_arg())
                )
                .subcommand(
                    Command::new("migrate")
                        .about("Produces a SQL migration script for a Exograph model and the specified database")
                        .arg(model_file_arg())
                        .arg(database_arg())
                        .arg(output_arg())
                        .arg(
                            Arg::new("allow-destructive-changes")
                                .help("By default, destructive changes in the model file are commented out. If specified, this option will uncomment such changes.")
                                .long("allow-destructive-changes")
                                .required(false)
                                .num_args(0),
                        )

                )
                .subcommand(
                    Command::new("import")
                        .about("Create exograph model file based on a database schema")
                        .arg(database_arg())
                        .arg(output_arg()),
                ),
        )
        .subcommand(
            Command::new("dev")
                .about("Run exograph server in development mode")
                .arg(model_file_arg())
                .arg(port_arg()),
        )
        .subcommand(
            Command::new("test")
                .about("Perform integration tests")
                .arg(
                    Arg::new("dir")
                        .help("The directory containing integration tests.")
                        .value_parser(clap::value_parser!(PathBuf))
                        .required(true)
                        .index(1),
                )
                .arg(
                    Arg::new("pattern")
                        .help("Glob pattern to choose which tests to run.")
                        .required(false)
                        .index(2),
                )
                .arg(
                    Arg::new("run-introspection-tests")
                        .help("When specified, run standard introspection tests on the tests' model files")
                        .required(false)
                        .long("run-introspection-tests").num_args(0)
                )
        )
        .subcommand(
            Command::new("yolo")
                .about("Run local exograph server with a temporary database")
                .arg(model_file_arg())
                .arg(port_arg()),
        )
        .get_matches();

    fn get<T: Clone + Send + Sync + 'static>(
        matches: &clap::ArgMatches,
        arg_id: &str,
    ) -> Option<T> {
        matches.get_one::<T>(arg_id).cloned()
    }

    fn get_required<T: Clone + Send + Sync + 'static>(
        matches: &clap::ArgMatches,
        arg_id: &str,
    ) -> Result<T> {
        get(matches, arg_id).ok_or_else(|| anyhow!("Required argument `{}` is not present", arg_id))
    }

    // Map subcommands with args
    let command: Box<dyn crate::commands::command::Command> = match matches.subcommand() {
        Some(("build", matches)) => Box::new(BuildCommand {
            model: get_required(matches, "model")?,
        }),
        Some(("new", matches)) => Box::new(NewCommand {
            path: get_required(matches, "path")?,
        }),
        Some(("schema", matches)) => match matches.subcommand() {
            Some(("create", matches)) => Box::new(schema::create::CreateCommand {
                model: get_required(matches, "model")?,
                output: get(matches, "output"),
            }),
            Some(("verify", matches)) => Box::new(schema::verify::VerifyCommand {
                model: get_required(matches, "model")?,
                database: get(matches, "database"),
            }),
            Some(("import", matches)) => Box::new(schema::import::ImportCommand {
                output: get(matches, "output"),
            }),
            Some(("migrate", matches)) => Box::new(schema::migrate::MigrateCommand {
                model: get_required(matches, "model")?,
                database: get(matches, "database"),
                output: get(matches, "output"),
                allow_destructive_changes: matches.get_flag("allow-destructive-changes"),
            }),
            _ => panic!("Unhandled command name"),
        },
        Some(("dev", matches)) => Box::new(DevCommand {
            model: get_required(matches, "model")?,
            port: get(matches, "port"),
        }),
        Some(("test", matches)) => Box::new(TestCommand {
            dir: get_required(matches, "dir")?,
            pattern: get(matches, "pattern"),
            run_introspection_tests: matches.contains_id("run-introspection-tests"),
        }),
        Some(("yolo", matches)) => Box::new(YoloCommand {
            model: get_required(matches, "model")?,
            port: get(matches, "port"),
        }),
        _ => panic!("Unhandled command name"),
    };

    command.run(Some(system_start_time))
}
