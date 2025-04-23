use std::{
    fs, io,
    path::{Path, PathBuf},
};

use tera::Tera;

use crate::cargo::CargoMetadata;

/// Struct to handle Container file creation
pub struct RpmPkg {
    path: PathBuf,
    ctx: tera::Context,
}

impl RpmPkg {
    const RPM_DATA_TARGET: &'static str = "prosa-rpm";

    /// Create a Red Hat package builder from build.rs script
    pub fn new(path: PathBuf) -> io::Result<RpmPkg> {
        let package_metadata = CargoMetadata::load_package_metadata()?;
        let mut ctx = tera::Context::new();
        package_metadata.j2_context(&mut ctx);

        // Add package build context
        ctx.insert(
            "config",
            &format!("/etc/ProSA/{}.yml", package_metadata.name),
        );
        ctx.insert("bin", &format!("/usr/bin/{}", package_metadata.name));

        Ok(RpmPkg { path, ctx })
    }

    fn get_binary_assets(name: &str) -> toml_edit::InlineTable {
        let mut binary_assets = toml_edit::InlineTable::new();
        binary_assets.insert(
            "source",
            toml_edit::Value::String(toml_edit::Formatted::new(format!(
                "target/release/{}",
                name
            ))),
        );
        binary_assets.insert(
            "dest",
            toml_edit::Value::String(toml_edit::Formatted::new(format!("/usr/bin/{}", name))),
        );
        binary_assets.insert(
            "mode",
            toml_edit::Value::String(toml_edit::Formatted::new("755".to_string())),
        );
        binary_assets
    }

    fn get_config_assets(name: &str) -> toml_edit::InlineTable {
        let mut config_assets = toml_edit::InlineTable::new();
        config_assets.insert(
            "source",
            toml_edit::Value::String(toml_edit::Formatted::new(format!(
                "target/{}/{}.yml",
                Self::RPM_DATA_TARGET,
                name
            ))),
        );
        config_assets.insert(
            "dest",
            toml_edit::Value::String(toml_edit::Formatted::new("/etc/ProSA/".to_string())),
        );
        config_assets.insert(
            "mode",
            toml_edit::Value::String(toml_edit::Formatted::new("644".to_string())),
        );
        config_assets
    }

    fn get_systemd_assets(name: &str) -> toml_edit::InlineTable {
        let mut systemd_assets = toml_edit::InlineTable::new();
        systemd_assets.insert(
            "source",
            toml_edit::Value::String(toml_edit::Formatted::new(format!(
                "target/{}/service",
                Self::RPM_DATA_TARGET
            ))),
        );
        systemd_assets.insert(
            "dest",
            toml_edit::Value::String(toml_edit::Formatted::new(format!(
                "/etc/systemd/system/{}.service",
                name
            ))),
        );
        systemd_assets.insert(
            "mode",
            toml_edit::Value::String(toml_edit::Formatted::new("644".to_string())),
        );
        systemd_assets
    }

    fn get_readme_assets(name: &str) -> toml_edit::InlineTable {
        let mut readme_assets = toml_edit::InlineTable::new();
        readme_assets.insert(
            "source",
            toml_edit::Value::String(toml_edit::Formatted::new("README.md".to_string())),
        );
        readme_assets.insert(
            "dest",
            toml_edit::Value::String(toml_edit::Formatted::new(format!(
                "/usr/share/doc/{}/README",
                name
            ))),
        );
        readme_assets.insert(
            "mode",
            toml_edit::Value::String(toml_edit::Formatted::new("644".to_string())),
        );
        readme_assets
    }

    /// Function to add Red Hat package metadata to `Cargo.toml`
    pub fn add_rpm_pkg_metadata(deb_table: &mut toml_edit::Table, name: &str) {
        if !deb_table.contains_key("assets") {
            // Add every assets properties to deb table
            let mut assets = toml_edit::Array::new();

            assets.push(Self::get_binary_assets(name));
            assets.push(Self::get_config_assets(name));
            assets.push(Self::get_systemd_assets(name));

            if Path::new("README.md").is_file() {
                assets.push(Self::get_readme_assets(name));
            }

            deb_table.insert("assets", toml_edit::Item::Value(assets.into()));
        }
    }

    /// Method to write package data (useful for the deb package) into a folder
    pub fn write_package_data(&self) -> io::Result<()> {
        let name = self
            .ctx
            .get("name")
            .and_then(|n| n.as_str())
            .ok_or(io::Error::new(
                io::ErrorKind::InvalidData,
                "Missing package name",
            ))?;
        let pkg_data_path = self.path.join(Self::RPM_DATA_TARGET);
        fs::create_dir_all(&pkg_data_path)?;

        // Copy configuration file
        fs::copy(
            self.path.join("config.yml"),
            pkg_data_path.join(format!("{}.yml", name)),
        )?;

        // Write systemd file
        let mut tera_build = Tera::default();
        tera_build
            .add_raw_template(
                "prosa.service",
                include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/systemd.j2")),
            )
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        let main_file = fs::File::create(pkg_data_path.join("service"))?;
        tera_build
            .render_to("prosa.service", &self.ctx, main_file)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }
}
