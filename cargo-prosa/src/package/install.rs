use std::{
    env, fmt, fs,
    io::{self, Write as _},
    path::{Path, PathBuf},
};

use clap::ArgMatches;
use tera::Tera;
use toml_edit::DocumentMut;

use crate::cargo::CargoMetadata;

#[cfg(target_os = "macos")]
const ASSETS_LAUNCHD_J2: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/launchd.j2"));

/// Struct to handle ProSA instance installation
pub struct InstanceInstall {
    name: String,
    bin_name: String,
    install_bin_dir: String,
    install_config_dir: String,
    install_service_dir: String,
    ctx: tera::Context,
    j2_service_asset: &'static str,
}

impl InstanceInstall {
    /// Create an instance install builder to install the ProSA instance locally
    pub fn new(args: &ArgMatches) -> io::Result<InstanceInstall> {
        let current_path = env::current_dir()?;
        let path = current_path.as_path();
        let name = args
            .get_one::<String>("name")
            .map(|n| n.as_str())
            .or(path.file_name().and_then(|p| p.to_str()))
            .unwrap_or("prosa")
            .to_lowercase()
            .replace(['/', ' '], "_");

        // Get all path for system or home installation
        let (install_bin_dir, install_config_dir, install_service_dir) = if args.get_flag("system")
        {
            #[cfg(target_os = "linux")]
            {
                (
                    "/usr/local/bin".to_string(),
                    "/etc/prosa".to_string(),
                    "/etc/systemd/system".to_string(),
                )
            }
            #[cfg(target_os = "macos")]
            {
                (
                    "/usr/local/bin".to_string(),
                    "/etc/prosa".to_string(),
                    "/Library/LaunchDaemons".to_string(),
                )
            }
        } else {
            let install_dir = env::var("HOME").map_err(|ve| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!(
                        "Can't determine the $HOME folder where to install the ProSA {name}: {ve}"
                    ),
                )
            })?;

            // Avoid empty or root dir
            if install_dir.len() < 2 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("Can't install with an empty or root $HOME: {install_dir}"),
                ));
            }

            #[cfg(target_os = "linux")]
            {
                (
                    format!("{install_dir}/.local/bin"),
                    format!("{install_dir}/.config/prosa"),
                    format!("{install_dir}/.config/systemd/user"),
                )
            }
            #[cfg(target_os = "macos")]
            {
                (
                    format!("{install_dir}/.local/bin"),
                    format!("{install_dir}/.config/prosa"),
                    format!("{install_dir}/Library/LaunchAgents"),
                )
            }
        };

        let package_metadata = CargoMetadata::load_package_metadata()?;
        let mut ctx = tera::Context::new();
        package_metadata.j2_context(&mut ctx);
        ctx.insert("name", &name);

        let bin_name = package_metadata
            .get_targets("bin")
            .and_then(|b| b.first().cloned())
            .ok_or(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Can't find the ProSA binary from the project",
            ))?;
        ctx.insert("bin", &format!("{install_bin_dir}/{bin_name}"));
        ctx.insert(
            "config",
            &format!("{install_config_dir}/{}/prosa.toml", name),
        );

        // Add a description if it don't exist (it's mandatory for launchd)
        if !ctx.contains_key("description") {
            ctx.insert("description", "Local ProSA instance");
        }

        Ok(InstanceInstall {
            name,
            bin_name,
            install_bin_dir,
            install_config_dir,
            install_service_dir,
            ctx,
            #[cfg(target_os = "linux")]
            j2_service_asset: super::ASSETS_SYSTEMD_J2,
            #[cfg(target_os = "macos")]
            j2_service_asset: ASSETS_LAUNCHD_J2,
        })
    }

    fn get_install_bin_dir(&self) -> PathBuf {
        PathBuf::from(self.install_bin_dir.clone())
    }

    fn get_install_config_path(&self) -> PathBuf {
        PathBuf::from(self.install_config_dir.clone())
    }

    fn get_install_service_path(&self) -> PathBuf {
        PathBuf::from(self.install_service_dir.clone())
    }

    #[cfg(target_os = "linux")]
    fn get_service_filename(&self) -> String {
        format!("{}.service", self.name)
    }

    #[cfg(target_os = "macos")]
    fn get_service_filename(&self) -> String {
        format!("com.prosa.{}.plist", self.name)
    }

    fn create_service_file(&self) -> tera::Result<()> {
        let service_name = self.get_service_filename();
        let service_path = self.get_install_service_path();
        let service_file_path = service_path.join(&service_name);

        let mut tera_build = Tera::default();
        tera_build.add_raw_template(&service_name, self.j2_service_asset)?;

        fs::create_dir_all(&service_path)?;
        let service_file = fs::File::create(service_file_path)?;
        tera_build.render_to(&service_name, &self.ctx, service_file)
    }

    fn copy_binary(&self, release: bool) -> io::Result<u64> {
        let binary_path = if release {
            format!("target/release/{}", self.bin_name)
        } else {
            format!("target/debug/{}", self.bin_name)
        };

        // If the binary don't exist, compile the Rust project with cargo
        match fs::exists(Path::new(&binary_path)) {
            Ok(true) => {}
            _ => {
                let build_args = if release {
                    vec!["build", "--release"]
                } else {
                    vec!["build"]
                };

                let cargo_build = std::process::Command::new("cargo")
                    .args(build_args)
                    .output()?;
                io::stdout().write_all(&cargo_build.stdout).unwrap();
                io::stderr().write_all(&cargo_build.stderr).unwrap();

                if !cargo_build.status.success() {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "Error during ProSA build",
                    ));
                }
            }
        }

        // Copy the binary to the local output directory
        let binary_output_path = self.get_install_bin_dir();
        fs::create_dir_all(&binary_output_path)?;
        fs::copy(
            binary_path,
            binary_output_path.join(Path::new(&self.bin_name)),
        )
    }

    fn gen_config(&self) -> io::Result<u64> {
        let config_dir = self.get_install_config_path().join(&self.name);
        fs::create_dir_all(&config_dir)?;

        // If the configuration file already exist, only add the missing parts to avoid removing some already configured things
        let config_path = config_dir.join("prosa.toml");
        if let Ok(true) = fs::exists(&config_path)
            && let Ok(new_config_toml) =
                fs::read_to_string("target/config.toml")?.parse::<DocumentMut>()
            && let Ok(mut config_toml) = fs::read_to_string(&config_path)?.parse::<DocumentMut>()
        {
            let mut modified = false;
            let config_table = config_toml.as_table_mut();
            for (new_config_key, new_config_item) in new_config_toml.as_table() {
                if !config_table.contains_key(new_config_key) {
                    config_table.insert(new_config_key, new_config_item.clone());
                    modified = true;
                }
            }

            // Override with the new configuration if any value have changed
            if modified {
                let mut config_toml_file = fs::File::create(config_path)?;
                let config_toml_str = config_toml.to_string();
                config_toml_file.write_all(config_toml_str.as_bytes())?;
                Ok(config_toml_str.len() as u64)
            } else {
                Ok(0)
            }
        } else {
            fs::copy("target/config.toml", config_path)
        }
    }

    /// Method to install ProSA on the system
    /// - Create a service file
    /// - Copy ProSA binary
    /// - Generate configuration
    pub fn install(&self, release: bool) -> io::Result<u64> {
        print!("Creating service ");
        self.create_service_file()
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        println!("OK");
        print!("Copying binary ");
        let mut file_size = self.copy_binary(release)?;
        println!("OK");
        print!("Generating configuration ");
        file_size += self.gen_config()?;
        println!("OK");
        Ok(file_size)
    }

    /// Method to remove ProSA from the system
    /// Let the configuration file as it is to avoid losing any configuration
    pub fn uninstall(&self, purge: bool) -> io::Result<()> {
        if purge {
            print!("Purge configuration file ");
            fs::remove_dir_all(self.get_install_config_path().join(&self.name))?;
            println!("OK");
        }

        print!("Remove service ");
        fs::remove_file(
            self.get_install_service_path()
                .join(self.get_service_filename()),
        )?;
        println!("OK");

        print!("Remove binary ");
        fs::remove_file(self.get_install_bin_dir().join(&self.bin_name))?;
        println!("OK");
        Ok(())
    }
}

impl fmt::Display for InstanceInstall {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(f, "ProSA `{}`", self.name)?;
        writeln!(
            f,
            "Binary file : {}/{}",
            self.install_bin_dir, self.bin_name
        )?;
        writeln!(
            f,
            "Config file : {}/{}/prosa.toml",
            self.install_config_dir, self.name
        )?;
        writeln!(
            f,
            "Service file: {}/{}",
            self.install_service_dir,
            self.get_service_filename()
        )
    }
}
