use std::{
    fs, io,
    path::{Path, PathBuf},
};

use tera::Tera;

use crate::cargo::CargoMetadata;

/// Struct to handle Container file creation
pub struct DebPkg {
    path: PathBuf,
    ctx: tera::Context,
}

impl DebPkg {
    const DEB_DATA_TARGET: &'static str = "prosa-deb";

    /// Create a debian package builder from build.rs script
    pub fn new(path: PathBuf) -> io::Result<DebPkg> {
        let package_metadata = CargoMetadata::load_package_metadata()?;
        let mut ctx = tera::Context::new();
        package_metadata.j2_context(&mut ctx);

        // Add package build context
        ctx.insert(
            "config",
            &format!("/etc/ProSA/{}.yml", package_metadata.name),
        );
        ctx.insert("bin", &format!("/usr/bin/{}", package_metadata.name));

        Ok(DebPkg { path, ctx })
    }

    fn get_binary_assets(name: &str) -> toml_edit::Array {
        let mut binary_assets = toml_edit::Array::new();
        binary_assets.push(format!("target/release/{name}"));
        binary_assets.push("usr/bin/");
        binary_assets.push("755");
        binary_assets
    }

    fn get_config_assets(name: &str) -> toml_edit::Array {
        let mut config_assets = toml_edit::Array::new();
        config_assets.push(format!("target/{}/{}.yml", Self::DEB_DATA_TARGET, name));
        config_assets.push("etc/ProSA/");
        config_assets.push("644");
        config_assets
    }

    fn get_readme_assets(name: &str) -> toml_edit::Array {
        let mut readme_assets = toml_edit::Array::new();
        readme_assets.push("README.md");
        readme_assets.push(format!("usr/share/doc/{name}/README"));
        readme_assets.push("644");
        readme_assets
    }

    /// Function to add debian package metadata to `Cargo.toml`
    pub fn add_deb_pkg_metadata(deb_table: &mut toml_edit::Table, name: &str) {
        if !deb_table.contains_key("depends") {
            deb_table.insert(
                "depends",
                toml_edit::Item::Value(toml_edit::Value::String(toml_edit::Formatted::new(
                    "$auto, libssl3".to_string(),
                ))),
            );
        }

        if !deb_table.contains_key("maintainer-scripts") {
            deb_table.insert(
                "maintainer-scripts",
                toml_edit::Item::Value(format!("target/{}/", Self::DEB_DATA_TARGET).into()),
            );
        }

        if !deb_table.contains_key("assets") {
            // Add every assets properties to deb table
            let mut assets = toml_edit::Array::new();

            assets.push(Self::get_binary_assets(name));
            assets.push(Self::get_config_assets(name));

            if Path::new("README.md").is_file() {
                assets.push(Self::get_readme_assets(name));
            }

            deb_table.insert("assets", toml_edit::Item::Value(assets.into()));
        }

        if let Some(toml_edit::Item::Value(toml_edit::Value::InlineTable(systemd_units))) =
            deb_table.get_mut("systemd-units")
        {
            if !systemd_units.contains_key("enable") {
                systemd_units.insert(
                    "enable",
                    toml_edit::Value::Boolean(toml_edit::Formatted::new(true)),
                );
            }
        } else {
            let mut inline_table = toml_edit::InlineTable::new();

            inline_table.insert(
                "enable",
                toml_edit::Value::Boolean(toml_edit::Formatted::new(true)),
            );

            deb_table.insert(
                "systemd-units",
                toml_edit::Item::Value(toml_edit::Value::InlineTable(inline_table)),
            );
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
        let pkg_data_path = self.path.join(Self::DEB_DATA_TARGET);
        fs::create_dir_all(&pkg_data_path)?;

        // Copy configuration file
        fs::copy(
            self.path.join("config.yml"),
            pkg_data_path.join(format!("{name}.yml")),
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
