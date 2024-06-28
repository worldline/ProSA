use std::{env, fs};

use assert_cmd::{cargo, Command};
use cargo_prosa::CONFIGURATION_FILENAME;
use predicates::prelude::predicate;
use predicates::Predicate;

/// Getter of a ProSA cargo command to test
fn cargo_prosa_command() -> Result<Command, cargo::CargoError> {
    let mut cmd = Command::cargo_bin(env!("CARGO_PKG_NAME"))?;
    cmd.arg("prosa");
    Ok(cmd)
}

#[test]
fn errors() -> Result<(), Box<dyn std::error::Error>> {
    // Try unknown command
    let mut cmd = cargo_prosa_command()?;
    cmd.arg("dummy");
    cmd.assert().failure().code(2).stderr("error: unrecognized subcommand 'dummy'\n\nUsage: cargo prosa <COMMAND>\n\nFor more information, try '--help'.\n");

    Ok(())
}

#[test]
fn project() -> Result<(), Box<dyn std::error::Error>> {
    const PROSA_NAME: &str = "dummy-test-prosa";
    let temp_dir = env::temp_dir();
    let prosa_path = temp_dir.join(PROSA_NAME);
    let prosa_toml_path = prosa_path.join(CONFIGURATION_FILENAME);

    // Clean test files
    let _ = fs::remove_dir_all(&prosa_path);

    // Generate a dummy project
    let mut cmd = cargo_prosa_command()?;
    cmd.current_dir(&temp_dir);
    cmd.args(["new", PROSA_NAME]);
    cmd.assert()
        .success()
        .stderr(predicate::str::contains(format!(
            "binary (application) `{}` package",
            PROSA_NAME
        )));

    // List all component available for ProSA
    let mut cmd = cargo_prosa_command()?;
    cmd.current_dir(&prosa_path);
    cmd.arg("list");
    cmd.assert().success().stdout(predicate::str::is_match(
        r"Package prosa\[[0-9].[0-9].[0-9]\] \(ProSA core\)
  - main
    - core::main::MainProc
  - stub
    Processor stub::proc::StubProc
    Settings stub::proc::StubSettings
    Adaptor:
     - stub::adaptor::StubParotAdaptor
Package prosa-utils\[[0-9].[0-9].[0-9]\] \(ProSA utils\)
  - tvf
    - msg::simple_string_tvf::SimpleStringTvf",
    )?);

    // Add a stub processor (dry_run)
    let mut cmd = cargo_prosa_command()?;
    cmd.current_dir(&prosa_path);
    cmd.args([
        "add",
        "--dry_run",
        "-n",
        "stub-1",
        "-a",
        "StubParotAdaptor",
        "stub",
    ]);
    cmd.assert().success().stdout("Will add ProSA processor stub-1 (stub)\n  Processor prosa::stub::proc::StubProc\n  Adaptor prosa::stub::adaptor::StubParotAdaptor\n\n");

    let predicate_stub_proc = predicate::str::contains("[[proc]]\nname = \"stub-1\"\nproc_name = \"stub\"\nproc = \"prosa::stub::proc::StubProc\"\nadaptor = \"prosa::stub::adaptor::StubParotAdaptor\"");
    assert!(!predicate_stub_proc.eval(fs::read_to_string(&prosa_toml_path)?.as_str()));

    // Add a stub processor
    let mut cmd = cargo_prosa_command()?;
    cmd.current_dir(&prosa_path);
    cmd.args(["add", "-n", "stub-1", "-a", "StubParotAdaptor", "stub"]);
    cmd.assert().success();
    assert!(predicate_stub_proc.eval(fs::read_to_string(&prosa_toml_path)?.as_str()));

    // Change the main task processor (dry_run)
    let mut cmd = cargo_prosa_command()?;
    cmd.current_dir(&prosa_path);
    cmd.args(["main", "--dry_run", "MainProc"]);
    cmd.assert()
        .success()
        .stdout("Will replace main proc with prosa::core::main::MainProc\n");

    // Change the main task processor
    let mut cmd = cargo_prosa_command()?;
    cmd.current_dir(&prosa_path);
    cmd.args(["main", "MainProc"]);
    cmd.assert().success();

    // Change the tvf use for ProSA (dry_run)
    let mut cmd = cargo_prosa_command()?;
    cmd.current_dir(&prosa_path);
    cmd.args(["tvf", "--dry_run", "SimpleStringTvf"]);
    cmd.assert().success().stdout(
        "Will replace TVF format with prosa_utils::msg::simple_string_tvf::SimpleStringTvf\n",
    );

    // Change the tvf use for ProSA
    let mut cmd = cargo_prosa_command()?;
    cmd.current_dir(&prosa_path);
    cmd.args(["tvf", "SimpleStringTvf"]);
    cmd.assert().success();

    // Try to build the ProSA
    let mut cmd = Command::new("cargo");
    cmd.arg("build");
    cmd.current_dir(&prosa_path);
    cmd.assert().success();

    // Try to update the ProSA
    let build_path = prosa_path.join("build.rs");
    let _ = fs::remove_file(&build_path);
    assert!(!build_path.exists());
    let mut cmd = cargo_prosa_command()?;
    cmd.current_dir(&prosa_path);
    cmd.arg("update");
    cmd.assert().success();
    assert!(build_path.exists());

    // Remove a stub processor (dry_run)
    let mut cmd = cargo_prosa_command()?;
    cmd.current_dir(&prosa_path);
    cmd.args(["remove", "--dry_run", "stub-1"]);
    cmd.assert().success().stdout("Will remove stub-1\n");
    assert!(predicate_stub_proc.eval(fs::read_to_string(&prosa_toml_path)?.as_str()));

    // Remove a stub processor
    let mut cmd = cargo_prosa_command()?;
    cmd.current_dir(&prosa_path);
    cmd.args(["remove", "stub-1"]);
    cmd.assert().success();
    assert!(!predicate_stub_proc.eval(fs::read_to_string(&prosa_toml_path)?.as_str()));

    // Try to init the ProSA
    let _ = fs::remove_file(&build_path);
    let _ = fs::remove_file(prosa_path.join("Cargo.toml"));
    assert!(!build_path.exists());
    let mut cmd = cargo_prosa_command()?;
    cmd.current_dir(&prosa_path);
    cmd.arg("init");
    cmd.assert().success();
    assert!(build_path.exists());

    // Get Bash command completion
    let mut cmd = cargo_prosa_command()?;
    cmd.current_dir(&prosa_path);
    cmd.args(["completion", "bash"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("cargo__prosa"));

    // Get Zsh command completion
    let mut cmd = cargo_prosa_command()?;
    cmd.current_dir(&prosa_path);
    cmd.args(["completion", "zsh"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("cargo__prosa"));

    Ok(())
}
