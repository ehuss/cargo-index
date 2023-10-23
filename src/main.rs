use anyhow::{bail, Error};
use clap::{crate_version, Arg, ArgAction, ArgMatches, Command};
use std::path::Path;
use std::process::exit;

fn main() {
    if let Err(e) = run() {
        eprintln!("Error: {}", e);
        for cause in e.chain().skip(1) {
            eprintln!("Caused by: {}", cause);
        }
        exit(1);
    }
    exit(0);
}

const ADD_HELP: &str = "\
This command will add a crate to an index.

If the `--crate` flag is passed, it will add the given crate. If
`--manifest-path` flag is passed, it will add the package with the given
`Cargo.toml` file. If neither flag is given, it will look in the current
directory for a `Cargo.toml` manifest.

All arguments at the end of the command line following `--` will be passed
as-is to `cargo package` when generating the `.crate` file.
";

const METADATA_HELP: &str = "\
This command will display the JSON metadata for a `.crate` file on stdout.

If the `--crate` flag is passed, it will display the metadata for the given
crate. If `--manifest-path` flag is passed, it will display the metadata the
package with the given `Cargo.toml` file. If neither flag is given, it will
look in the current directory for a `Cargo.toml` manifest.

All arguments at the end of the command line following `--` will be passed
as-is to `cargo package` when generating the `.crate` file.
";

trait AppExt: Sized {
    fn _arg(self, arg: Arg) -> Self;

    fn arg_manifest(self) -> Self {
        self._arg(
            Arg::new("manifest-path")
                .long("manifest-path")
                .value_name("PATH")
                .help("Path to Cargo.toml file."),
        )
    }

    fn arg_crate(self) -> Self {
        self._arg(
            Arg::new("crate")
                .long("crate")
                .value_name("PATH")
                .conflicts_with("package-args")
                .help("Path to .crate file."),
        )
    }

    fn arg_index(self) -> Self {
        self._arg(
            Arg::new("index")
                .long("index")
                .value_name("INDEX")
                .required(true)
                .help("Path to index."),
        )
    }

    fn arg_index_url(self) -> Self {
        self._arg(
            Arg::new("index-url")
                .long("index-url")
                .value_name("INDEX-URL")
                .required(true)
                .help("Public URL of the index."),
        )
    }

    fn arg_package(self, help: &'static str, required: bool) -> Self {
        self._arg(
            Arg::new("package")
                .long("package")
                .short('p')
                .value_name("NAME")
                .required(required)
                .help(help),
        )
    }

    fn arg_version(self, help: &'static str, required: bool) -> Self {
        self._arg(
            Arg::new("version")
                .long("version")
                .alias("vers")
                .value_name("VERSION")
                .required(required)
                .help(help),
        )
    }

    fn arg_force(self) -> Self {
        self._arg(
            Arg::new("force")
                .long("force")
                .alias("f")
                .action(ArgAction::SetTrue)
                .help(
                    "Update the entry for the current package version, even if it already exists.",
                ),
        )
    }

    fn arg_package_args(self) -> Self {
        self._arg(Arg::new("package-args").action(ArgAction::Append))
    }
}

impl AppExt for Command {
    fn _arg(self, arg: Arg) -> Self {
        self.arg(arg)
    }
}

fn run() -> Result<(), Error> {
    let matches = Command::new("cargo-index")
        .version(crate_version!())
        .bin_name("cargo")
        .subcommand_required(true)
        .arg_required_else_help(true)
        .propagate_version(true)
        .subcommand(
            Command::new("index")
                .about("Manage a registry index.")
                .subcommand_required(true)
                .arg_required_else_help(true)
                .subcommand(
                    Command::new("add")
                        .about("Add a package to an index.")
                        .after_help(ADD_HELP)
                        .trailing_var_arg(true)
                        .arg_manifest()
                        .arg_crate()
                        .arg_index()
                        .arg_index_url()
                        .arg_force()
                        .arg(
                            Arg::new("upload")
                            .long("upload")
                            .value_name("DIR")
                            .help("If set, will copy the crate into the given directory. \
                                Use {crate} and {version} to be included in the directory path.")
                            )
                        .arg_package_args()
                )
                .subcommand(
                    Command::new("init")
                        .about("Create a new index.")
                        .arg_index()
                        .arg(
                            Arg::new("dl")
                            .long("dl")
                            .value_name("DL")
                            .required(true)
                            .help("URL of download host such as \
                                https://example.com/api/v1/crates/{crate}/{version}/download \
                                If the {crate}/{version} markers are not present, Cargo \
                                automatically adds `/{crate}/{version}/download` to the end."))
                        .arg(
                            Arg::new("api")
                            .long("api")
                            .value_name("API")
                            .help("URL of API host such as https://example.com"))
                )
                .subcommand(
                    Command::new("metadata")
                        .about("Generate JSON metadata for a package.")
                        .after_help(METADATA_HELP)
                        .trailing_var_arg(true)
                        .arg_manifest()
                        .arg_crate()
                        .arg_index_url()
                        .arg_package_args()
                )
                .subcommand(
                    Command::new("yank")
                        .about("Yank a crate from an index.")
                        .arg_index()
                        .arg_package("Name of the package to yank.", true)
                        .arg_version("Version to yank.", true)
                        .disable_version_flag(true)
                )
                .subcommand(
                    Command::new("unyank")
                        .about("Un-yank a crate from an index.")
                        .arg_index()
                        .arg_package("Name of the package to unyank.", true)
                        .arg_version("Version to unyank.", true)
                        .disable_version_flag(true)
                )
                .subcommand(
                    Command::new("list")
                        .about("List entries in the index.")
                        .arg_index()
                        .arg_package("Name of the package to search for.", false)
                        .arg_version("Version requirement to search for.", false)
                        .disable_version_flag(true)
                )
                .subcommand(
                    Command::new("validate")
                        .about("Validate the format of an index.")
                        .arg_index()
                        .arg(
                            Arg::new("crates")
                                .long("crates")
                                .value_name("DIR")
                                .help("Optional path to the location of all .crate files. \
                                    If set, will validate the files exist and that the checksums are correct. \
                                    Use {crate} and {version} to be included in the directory path.")
                        )
                )
        )
        .get_matches();
    let submatches = matches
        .subcommand_matches("index")
        .expect("Expected `index` subcommand.");

    match submatches.subcommand() {
        Some(("init", args)) => init(args),
        Some(("add", args)) => add(args),
        Some(("metadata", args)) => metadata(args),
        Some(("yank", args)) => yank(args),
        Some(("unyank", args)) => unyank(args),
        Some(("list", args)) => list(args),
        Some(("validate", args)) => validate(args),
        _ => {
            // Enforced by SubcommandRequiredElseHelp.
            unreachable!()
        }
    }?;

    Ok(())
}

fn package_args(args: &ArgMatches) -> Option<Vec<String>> {
    args.get_many::<String>("package-args")
        .map(|values| values.cloned().collect())
}

fn init(args: &ArgMatches) -> Result<(), Error> {
    let path = args.get_one::<String>("index").unwrap();
    reg_index::init(
        path,
        args.get_one::<String>("dl").unwrap(),
        args.get_one::<String>("api").map(String::as_str),
    )?;
    println!("Index created at `{}`.", path);
    Ok(())
}

fn add(args: &ArgMatches) -> Result<(), Error> {
    let index_path = args.get_one::<String>("index").unwrap();
    let index_url = args.get_one::<String>("index-url").unwrap();
    let krate = args.get_one::<String>("crate").map(Path::new);
    let upload = args.get_one::<String>("upload").map(String::as_str);
    let manifest_path = args.get_one::<String>("manifest-path").map(Path::new);
    let force = args.get_flag("force");
    let package_args = package_args(args);
    let reg_pkg = match (manifest_path, krate) {
        (Some(_), None) | (None, None) => {
            if force {
                reg_index::force_add(
                    index_path,
                    index_url,
                    manifest_path,
                    upload,
                    package_args.as_ref(),
                )
            } else {
                reg_index::add(
                    index_path,
                    index_url,
                    manifest_path,
                    upload,
                    package_args.as_ref(),
                )
            }
        }
        (None, Some(krate)) => reg_index::add_from_crate(index_path, index_url, krate, upload),
        (Some(_), Some(_)) => bail!("Both --crate and --manifest-path cannot be specified."),
    }?;
    println!("{}:{} successfully added!", reg_pkg.name, reg_pkg.vers);
    Ok(())
}

fn metadata(args: &ArgMatches) -> Result<(), Error> {
    let index_url = args.get_one::<String>("index-url").unwrap();
    let manifest_path = args.get_one::<String>("manifest-path").map(Path::new);
    let krate = args.get_one::<String>("crate").map(Path::new);
    let package_args = package_args(args);
    let reg_pkg = match (manifest_path, krate) {
        (Some(_), None) | (None, None) => {
            reg_index::metadata(index_url, manifest_path, package_args.as_ref())
        }
        (None, Some(krate)) => reg_index::metadata_from_crate(index_url, krate),
        (Some(_), Some(_)) => bail!("Both --crate and --manifest-path cannot be specified."),
    }?;
    println!("{}", serde_json::to_string(&reg_pkg)?);
    Ok(())
}

fn yank(args: &ArgMatches) -> Result<(), Error> {
    let pkg = args.get_one::<String>("package").unwrap();
    let version = args.get_one::<String>("version").unwrap();
    reg_index::yank(args.get_one::<String>("index").unwrap(), pkg, version)?;
    println!("{}:{} yanked!", pkg, version);
    Ok(())
}

fn unyank(args: &ArgMatches) -> Result<(), Error> {
    let pkg = args.get_one::<String>("package").unwrap();
    let version = args.get_one::<String>("version").unwrap();
    reg_index::unyank(args.get_one::<String>("index").unwrap(), pkg, version)?;
    println!("{}:{} unyanked!", pkg, version);
    Ok(())
}

fn list(args: &ArgMatches) -> Result<(), Error> {
    let pkg = args.get_one::<String>("package").map(String::as_str);
    let version = args.get_one::<String>("version").map(String::as_str);
    let mut count = 0;
    reg_index::list_all(
        args.get_one::<String>("index").unwrap(),
        pkg,
        version,
        |entries| {
            for entry in entries {
                count += 1;
                println!("{}", serde_json::to_string(&entry).unwrap());
            }
        },
    )?;
    if count == 0 {
        match (pkg, version) {
            (Some(pkg), Some(version)) => bail!(
                "No entries found for `{}` that match version `{}`.",
                pkg,
                version
            ),
            (Some(pkg), None) => bail!("Package `{}` is not in the index.", pkg),
            (None, Some(version)) => bail!(
                "No packages matching version requirement `{}` found.",
                version
            ),
            (None, None) => bail!("The index is empty!"),
        }
    }
    Ok(())
}

fn validate(args: &ArgMatches) -> Result<(), Error> {
    reg_index::validate(
        args.get_one::<String>("index").unwrap(),
        args.get_one::<String>("crates").map(String::as_str),
    )
}
