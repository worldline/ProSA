//! Tool to build project
//!
//! Builder contain the strucure of the ProSA.toml file useful to build a ProSA.

use std::{
    fmt, fs,
    io::{self, Write},
    path::Path,
};

use serde::{Deserialize, Serialize};
use toml_edit::{Item, Table, Value};

use crate::cargo::{CargoMetadata, ComponentVersion};

/// Descriptor of ProSA main configuration
///
/// <svg width="40" height="40">
#[doc = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/doc_assets/main.svg"))]
/// </svg>
#[derive(Debug, PartialEq, Deserialize, Serialize)]
pub struct MainDesc {
    /// Name of the main task to use for ProSA (`prosa::core::main::MainProc` by default)
    pub main: String,
    /// Name of the TVF use for ProSA (`prosa_utils::msg::simple_string_tvf::SimpleStringTvf` by default)
    pub tvf: String,
}

impl Default for MainDesc {
    fn default() -> Self {
        MainDesc {
            main: String::from("prosa::core::main::MainProc"),
            tvf: String::from("prosa_utils::msg::simple_string_tvf::SimpleStringTvf"),
        }
    }
}

/// Descriptor of ProSA processor configuration
///
/// <svg width="40" height="40">
#[doc = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/doc_assets/proc.svg"))]
/// </svg>
#[derive(Debug, PartialEq, Deserialize, Serialize)]
pub struct ProcDesc {
    /// Optional description name (processor name by default)
    pub name: Option<String>,
    /// Name of the exposed processor
    pub proc_name: String,
    /// Processor to use
    pub proc: String,
    /// Adaptor to use
    pub adaptor: String,
}

impl ProcDesc {
    /// Create a new processor desc object
    pub fn new(proc_name: String, proc: String, adaptor: String) -> Self {
        ProcDesc {
            name: None,
            proc_name,
            proc,
            adaptor,
        }
    }

    /// Get the name of the processor
    pub fn get_name(&self) -> String {
        if let Some(name) = &self.name {
            name.clone()
        } else {
            self.proc_name.clone()
        }
    }

    /// Getter of the (processor, adaptor) version from the processor description
    pub fn get_versions<'a>(
        &self,
        cargo_metadata: &'a CargoMetadata,
    ) -> (Option<ComponentVersion<'a>>, Option<ComponentVersion<'a>>) {
        cargo_metadata.get_versions(&self.proc, &self.adaptor)
    }
}

impl fmt::Display for ProcDesc {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(
            f,
            "ProSA processor {} ({})",
            self.name.as_ref().unwrap_or(&self.proc),
            self.proc_name,
        )?;
        writeln!(f, "  Processor {}", self.proc)?;
        writeln!(f, "  Adaptor {}", self.adaptor)
    }
}

impl TryFrom<&Item> for ProcDesc {
    type Error = &'static str;

    fn try_from(item: &Item) -> Result<Self, Self::Error> {
        if let Item::ArrayOfTables(array_tables) = item {
            let mut name = None;
            let mut proc_name = None;
            let mut proc = None;
            let mut adaptor = None;
            for array in array_tables {
                if let Some(Item::Value(Value::String(item_name))) = array.get("name") {
                    name = Some(item_name.value().clone());
                } else if let Some(Item::Value(Value::String(item_name))) = array.get("proc_name") {
                    proc_name = Some(item_name.value().clone());
                } else if let Some(Item::Value(Value::String(item_name))) = array.get("proc") {
                    proc = Some(item_name.value().clone());
                } else if let Some(Item::Value(Value::String(item_name))) = array.get("adaptor") {
                    adaptor = Some(item_name.value().clone());
                }
            }

            if let Some(proc_name) = proc_name {
                if let Some(proc) = proc {
                    if let Some(adaptor) = adaptor {
                        Ok(ProcDesc {
                            name,
                            proc_name,
                            proc,
                            adaptor,
                        })
                    } else {
                        Err("No `adaptor` key in toml ProSA description")
                    }
                } else {
                    Err("No `proc` key in toml ProSA description")
                }
            } else {
                Err("No `proc_name` key in toml ProSA description")
            }
        } else {
            Err("The item type is not correct for ProSAProxDesc")
        }
    }
}

impl From<ProcDesc> for Table {
    fn from(proc_desc: ProcDesc) -> Table {
        let mut table = toml_edit::Table::new();
        if let Some(name) = proc_desc.name {
            table.insert(
                "name",
                Item::Value(toml_edit::Value::String(toml_edit::Formatted::new(name))),
            );
        }
        table.insert(
            "proc_name",
            Item::Value(toml_edit::Value::String(toml_edit::Formatted::new(
                proc_desc.proc_name,
            ))),
        );
        table.insert(
            "proc",
            Item::Value(toml_edit::Value::String(toml_edit::Formatted::new(
                proc_desc.proc,
            ))),
        );
        table.insert(
            "adaptor",
            Item::Value(toml_edit::Value::String(toml_edit::Formatted::new(
                proc_desc.adaptor,
            ))),
        );

        table
    }
}

/// Descriptor of ProSA global configuration
#[derive(Debug, Default, PartialEq, Deserialize, Serialize)]
pub struct Desc {
    /// ProSA main task descriptor
    ///
    /// <svg width="40" height="40">
    #[doc = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/doc_assets/main.svg"))]
    /// </svg>
    pub prosa: MainDesc,
    /// ProSA processors descriptors
    ///
    /// <svg width="40" height="40">
    #[doc = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/doc_assets/proc.svg"))]
    /// </svg>
    pub proc: Option<Vec<ProcDesc>>,
}

impl Desc {
    /// Method to create a ProSA toml description file
    pub fn create<P>(&self, path: P) -> Result<(), io::Error>
    where
        P: AsRef<Path>,
    {
        fn inner(desc: &Desc, mut file: fs::File) -> Result<(), io::Error> {
            writeln!(file, "# ProSA definition")?;
            writeln!(
                file,
                "{}",
                toml::to_string(&desc)
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?
            )
        }
        inner(self, fs::File::create(&path)?)
    }

    /// Method to read a ProSA toml description file
    pub fn read<P>(path: P) -> Result<Desc, io::Error>
    where
        P: AsRef<Path>,
    {
        let prosa_desc_file = fs::read_to_string(path)?;
        toml::from_str::<Desc>(prosa_desc_file.as_str())
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    /// Method to add a processor to the list
    #[cfg(test)]
    pub fn add_proc(&mut self, proc_desc: ProcDesc) {
        if let Some(proc) = self.proc.as_mut() {
            proc.push(proc_desc);
        } else {
            self.proc = Some(vec![proc_desc]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prosa_desc_toml() {
        let mut prosa_desc = Desc::default();
        prosa_desc.add_proc(ProcDesc::new(
            "proc".into(),
            "crate::proc".into(),
            "crate::adaptor".into(),
        ));

        let prosa_toml = "[prosa]
main = \"prosa::core::main::MainProc\"
tvf = \"prosa_utils::msg::simple_string_tvf::SimpleStringTvf\"

[[proc]]
proc_name = \"proc\"
proc = \"crate::proc\"
adaptor = \"crate::adaptor\"
";
        assert_eq!(prosa_toml, toml::to_string(&prosa_desc).unwrap());

        // FIXME use environment variable when they will be available for unit tests
        let toml_path_file = Path::new("/tmp/test_prosa_desc.toml");
        let mut toml_file = fs::File::create(toml_path_file).unwrap();
        toml_file.write_all(prosa_toml.as_bytes()).unwrap();

        let prosa_desc_from_file = Desc::read(toml_path_file).unwrap();
        assert_eq!(prosa_desc, prosa_desc_from_file);
    }
}
