use std::{
    fmt, fs, io,
    path::{Path, PathBuf},
};

use clap::ArgMatches;
use tera::Tera;

use crate::cargo::CargoMetadata;

/// Struct to handle Container file creation
pub struct ContainerFile {
    is_docker: bool,
    ctx: tera::Context,
    path: Option<String>,
}

impl ContainerFile {
    /// Create a container file builder from `cargo-prosa` command arguments
    pub fn new(args: &ArgMatches) -> io::Result<ContainerFile> {
        let package_metadata = CargoMetadata::load_package_metadata()?;
        let is_docker = args.get_flag("docker");
        let mut ctx = tera::Context::new();
        package_metadata.j2_context(&mut ctx);
        ctx.insert("docker", &is_docker);
        ctx.insert(
            "image",
            args.get_one::<String>("image")
                .expect("required container base image"),
        );
        ctx.insert(
            "package_manager",
            args.get_one::<String>("package_manager")
                .expect("required package manager"),
        );
        let builder_img = args.get_one::<String>("builder");
        if let Some(img) = builder_img {
            ctx.insert("builder_image", img);
        }

        Ok(ContainerFile {
            is_docker,
            ctx,
            path: args.get_one::<String>("PATH").cloned(),
        })
    }

    /// Method to get the path of the Dockerfile/Containerfile
    pub fn get_path(&self) -> PathBuf {
        if let Some(p) = &self.path {
            let path = Path::new(p);
            if path.is_dir() {
                if self.is_docker {
                    path.join("Dockerfile")
                } else {
                    path.join("Containerfile")
                }
            } else {
                path.to_path_buf()
            }
        } else if self.is_docker {
            Path::new("Dockerfile").to_path_buf()
        } else {
            Path::new("Containerfile").to_path_buf()
        }
    }

    /// Method to create a container file
    pub fn create_container_file(&self) -> tera::Result<()> {
        let template_name = if self.is_docker {
            "Dockerfile"
        } else {
            "Containerfile"
        };

        let mut tera_build = Tera::default();
        tera_build.add_raw_template(
            template_name,
            include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/container.j2")),
        )?;

        let build_file = fs::File::create(self.get_path()).map_err(tera::Error::io_error)?;
        tera_build.render_to(template_name, &self.ctx, build_file)
    }
}

impl fmt::Display for ContainerFile {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let img_name = format!(
            "{}:{}",
            self.ctx.get("name").unwrap().as_str().unwrap(),
            self.ctx.get("version").unwrap().as_str().unwrap()
        );
        writeln!(f, "To build your container, use the command:")?;
        if self.is_docker {
            write!(f, "  `docker build")?;
            if self.path.is_some() {
                write!(f, " -f {}", self.get_path().display())?;
            }
            writeln!(f, " -t {} .`", img_name)?;

            if self.ctx.contains_key("builder_image") {
                writeln!(
                    f,
                    "If you have an external git dependency, specify your ssh agent with:"
                )?;

                write!(f, "  `docker build")?;
                if self.path.is_some() {
                    write!(f, " -f {}", self.get_path().display())?;
                }
                writeln!(f, " --ssh default=$SSH_AUTH_SOCK -t {} .`", img_name)
            } else {
                Ok(())
            }
        } else if self.path.is_some() {
            writeln!(
                f,
                "  `podman build -f {} -t {} .`",
                self.get_path().display(),
                img_name
            )
        } else {
            writeln!(f, "  `podman build -t {} .`", img_name)
        }
    }
}
