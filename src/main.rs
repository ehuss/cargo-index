use clap::{crate_version, App, AppSettings, Arg, ArgMatches, SubCommand};
use failure::{bail, Error};
use std::path::Path;
use std::process::exit;

fn main() {
    if let Err(e) = run() {
        eprintln!("Error: {}", e);
        for cause in e.iter_chain().skip(1) {
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
    fn _arg(self, arg: Arg<'static, 'static>) -> Self;

    fn arg_manifest(self) -> Self {
        self._arg(
            Arg::with_name("manifest-path")
                .long("manifest-path")
                .value_name("PATH")
                .help("Path to Cargo.toml file."),
        )
    }

    fn arg_crate(self) -> Self {
        self._arg(
            Arg::with_name("crate")
                .long("crate")
                .value_name("PATH")
                .help("Path to .crate file."),
        )
    }

    fn arg_index(self) -> Self {
        self._arg(
            Arg::with_name("index")
                .long("index")
                .value_name("INDEX")
                .required(true)
                .help("Path to index."),
        )
    }

    fn arg_index_url(self) -> Self {
        self._arg(
            Arg::with_name("index-url")
                .long("index-url")
                .value_name("INDEX-URL")
                .required(true)
                .help("Public URL of the index."),
        )
    }

    fn arg_package(self, help: &'static str, required: bool) -> Self {
        self._arg(
            Arg::with_name("package")
                .long("package")
                .short("p")
                .value_name("NAME")
                .required(required)
                .help(help),
        )
    }

    fn arg_version(self, help: &'static str, required: bool) -> Self {
        self._arg(
            Arg::with_name("version")
                .long("version")
                .alias("vers")
                .value_name("VERSION")
                .required(required)
                .help(help),
        )
    }

    fn arg_force(self) -> Self {
        self._arg(
            Arg::with_name("force")
                .long("force")
                .alias("f")
                .takes_value(false)
                .required(false)
                .help("Update the entry for the current package version, even if it already exists."),
        )
    }

    fn arg_package_args(self) -> Self {
        self._arg(Arg::with_name("package-args").multiple(true))
    }
}

impl AppExt for App<'static, 'static> {
    fn _arg(self, arg: Arg<'static, 'static>) -> Self {
        self.arg(arg)
    }
}

fn run() -> Result<(), Error> {
    let matches = App::new("cargo-index")
        .version(crate_version!())
        .bin_name("cargo")
        .setting(AppSettings::SubcommandRequiredElseHelp)
        .global_settings(&[
            AppSettings::GlobalVersion,  // subcommands inherit version
            AppSettings::ColoredHelp,
        ])
        .subcommand(
            SubCommand::with_name("index")
                .about("Manage a registry index.")
                .setting(AppSettings::SubcommandRequiredElseHelp)
                .subcommand(
                    SubCommand::with_name("add")
                        .about("Add a package to an index.")
                        .after_help(ADD_HELP)
                        .setting(AppSettings::TrailingVarArg)
                        .arg_manifest()
                        .arg_crate()
                        .arg_index()
                        .arg_index_url()
                        .arg_force()
                        .arg(
                            Arg::with_name("upload")
                            .long("upload")
                            .value_name("DIR")
                            .help("If set, will copy the crate into the given directory. \
                                Use {crate} and {version} to be included in the directory path.")
                            )
                        .arg_package_args()
                )
                .subcommand(
                    SubCommand::with_name("init")
                        .about("Create a new index.")
                        .arg_index()
                        .arg(
                            Arg::with_name("dl")
                            .long("dl")
                            .value_name("DL")
                            .required(true)
                            .help("URL of download host such as \
                                https://example.com/api/v1/crates/{crate}/{version}/download \
                                If the {crate}/{version} markers are not present, Cargo \
                                automatically adds `/{crate}/{version}/download` to the end."))
                        .arg(
                            Arg::with_name("api")
                            .long("api")
                            .value_name("API")
                            .help("URL of API host such as https://example.com"))
                )
                .subcommand(
                    SubCommand::with_name("metadata")
                        .about("Generate JSON metadata for a package.")
                        .after_help(METADATA_HELP)
                        .setting(AppSettings::TrailingVarArg)
                        .arg_manifest()
                        .arg_crate()
                        .arg_index_url()
                        .arg_package_args()
                )
                .subcommand(
                    SubCommand::with_name("yank")
                        .about("Yank a crate from an index.")
                        .arg_index()
                        .arg_package("Name of the package to yank.", true)
                        .arg_version("Version to yank.", true)
                )
                .subcommand(
                    SubCommand::with_name("unyank")
                        .about("Un-yank a crate from an index.")
                        .arg_index()
                        .arg_package("Name of the package to unyank.", true)
                        .arg_version("Version to unyank.", true)
                )
                .subcommand(
                    SubCommand::with_name("list")
                        .about("List entries in the index.")
                        .arg_index()
                        .arg_package("Name of the package to search for.", false)
                        .arg_version("Version requirement to search for.", false)
                )
                .subcommand(
                    SubCommand::with_name("validate")
                        .about("Validate the format of an index.")
                        .arg_index()
                        .arg(
                            Arg::with_name("crates")
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
        ("init", Some(args)) => init(args),
        ("add", Some(args)) => add(args),
        ("metadata", Some(args)) => metadata(args),
        ("yank", Some(args)) => yank(args),
        ("unyank", Some(args)) => unyank(args),
        ("list", Some(args)) => list(args),
        ("validate", Some(args)) => validate(args),
        _ => {
            // Enforced by SubcommandRequiredElseHelp.
            unreachable!()
        }
    }?;

    Ok(())
}

fn package_args(args: &ArgMatches<'_>) -> Result<Option<Vec<String>>, Error> {
    let package_args: Option<Vec<String>> = args
        .values_of("package-args")
        .map(|values| values.map(|s| s.to_string()).collect());
    if args.is_present("crate") && args.is_present("package-args") {
        bail!("`cargo package` arguments shouldn't be specified with `--crate`.")
    }
    Ok(package_args)
}

fn init(args: &ArgMatches<'_>) -> Result<(), Error> {
    let path = args.value_of("index").unwrap();
    reg_index::init(path, args.value_of("dl").unwrap(), args.value_of("api"))?;
    println!("Index created at `{}`.", path);
    Ok(())
}

fn add(args: &ArgMatches<'_>) -> Result<(), Error> {
    let index_path = args.value_of("index").unwrap();
    let index_url = args.value_of("index-url").unwrap();
    let krate = args.value_of("crate").map(Path::new);
    let upload = args.value_of("upload");
    let manifest_path = args.value_of("manifest-path").map(Path::new);
    let force = args.is_present("force");
    let package_args = package_args(args)?;
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
        },
        (None, Some(krate)) => reg_index::add_from_crate(index_path, index_url, krate, upload),
        (Some(_), Some(_)) => bail!("Both --crate and --manifest-path cannot be specified."),
    }?;
    println!("{}:{} successfully added!", reg_pkg.name, reg_pkg.vers);
    Ok(())
}

fn metadata(args: &ArgMatches<'_>) -> Result<(), Error> {
    let index_url = args.value_of("index-url").unwrap();
    let manifest_path = args.value_of("manifest-path").map(Path::new);
    let krate = args.value_of("crate").map(Path::new);
    let package_args = package_args(args)?;
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

fn yank(args: &ArgMatches<'_>) -> Result<(), Error> {
    let pkg = args.value_of("package").unwrap();
    let version = args.value_of("version").unwrap();
    reg_index::yank(args.value_of("index").unwrap(), pkg, version)?;
    println!("{}:{} yanked!", pkg, version);
    Ok(())
}

fn unyank(args: &ArgMatches<'_>) -> Result<(), Error> {
    let pkg = args.value_of("package").unwrap();
    let version = args.value_of("version").unwrap();
    reg_index::unyank(args.value_of("index").unwrap(), pkg, version)?;
    println!("{}:{} unyanked!", pkg, version);
    Ok(())
}

fn list(args: &ArgMatches<'_>) -> Result<(), Error> {
    let pkg = args.value_of("package");
    let version = args.value_of("version");
    let mut count = 0;
    reg_index::list_all(args.value_of("index").unwrap(), pkg, version, |entries| {
        for entry in entries {
            count += 1;
            println!("{}", serde_json::to_string(&entry).unwrap());
        }
    })?;
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

fn validate(args: &ArgMatches<'_>) -> Result<(), Error> {
    reg_index::validate(args.value_of("index").unwrap(), args.value_of("crates"))
}
