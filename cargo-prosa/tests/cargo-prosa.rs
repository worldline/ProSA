use std::path::PathBuf;
use std::{env, fs};

use assert_cmd::{Command, cargo};
use cargo_prosa::CONFIGURATION_FILENAME;
use predicates::Predicate;
use predicates::prelude::predicate;

/// Getter of a ProSA cargo command to test
fn cargo_prosa_command() -> Result<Command, cargo::CargoError> {
    let mut cmd = Command::cargo_bin(env!("CARGO_PKG_NAME"))?;
    cmd.arg("prosa");
    Ok(cmd)
}

/// To test the dummy ProSA, we need to change the dependencies to take the local one
fn replace_prosa_dependencies(prosa_path: &PathBuf) {
    let mut test_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    test_path.pop();
    for (prosa_dep, prosa_dep_path, build_opt) in [
        ("prosa-utils", "prosa_utils", None),
        ("prosa", "prosa", None),
        ("cargo-prosa", "cargo-prosa", Some(["--build"])),
    ] {
        let test_prosa_dep = test_path.join(prosa_dep_path);
        let mut cmd = Command::new("cargo");
        cmd.arg("add");
        cmd.current_dir(prosa_path);
        if let Some(opt) = build_opt {
            cmd.args(opt);
        }
        cmd.args(["--path", test_prosa_dep.to_str().unwrap(), prosa_dep]);
        cmd.assert().success();
    }
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
    cmd.args(["new", "--deb", PROSA_NAME]);
    cmd.assert()
        .success()
        .stderr(predicate::str::contains(format!(
            "binary (application) `{PROSA_NAME}` package"
        )));
    replace_prosa_dependencies(&prosa_path);

    // List all component available for ProSA
    let mut cmd = cargo_prosa_command()?;
    cmd.current_dir(&prosa_path);
    cmd.arg("list");
    cmd.assert().success().stdout(predicate::str::is_match(
        r"Package prosa\[[0-9].[0-9].[0-9]\] \(ProSA core\)
  - inj
    Processor inj::proc::InjProc
    Settings inj::proc::InjSettings
    Adaptor:
     - inj::adaptor::InjDummyAdaptor
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

    let predicate_stub_proc = predicate::str::contains(
        "[[proc]]\nname = \"stub-1\"\nproc_name = \"stub\"\nproc = \"prosa::stub::proc::StubProc\"\nadaptor = \"prosa::stub::adaptor::StubParotAdaptor\"",
    );
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

    // Test if build have generated everything
    assert!(
        prosa_path
            .join("target")
            .join("prosa-deb")
            .join("service")
            .exists()
    );

    // Try to update the ProSA
    let build_path = prosa_path.join("build.rs");
    let _ = fs::remove_file(&build_path);
    assert!(!build_path.exists());
    let mut cmd = cargo_prosa_command()?;
    cmd.current_dir(&prosa_path);
    cmd.arg("update");
    cmd.assert().success();
    assert!(build_path.exists());

    // Try to generate container files
    let (containerfile_path, dockerfile_path) = (
        prosa_path.join("Containerfile"),
        prosa_path.join("Dockerfile"),
    );
    let _ = fs::remove_file(&containerfile_path);
    let _ = fs::remove_file(&dockerfile_path);
    assert!(!containerfile_path.exists() && !dockerfile_path.exists());
    let mut cmd = cargo_prosa_command()?;
    cmd.current_dir(&prosa_path);
    cmd.args(["container", containerfile_path.to_str().unwrap()]);
    cmd.assert().success().stdout(predicate::str::is_match(
        r"To build your container, use the command:
  `podman build -f .*/dummy-test-prosa/Containerfile -t dummy-test-prosa:0\.1\.0 \.`",
    )?);
    assert!(containerfile_path.exists());
    let mut cmd = cargo_prosa_command()?;
    cmd.current_dir(&prosa_path);
    cmd.args([
        "container",
        "--docker",
        "-b rust-latest",
        dockerfile_path.to_str().unwrap(),
    ]);
    cmd.assert().success().stdout(predicate::str::is_match(
        r"To build your container, use the command:
  `docker build -f .*/dummy-test-prosa/Dockerfile -t dummy-test-prosa:0\.1\.0 \.`
If you have an external git dependency, specify your ssh agent with:
  `docker build -f .*/dummy-test-prosa/Dockerfile --ssh default=\$SSH_AUTH_SOCK -t dummy-test-prosa:0\.1\.0 \.`",
    )?);
    assert!(dockerfile_path.exists());

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
    replace_prosa_dependencies(&prosa_path);

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
