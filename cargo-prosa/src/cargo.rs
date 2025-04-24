//! Tools for Cargo
//!
//! Cargo contain the format of cargo-metadata to retrieve all ProSA dependencies infos.
//!
//! To declare a Main task from your Cargo.toml project (don't include your crate name):
//! ```toml
//! [package.metadata.prosa]
//! main = ["core::main::MainProc"]
//! ```
//!
//! To declare a TVF format from your Cargo.toml project (don't include your crate name):
//! ```toml
//! [package.metadata.prosa]
//! tvf = ["msg::simple_string_tvf::SimpleStringTvf"]
//! ```
//!
//! To declare a processor from your Cargo.toml project (don't include your crate name):
//! ```toml
//! [package.metadata.prosa.myproc]
//! proc = "MyProc"
//! settings = "MySettings"
//! adaptor = ["MyAdaptor1", "MyAdaptor2"]
//! ```
//! or only an adaptor for an existing processor
//! ```toml
//! [package.metadata.prosa.myproc]
//! adaptor = ["MyCustomAdaptor"]
//! ```

use std::{collections::HashMap, fmt, io};

use serde::Deserialize;

use crate::builder::ProcDesc;

/// Structure to define ProSA component (processor/adaptor) version
#[derive(Debug, PartialEq)]
pub struct ComponentVersion<'a> {
    /// Name of the component
    pub name: String,
    /// Name of the component's crate
    pub crate_name: &'a str,
    /// Version of the component
    pub version: &'a str,
}

impl fmt::Display for ComponentVersion<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if f.alternate() {
            writeln!(f, "{}", self.name)?;
            writeln!(f, "  crate   {}", self.crate_name)?;
            writeln!(f, "  version {}", self.version)
        } else {
            write!(
                f,
                "{} = {{ crate = {}, version = {} }}",
                self.name, self.crate_name, self.version
            )
        }
    }
}

/// Metadata of a ProSA processor
#[derive(Debug, Clone, Deserialize)]
pub struct Metadata {
    /// Name of the ProSA package (not included in the metadata)
    #[serde(skip_deserializing)]
    pub name: Option<String>,
    /// Description of the ProSA package (not included in the metadata)
    #[serde(skip_deserializing)]
    pub description: Option<String>,
    /// Struct name of the ProSA processor
    pub proc: Option<String>,
    /// Struct name of the ProSA settings
    pub settings: Option<String>,
    /// Struct names of ProSA adpators
    pub adaptor: Option<Vec<String>>,
}

impl Metadata {
    /// Method to add the crate name, and description to the metadata
    fn specify(&mut self, crate_name: &str, description: Option<String>) {
        self.description = description;
        let crate_prefix = format!("{crate_name}::");

        if let Some(proc) = &mut self.proc {
            proc.insert_str(0, crate_prefix.as_str());
        }

        if let Some(settings) = &mut self.settings {
            settings.insert_str(0, crate_prefix.as_str());
        }

        if let Some(adaptor) = &mut self.adaptor {
            for adaptor in adaptor {
                adaptor.insert_str(0, crate_prefix.as_str());
            }
        }
    }

    /// Method to merge 2 metadata from the same processor
    pub fn merge(&mut self, prosa_metadata: Metadata) {
        if self.proc.is_none() {
            self.proc = prosa_metadata.proc;
        }

        if self.settings.is_none() {
            self.settings = prosa_metadata.settings;
        }

        if let Some(adaptor_list) = &mut self.adaptor {
            if let Some(prosa_adaptor_list) = prosa_metadata.adaptor {
                adaptor_list.extend(prosa_adaptor_list);
            }
        } else {
            self.adaptor = prosa_metadata.adaptor;
        }
    }

    /// Method to know if it's the processor from its name
    pub fn match_proc(&self, name: &str, crate_name: Option<&str>) -> Option<&String> {
        if let Some(proc) = &self.proc {
            let proc_name = proc.replace('-', "_");
            if let Some(crate_name) = crate_name {
                let proc_name = format!("{crate_name}::{proc_name}");
                if proc_name.contains(name) {
                    return Some(proc);
                }
            } else if proc_name.contains(name) {
                return Some(proc);
            }
        }

        None
    }

    /// Method to find an adaptor from its name
    pub fn find_adaptor(&self, name: &str, crate_name: Option<&str>) -> Option<&String> {
        if let Some(adaptors) = &self.adaptor {
            for adaptor in adaptors {
                let adaptor_name = adaptor.replace('-', "_");
                if let Some(crate_name) = crate_name {
                    let adaptor_name = format!("{crate_name}::{adaptor_name}");
                    if adaptor_name.contains(name) {
                        return Some(adaptor);
                    }
                } else if adaptor_name.contains(name) {
                    return Some(adaptor);
                }
            }
        }

        None
    }

    /// Get a ProSA processor description from the metadata
    pub fn get_proc_desc(
        &self,
        adaptor_name: Option<&str>,
        crate_name: Option<&str>,
    ) -> Result<ProcDesc, String> {
        let name = if let Some(name) = &self.name {
            Some(name.as_str())
        } else {
            self.proc
                .as_ref()
                .and_then(|p| p.rsplit_once(':').map(|(_, proc_name)| proc_name))
        }
        .ok_or(String::from("Missing ProSA `name` metadata"))?;

        let adaptor = if let Some(adaptor) =
            adaptor_name.and_then(|name| self.find_adaptor(name, crate_name))
        {
            Some(adaptor)
        } else if let Some(adaptors) = &self.adaptor {
            adaptors.first()
        } else {
            None
        }
        .ok_or(format!("Can't find a ProSA `adaptor` for {name}"))?;

        Ok(ProcDesc {
            name: None,
            proc_name: name.into(),
            proc: self
                .proc
                .clone()
                .map(|p| p.replace('-', "_"))
                .ok_or(format!("Missing ProSA `proc` metadata for {name}"))?,
            adaptor: adaptor.replace('-', "_"),
        })
    }
}

impl fmt::Display for Metadata {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if let Some(proc) = &self.proc {
            writeln!(f, "    Processor {proc}")?;
        }

        if let Some(settings) = &self.settings {
            writeln!(f, "    Settings {settings}")?;
        }

        if let Some(adaptor) = &self.adaptor {
            writeln!(f, "    Adaptor:")?;
            for adaptor in adaptor {
                writeln!(f, "     - {adaptor}")?;
            }
        }

        Ok(())
    }
}

/// Package metadata that describe a crate
#[derive(Debug, Deserialize)]
pub struct PackageMetadata {
    /// Name of the package
    pub name: String,
    /// Version of the package
    pub version: String,
    /// Package license
    pub license: Option<String>,
    /// Description of the package
    pub description: Option<String>,
    /// URL of the package documentation
    pub documentation: Option<String>,
    /// Authors of the package
    pub authors: Vec<String>,

    /// Metadata of the package
    metadata: Option<HashMap<String, serde_json::Value>>,
}

impl PackageMetadata {
    /// Know if a metadata key is present
    pub fn contain_metadata(&self, name: &str) -> bool {
        if let Some(metadata) = &self.metadata {
            metadata.contains_key(name)
        } else {
            false
        }
    }

    /// Know if the package contain ProSA metadata
    pub fn is_prosa(&self) -> bool {
        self.contain_metadata("prosa")
    }

    /// Getter of the ProSA Processor or Adaptor if present
    pub fn get_prosa_proc_metadata(&self) -> Option<HashMap<&str, Metadata>> {
        if let Some(metadata) = self
            .metadata
            .as_ref()
            .and_then(|m| m.get("prosa").and_then(|w| w.as_object()))
        {
            let mut proc_metadata = HashMap::new();
            for (name, data) in metadata {
                if name != "main" && name != "tvf" {
                    if let Ok(prosa_metadata) = serde_json::from_value::<Metadata>(data.clone()) {
                        proc_metadata.insert(name.as_str(), prosa_metadata);
                    }
                }
            }

            Some(proc_metadata)
        } else {
            None
        }
    }

    /// Getter of a ProSA metadata list
    fn get_prosa_metadata(&self, ty: &str) -> Vec<String> {
        let mut meta_list: Vec<String> = Vec::new();
        if let Some(metadata) = self
            .metadata
            .as_ref()
            .and_then(|m| m.get("prosa").and_then(|w| w.as_object()))
        {
            for (meta_name, data) in metadata {
                if meta_name == ty {
                    if let Ok(prosa_metadata) =
                        serde_json::from_value::<Vec<String>>(data.clone()).map(|v| v.into_iter())
                    {
                        meta_list.append(
                            &mut prosa_metadata
                                .map(|w| format!("{}::{}", self.name.replace('-', "_"), w))
                                .collect::<Vec<String>>(),
                        );
                    }
                }
            }
        }

        meta_list
    }

    /// Getter of the ProSA main list
    pub fn get_prosa_main(&self) -> Vec<String> {
        self.get_prosa_metadata("main")
    }

    /// Getter of the ProSA TVF list
    pub fn get_prosa_tvf(&self) -> Vec<String> {
        self.get_prosa_metadata("tvf")
    }

    /// Method to get a component version from its name if it exist
    fn get_version(&self, name: &str, ty: &str) -> Option<ComponentVersion> {
        if let Some(metadata) = self
            .metadata
            .as_ref()
            .and_then(|m| m.get("prosa").and_then(|w| w.as_object()))
        {
            for (meta_name, data) in metadata {
                if meta_name != "main" && meta_name != "tvf" && ty != "main" && ty != "tvf" {
                    if let Ok(prosa_metadata) = serde_json::from_value::<Metadata>(data.clone()) {
                        if ty == "proc" {
                            if let Some(proc_name) = prosa_metadata.match_proc(
                                name.replace('-', "_").as_str(),
                                Some(self.name.replace('-', "_").as_str()),
                            ) {
                                return Some(ComponentVersion {
                                    name: proc_name.clone(),
                                    crate_name: &self.name,
                                    version: &self.version,
                                });
                            }
                        } else if ty == "adaptor" {
                            if let Some(adaptor_name) = prosa_metadata.find_adaptor(
                                name.replace('-', "_").as_str(),
                                Some(self.name.replace('-', "_").as_str()),
                            ) {
                                return Some(ComponentVersion {
                                    name: adaptor_name.clone(),
                                    crate_name: &self.name,
                                    version: &self.version,
                                });
                            }
                        }
                    }
                } else if meta_name == ty {
                    if let Ok(prosa_metadata) = serde_json::from_value::<Vec<String>>(data.clone())
                    {
                        if let Some(component_name) = prosa_metadata.into_iter().find(|w| {
                            format!("{}::{}", self.name.replace('-', "_"), w).contains(name)
                        }) {
                            return Some(ComponentVersion {
                                name: component_name,
                                crate_name: &self.name,
                                version: &self.version,
                            });
                        }
                    }
                }
            }
        }

        None
    }

    /// Method to get the processor version from its name if it exist
    pub fn get_main_version(&self, name: &str) -> Option<ComponentVersion> {
        self.get_version(name, "main")
    }

    /// Method to get the processor version from its name if it exist
    pub fn get_tvf_version(&self, name: &str) -> Option<ComponentVersion> {
        self.get_version(name, "tvf")
    }

    /// Method to get the processor version from its name if it exist
    pub fn get_proc_version(&self, name: &str) -> Option<ComponentVersion> {
        self.get_version(name, "proc")
    }

    /// Method to get the adaptor version from its name if it exist
    pub fn get_adaptor_version(&self, name: &str) -> Option<ComponentVersion> {
        self.get_version(name, "adaptor")
    }

    /// Method to add package metadata to a Jinja context
    pub fn j2_context(&self, ctx: &mut tera::Context) {
        ctx.insert("name", &self.name);
        ctx.insert("version", &self.version);
        if let Some(license) = &self.license {
            ctx.insert("license", license);
        }
        if let Some(desc) = &self.description {
            ctx.insert("description", desc);
        }
        if let Some(doc) = &self.documentation {
            ctx.insert("documentation", doc);
        }
        if !self.authors.is_empty() {
            ctx.insert("authors", &self.authors);
        }

        if let Some(metadata) = &self.metadata {
            ctx.insert("deb_pkg", &metadata.contains_key("deb"));
        }
    }
}

impl fmt::Display for PackageMetadata {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if let Some(description) = &self.description {
            writeln!(
                f,
                "Package {}[{}] ({})",
                self.name, self.version, description
            )?;
        } else {
            writeln!(f, "Package {}[{}]", self.name, self.version)?;
        }

        if let Some(metadata) = self
            .metadata
            .as_ref()
            .and_then(|m| m.get("prosa").and_then(|w| w.as_object()))
        {
            for (name, data) in metadata {
                writeln!(f, "  - {name}")?;
                if name != "main" && name != "tvf" {
                    if let Ok(prosa_metadata) = serde_json::from_value::<Metadata>(data.clone()) {
                        write!(f, "{prosa_metadata}")?;
                    }
                } else if let Ok(prosa_metadata) =
                    serde_json::from_value::<Vec<String>>(data.clone())
                {
                    for prosa_meta in prosa_metadata {
                        writeln!(f, "    - {prosa_meta}")?;
                    }
                }
            }
        }

        Ok(())
    }
}

/// Structure that contain all cargo metadata
#[derive(Debug, Deserialize)]
pub struct CargoMetadata {
    packages: Vec<PackageMetadata>,
}

impl CargoMetadata {
    /// Method to load metadata for the ProSA package
    pub fn load_metadata() -> Result<CargoMetadata, io::Error> {
        // Get packges metadata
        let cargo_metadata = std::process::Command::new("cargo")
            .args(vec!["metadata", "-q"])
            .output()?;

        Ok(serde_json::from_slice(cargo_metadata.stdout.as_slice())?)
    }

    /// Method to load metadata of the current ProSA package without its dependencies
    pub fn load_package_metadata() -> Result<PackageMetadata, io::Error> {
        // Get local packges metadata
        let cargo_metadata = std::process::Command::new("cargo")
            .args(vec!["metadata", "-q", "--no-deps"])
            .output()?;
        if cargo_metadata.status.success() {
            let mut metadata: CargoMetadata =
                serde_json::from_slice(cargo_metadata.stdout.as_slice())?;
            if metadata.packages.len() == 1 {
                Ok(metadata.packages.pop().unwrap())
            } else {
                Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Local package metadata is not correct",
                ))
            }
        } else {
            Err(io::Error::other(
                std::str::from_utf8(cargo_metadata.stderr.as_slice()).unwrap_or(
                    format!(
                        "Can't retrieve package metadata {:?}",
                        cargo_metadata.status.code()
                    )
                    .as_str(),
                ),
            ))
        }
    }

    /// Method to get all merged ProSA proc metadata
    pub fn prosa_proc_metadata(&self) -> HashMap<&str, Metadata> {
        let mut prosa_list: HashMap<&str, Metadata> = HashMap::with_capacity(self.packages.len());
        for package in &self.packages {
            if let Some(prosa_metadata) = package.get_prosa_proc_metadata() {
                for (prosa_proc_name, mut prosa_proc_metadata) in prosa_metadata {
                    prosa_proc_metadata.specify(&package.name, package.description.clone());
                    if let Some(prosa_existing_metadata) = prosa_list.get_mut(prosa_proc_name) {
                        prosa_existing_metadata.merge(prosa_proc_metadata);
                    } else {
                        prosa_list.insert(prosa_proc_name, prosa_proc_metadata);
                    }
                }
            }
        }

        prosa_list
    }

    /// Method to get all merged ProSA main proc
    pub fn prosa_main(&self) -> Vec<String> {
        let mut main = Vec::new();
        for package in &self.packages {
            main.append(&mut package.get_prosa_main());
        }

        main
    }

    /// Method to get all merged ProSA TVF format
    pub fn prosa_tvf(&self) -> Vec<String> {
        let mut tvf = Vec::new();
        for package in &self.packages {
            tvf.append(&mut package.get_prosa_tvf());
        }

        tvf
    }

    /// Getter of the main version from its name if it exist
    pub fn get_main_version(&self, main_name: &str) -> Option<ComponentVersion> {
        for package in &self.packages {
            if let Some(main) = package.get_main_version(main_name) {
                return Some(main);
            }
        }

        None
    }

    /// Getter of the TVF version from its name if it exist
    pub fn get_tvf_version(&self, main_name: &str) -> Option<ComponentVersion> {
        for package in &self.packages {
            if let Some(main) = package.get_tvf_version(main_name) {
                return Some(main);
            }
        }

        None
    }

    /// Getter of the (processor, adaptor) version from their name if it exist
    pub fn get_versions(
        &self,
        proc_name: &str,
        adaptor_name: &str,
    ) -> (Option<ComponentVersion>, Option<ComponentVersion>) {
        let mut processor_version = None;
        let mut adaptor_version = None;
        for package in &self.packages {
            if processor_version.is_none() {
                if let Some(proc) = package.get_proc_version(proc_name) {
                    processor_version = Some(proc);

                    if adaptor_version.is_some() {
                        break;
                    }
                }
            }

            if adaptor_version.is_none() {
                if let Some(adaptor) = package.get_adaptor_version(adaptor_name) {
                    adaptor_version = Some(adaptor);

                    if processor_version.is_some() {
                        break;
                    }
                }
            }
        }

        (processor_version, adaptor_version)
    }
}

impl fmt::Display for CargoMetadata {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        for package in &self.packages {
            if package.is_prosa() {
                write!(f, "{package}")?;
            }
        }

        Ok(())
    }
}
