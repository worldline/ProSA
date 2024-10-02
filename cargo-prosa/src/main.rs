#![doc = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/COPYRIGHT"))]
//!
//! [![github]](https://github.com/worldline/prosa)&ensp;[![crates-io]](https://crates.io/crates/prosa)&ensp;[![docs-rs]](crate)
//!
#![doc = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/doc_assets/header_badges.md"))]
//!
//! Build your own ProSA

use std::{
    collections::HashSet,
    env, fs,
    io::{self, Write},
    path::Path,
    str::FromStr,
};

use cargo_prosa::{
    builder::Desc,
    cargo::CargoMetadata,
    package::{container::ContainerFile, deb::DebPkg},
    CONFIGURATION_FILENAME,
};
use clap::{arg, Command};
use tera::Tera;
use toml_edit::DocumentMut;

macro_rules! cargo {
    ( $m:expr, $p:expr, $( $dep:expr ),* ) => {
        {
            let cargo = if let Some(path) = $p {
                std::process::Command::new("cargo")
                .args(vec![
                    $m,
                    "--manifest-path",
                    format!("{}/Cargo.toml", path).as_str(),
                    $(
                        $dep,
                    )*
                ]).output()
            } else {
                std::process::Command::new("cargo")
                .args(vec![
                    $m,
                    $(
                        $dep,
                    )*
                ]).output()
            }?;

            io::stdout()
                .write_all(&cargo.stdout)?;
            io::stderr()
                .write_all(&cargo.stderr)?;

            cargo
        }
    };
}

/// Function to render jinja build.rs file into prosa project
fn render_build_rs<P>(path: P, ctx: &tera::Context) -> Result<(), tera::Error>
where
    P: AsRef<Path>,
{
    const RENDER_FILENAME: &str = "build.rs";
    let mut tera_build = Tera::default();
    tera_build.add_raw_template(
        RENDER_FILENAME,
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/build.rs.j2")),
    )?;

    let build_file = fs::File::create(&path).map_err(tera::Error::io_error)?;
    tera_build.render_to(RENDER_FILENAME, ctx, build_file)
}

/// Function to render jinja build.rs file into prosa project
fn render_main_rs<P>(path: P, ctx: &tera::Context) -> Result<(), tera::Error>
where
    P: AsRef<Path>,
{
    const RENDER_FILENAME: &str = "main.rs";
    let mut tera_build = Tera::default();
    tera_build.add_raw_template(
        RENDER_FILENAME,
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/main.rs.j2")),
    )?;

    let main_file = fs::File::create(&path).map_err(tera::Error::io_error)?;
    tera_build.render_to(RENDER_FILENAME, ctx, main_file)
}

/// Function to initiate ProSA project file (or update them if existing)
fn init_prosa(path: &str, context: &tera::Context) -> io::Result<()> {
    let prosa_path = Path::new(&path);

    // Add dependencies
    let cargo_add_prosa = cargo!("add", Some(path), "prosa");
    let cargo_add_prosa_utils = cargo!("add", Some(path), "prosa-utils");
    let cargo_add_clap = cargo!("add", Some(path), "clap");
    let cargo_add_daemonize = cargo!("add", Some(path), "daemonize");
    let cargo_add_tokio = cargo!("add", Some(path), "tokio");
    let cargo_add_serde = cargo!("add", Some(path), "serde");
    let cargo_add_config = cargo!("add", Some(path), "config");
    let cargo_add_tracing = cargo!("add", Some(path), "tracing");

    // Add build dependencies
    let cargo_add_build_cargo_prosa = cargo!("add", Some(path), "--build", "cargo-prosa");
    let cargo_add_build_toml = cargo!("add", Some(path), "--build", "toml");

    // Run fmt to reformat code
    let _ = cargo!("fmt", Some(path), "-q");

    if cargo_add_prosa.status.success()
        && cargo_add_prosa_utils.status.success()
        && cargo_add_clap.status.success()
        && cargo_add_daemonize.status.success()
        && cargo_add_tokio.status.success()
        && cargo_add_serde.status.success()
        && cargo_add_config.status.success()
        && cargo_add_tracing.status.success()
        && cargo_add_build_cargo_prosa.status.success()
        && cargo_add_build_toml.status.success()
    {
        // Create (or replace) ProSA files
        render_build_rs(prosa_path.join("build.rs"), context)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        render_main_rs(prosa_path.join("src").join("main.rs"), context)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        // Create ProSA.toml desc config file if it doesn't exist
        let prosa_desc_config_path = prosa_path.join(CONFIGURATION_FILENAME);
        if !prosa_desc_config_path.exists() {
            Desc::default().create(prosa_desc_config_path)?;
        }

        // Add optional parameters for deb package build
        if let Some(tera::Value::Bool(true)) = context.get("deb_pkg") {
            let cargo_toml = fs::read_to_string(prosa_path.join("Cargo.toml"))?;
            let mut cargo_doc = cargo_toml
                .parse::<DocumentMut>()
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
            if let Some(toml_edit::Item::Table(package_table)) = cargo_doc.get_mut("package") {
                if let Some(name) = context.get("name").and_then(|v| v.as_str()) {
                    if let Some(toml_edit::Item::Table(metadata_table)) =
                        package_table.get_mut("metadata")
                    {
                        if let Some(toml_edit::Item::Table(deb_table)) =
                            metadata_table.get_mut("deb")
                        {
                            DebPkg::add_deb_pkg_metadata(deb_table, name);
                        } else {
                            let mut deb_table = toml_edit::Table::new();
                            DebPkg::add_deb_pkg_metadata(&mut deb_table, name);

                            metadata_table.insert("deb", toml_edit::Item::Table(deb_table));
                        }
                    } else {
                        let mut deb_table = toml_edit::Table::new();
                        DebPkg::add_deb_pkg_metadata(&mut deb_table, name);

                        let mut metadata_table = toml_edit::Table::new();
                        metadata_table.set_implicit(true);
                        metadata_table.insert("deb", toml_edit::Item::Table(deb_table));

                        package_table.insert("metadata", toml_edit::Item::Table(metadata_table));
                    }
                }
            }

            let mut cargo_toml_file = fs::File::create(prosa_path.join("Cargo.toml"))?;
            cargo_toml_file.write_all(cargo_doc.to_string().as_bytes())?;
        }
    }

    Ok(())
}

fn cli() -> Command {
    Command::new("cargo")
        .bin_name("cargo")
        .subcommand_required(true)
        .arg_required_else_help(true)
        .subcommand(Command::new("prosa")
            .about("ProSA builder")
            .subcommand_required(true)
            .arg_required_else_help(true)
            .subcommand(
                Command::new("new")
                    .about("Create a new ProSA package")
                    .arg(arg!(-n --name <NAME> "Set the package name. Defaults to the directory name"))
                    .arg(arg!(--deb "Configure the ProSA to generate a deb package").action(clap::ArgAction::SetTrue))
                    .arg(arg!(<PATH> "Name of the new ProSA"))
                    .arg_required_else_help(true),
            )
            .subcommand(
                Command::new("init")
                    .about("Create a new ProSA package in an existing directory")
                    .arg(arg!(--deb "Configure the ProSA to generate a deb package").action(clap::ArgAction::SetTrue))
                    .arg(arg!(-n --name <NAME> "Set the package name. Defaults to the directory name"))
            )
            .subcommand(
                Command::new("update")
                    .about("Update ProSA files to the latest skeleton")
                    .arg(arg!(--deb "Configure the ProSA to generate a deb package").action(clap::ArgAction::SetTrue))
            )
            .subcommand(
                Command::new("add")
                    .about("Add a ProSA processor")
                    .arg(arg!(--dry_run "Displays what would be updated, but doesn't actually write the ProSA files").action(clap::ArgAction::SetTrue))
                    .arg(arg!(-n --name <NAME> "Name of the processor schedule inside the ProSA (use the processor name by default)"))
                    .arg(arg!(-a --adaptor <ADAPTOR> "Adaptor name to use for the processor"))
                    .arg(arg!(<PROCESSOR> "Processor to add"))
                    .arg_required_else_help(true),
            )
            .subcommand(
                Command::new("remove")
                    .about("Remove one or more ProSA processor")
                    .arg(arg!(--dry_run "Displays what would be removed, but doesn't actually write the ProSA files").action(clap::ArgAction::SetTrue))
                    .arg(arg!(<PROCESSORS> ... "Processors to remove"))
                    .arg_required_else_help(true),
            )
            .subcommand(
                Command::new("main")
                    .about("Change the ProSA main processor")
                    .arg(arg!(--dry_run "Displays what would be removed, but doesn't actually write the ProSA files").action(clap::ArgAction::SetTrue))
                    .arg(arg!(<MAIN> "Name of the main processor"))
                    .arg_required_else_help(true),
            )
            .subcommand(
                Command::new("tvf")
                    .about("Change the ProSA TVF internal messaging")
                    .arg(arg!(--dry_run "Displays what would be removed, but doesn't actually write the ProSA files").action(clap::ArgAction::SetTrue))
                    .arg(arg!(<TVF> "Name of the TVF"))
                    .arg_required_else_help(true),
            )
            .subcommand(
                Command::new("list")
                    .about("List all available ProSA component")
            )
            .subcommand(
                Command::new("container")
                    .about("Create a container file to containerize ProSA")
                    .arg(arg!(--docker "Generate Dockerfile container format").action(clap::ArgAction::SetTrue))
                    .arg(arg!(-i --image <IMG> "Base image to use for ProSA container image").default_value("debian:stable-slim"))
                    .arg(arg!(-b --builder <BUILDER_IMG> "Builder to use to compile the ProSA"))
                    .arg(arg!(-p --package_manager <PKG_MANAGER> "Indicate which package manager to use with the Docker image to install pre-requisite").default_value("apt"))
                    .arg(arg!([PATH] "Path of the output container file to generate an image"))
            )
            .subcommand(
                Command::new("completion")
                    .about("Output shell completion code for the specified shell (Bash, Elvish, Fish, PowerShell, or Zsh)")
                    .arg(arg!(<SHELL>))
                    .arg_required_else_help(true),
            )
        )
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if let Some(("prosa", m)) = cli().get_matches().subcommand() {
        match m.subcommand() {
            Some(("new", matches)) => {
                let mut j2_context = tera::Context::new();
                let path = matches
                    .get_one::<String>("PATH")
                    .expect("required ProSA name");
                let mut args = vec!["new", "--bin"];
                if let Some(name) = matches.get_one::<String>("name") {
                    args.push("--name");
                    args.push(name);
                    j2_context.insert("name", name);
                } else {
                    j2_context.insert("name", path);
                }

                args.push(path);
                j2_context.insert("path", path);
                j2_context.insert("deb_pkg", &matches.get_flag("deb"));

                // Create the new Rust project
                let cargo_new = std::process::Command::new("cargo").args(args).output()?;

                io::stdout().write_all(&cargo_new.stdout).unwrap();
                io::stderr().write_all(&cargo_new.stderr).unwrap();

                if cargo_new.status.success() {
                    init_prosa(path, &j2_context)?;
                }
            }
            Some(("init", matches)) => {
                let mut j2_context = tera::Context::new();
                let current_path = env::current_dir()?;
                let path = current_path.as_path();
                let mut args = vec!["init", "--bin"];
                if let Some(name) = matches.get_one::<String>("name") {
                    args.push("--name");
                    args.push(name);
                    j2_context.insert("name", name);
                } else if let Some(name) = path.file_name() {
                    j2_context.insert("name", &tera::Value::String(name.to_str().unwrap().into()));
                }

                j2_context.insert("deb_pkg", &matches.get_flag("deb"));

                if let Some(path_name) = path.to_str() {
                    j2_context.insert("path", path_name);

                    // Init the Rust project
                    let cargo_init = std::process::Command::new("cargo").args(args).output()?;

                    io::stdout().write_all(&cargo_init.stdout).unwrap();
                    io::stderr().write_all(&cargo_init.stderr).unwrap();

                    if cargo_init.status.success() {
                        init_prosa(path_name, &j2_context)?;
                    }
                } else {
                    return Err(Box::new(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "Wrong current dir path format",
                    )));
                }
            }
            Some(("update", matches)) => {
                let package_metadata = CargoMetadata::load_package_metadata()?;
                let mut j2_context = tera::Context::new();
                package_metadata.j2_context(&mut j2_context);
                if !j2_context.contains_key("deb_pkg") {
                    j2_context.insert("deb_pkg", &matches.get_flag("deb"));
                }

                if let Some(path_name) = env::current_dir()?.as_path().to_str() {
                    j2_context.insert("path", path_name);
                    init_prosa(path_name, &j2_context)?;
                } else {
                    return Err(Box::new(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "Wrong current dir path format",
                    )));
                }
            }
            Some(("add", matches)) => {
                let dry_run = matches.get_flag("dry_run");
                let prosa_toml = fs::read_to_string(CONFIGURATION_FILENAME)?;
                let mut prosa_doc = prosa_toml.parse::<DocumentMut>()?;
                if let Some(processor) = matches.get_one::<String>("PROCESSOR") {
                    if let Some(proc_metadata) = CargoMetadata::load_metadata()?
                        .prosa_proc_metadata()
                        .get(processor)
                    {
                        let mut proc_desc = proc_metadata.get_proc_desc(
                            matches.get_one::<String>("adaptor").map(|x| x.as_str()),
                            None,
                        )?;
                        if let Some(name) = matches.get_one::<String>("name") {
                            proc_desc.name = Some(name.clone());
                        } else if proc_desc.name.is_none() {
                            proc_desc.name = Some(processor.clone());
                        }

                        // Use the processor name instead of the crate name
                        proc_desc.proc_name = processor.clone();

                        if !dry_run {
                            if let Some(toml_edit::Item::ArrayOfTables(array_tables)) =
                                prosa_doc.get_mut("proc")
                            {
                                array_tables.push(proc_desc.into());
                            } else {
                                let mut array_tables = toml_edit::ArrayOfTables::new();
                                array_tables.push(proc_desc.into());
                                prosa_doc
                                    .insert("proc", toml_edit::Item::ArrayOfTables(array_tables));
                            }

                            let mut prosa_toml_file = fs::File::create(CONFIGURATION_FILENAME)?;
                            prosa_toml_file.write_all(prosa_doc.to_string().as_bytes())?;
                        } else {
                            println!("Will add {}", proc_desc);
                        }
                    }
                }
            }
            Some(("remove", matches)) => {
                let dry_run = matches.get_flag("dry_run");
                let processors: HashSet<&String> = matches
                    .get_many::<String>("PROCESSORS")
                    .unwrap_or_default()
                    .collect();

                let prosa_toml = fs::read_to_string(CONFIGURATION_FILENAME)?;
                let mut prosa_doc = prosa_toml.parse::<DocumentMut>()?;
                if let Some(toml_edit::Item::ArrayOfTables(array_tables)) =
                    prosa_doc.get_mut("proc")
                {
                    array_tables.retain(|table| {
                        if let Some(toml_edit::Item::Value(toml_edit::Value::String(name))) =
                            table.get("name")
                        {
                            if processors.contains(&name.value()) {
                                if dry_run {
                                    println!("Will remove {}", name.value());
                                } else {
                                    return false;
                                }
                            }
                        }

                        true
                    });
                }

                if !dry_run {
                    let mut prosa_toml_file = fs::File::create(CONFIGURATION_FILENAME)?;
                    prosa_toml_file.write_all(prosa_doc.to_string().as_bytes())?;
                }
            }
            Some(("main", matches)) => {
                let dry_run = matches.get_flag("dry_run");
                let prosa_toml = fs::read_to_string(CONFIGURATION_FILENAME)?;
                let mut prosa_doc = prosa_toml.parse::<DocumentMut>()?;
                if let Some(main_name) = matches.get_one::<String>("MAIN") {
                    for main in CargoMetadata::load_metadata()?.prosa_main() {
                        if main.contains(main_name) {
                            if !dry_run {
                                if let Some(toml_edit::Item::Table(table)) =
                                    prosa_doc.get_mut("prosa")
                                {
                                    table.insert(
                                        "main",
                                        toml_edit::Item::Value(toml_edit::Value::String(
                                            toml_edit::Formatted::new(main),
                                        )),
                                    );
                                }

                                let mut prosa_toml_file = fs::File::create(CONFIGURATION_FILENAME)?;
                                prosa_toml_file.write_all(prosa_doc.to_string().as_bytes())?;
                                break;
                            } else {
                                println!("Will replace main proc with {}", main);
                                break;
                            }
                        }
                    }
                }
            }
            Some(("tvf", matches)) => {
                let dry_run = matches.get_flag("dry_run");
                let prosa_toml = fs::read_to_string(CONFIGURATION_FILENAME)?;
                let mut prosa_doc = prosa_toml.parse::<DocumentMut>()?;
                if let Some(tvf_name) = matches.get_one::<String>("TVF") {
                    for tvf in CargoMetadata::load_metadata()?.prosa_tvf() {
                        if tvf.contains(tvf_name) {
                            if !dry_run {
                                if let Some(toml_edit::Item::Table(table)) =
                                    prosa_doc.get_mut("prosa")
                                {
                                    table.insert(
                                        "tvf",
                                        toml_edit::Item::Value(toml_edit::Value::String(
                                            toml_edit::Formatted::new(tvf),
                                        )),
                                    );
                                }

                                let mut prosa_toml_file = fs::File::create(CONFIGURATION_FILENAME)?;
                                prosa_toml_file.write_all(prosa_doc.to_string().as_bytes())?;
                                break;
                            } else {
                                println!("Will replace TVF format with {}", tvf);
                                break;
                            }
                        }
                    }
                }
            }
            Some(("list", _matches)) => {
                let cargo_metadata = CargoMetadata::load_metadata()?;
                print!("{}", cargo_metadata);
            }
            Some(("container", matches)) => {
                let container = ContainerFile::new(matches)?;
                container.create_container_file()?;

                // Help on use
                print!("{}", container);
            }
            Some(("completion", matches)) => {
                let shell = clap_complete::Shell::from_str(
                    matches
                        .get_one::<String>("SHELL")
                        .expect("required")
                        .as_str(),
                )
                .expect("Unkown shell");
                clap_complete::generate(shell, &mut cli(), "cargo", &mut std::io::stdout())
            }
            _ => unreachable!(), // All subcommands are defined above, anything else is unreachable!()
        }
    } else {
        unreachable!(); // This should be unreachable because the command is use with cargo
    }

    Ok(())
}
