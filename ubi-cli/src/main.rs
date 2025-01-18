use anyhow::{anyhow, Error, Result};
use clap::{Arg, ArgAction, ArgGroup, ArgMatches, Command};
use log::{debug, error};
use std::{env, path::Path, str::FromStr};
use strum::VariantNames;
use thiserror::Error;
use ubi::{ForgeType, Ubi, UbiBuilder};

#[derive(Debug, Error)]
enum UbiError {
    #[error("{0:}")]
    InvalidArgsError(String),
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let cmd = cmd();
    let matches = cmd.get_matches();
    let res = init_logger_from_matches(&matches);
    if let Err(e) = res {
        eprintln!("Error creating logger: {e}");
        std::process::exit(126);
    }

    // We use this when `--self-upgrade` is passed. We need to create this String here so that we
    // can make a Ubi<'_> instance that borrows this value. It needs to have the same lifetime as
    // `matches`. If we try to make it in `self_upgrade_ubi` we end up trying to return a reference
    // data owned by that fn.
    let ubi_exe_path = match env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            let e = anyhow!("could not find path for current executable: {e}");
            print_err(&e);
            std::process::exit(127);
        }
    };
    let status = match make_ubi(&matches, &ubi_exe_path) {
        Ok((mut u, post_run)) => match u.install_binary().await {
            Ok(()) => {
                if let Some(post_run) = post_run {
                    post_run();
                }
                0
            }
            Err(e) => {
                print_err(&e);
                1
            }
        },
        Err(e) => {
            print_err(&e);
            127
        }
    };
    std::process::exit(status);
}

const MAX_TERM_WIDTH: usize = 100;

#[allow(clippy::too_many_lines)]
fn cmd() -> Command {
    Command::new("ubi")
        .version(env!("CARGO_PKG_VERSION"))
        .author("Dave Rolsky <autarch@urth.org>")
        .about("The universal binary release installer")
        .arg(Arg::new("project").long("project").short('p').help(concat!(
            "The project you want to install, like houseabsolute/precious",
            " or https://github.com/houseabsolute/precious.",
        )))
        .arg(
            Arg::new("tag")
                .long("tag")
                .short('t')
                .help("The tag to download. Defaults to the latest release."),
        )
        .arg(Arg::new("url").long("url").short('u').help(concat!(
            "The url of the file to download. This can be provided instead of a project or",
            " tag. This will not use the forge site's API, so you will never hit its API",
            " limits. With this parameter, you do not need to set a token env var except for",
            " private repos."
        )))
        .arg(
            Arg::new("self-upgrade")
                .long("self-upgrade")
                .action(ArgAction::SetTrue)
                .help(concat!(
                    "Use ubi to upgrade to the latest version of ubi. The",
                    " --exe, --in, --project, --tag, and --url args will be",
                    " ignored."
                )),
        )
        .arg(
            Arg::new("in")
                .long("in")
                .short('i')
                .help("The directory in which the binary should be placed. Defaults to ./bin."),
        )
        .arg(Arg::new("exe").long("exe").short('e').help(concat!(
            "The name of the file to look for in an archive file, or the name of the downloadable",
            " file excluding its extension, e.g. `ubi.gz`. By default this is the same as the",
            " project name, so for houseabsolute/precious we look for precious or",
            " precious.exe. When running on Windows the `.exe` suffix will be added, as needed. You",
            " cannot pass `--extract-all` when this is set.",
        )))
        .arg(Arg::new("rename-exe-to").long("rename-exe").help(concat!(
            "The name to use for the executable after it is unpacked. By default this is the same",
            " as the name of the file passed for the `--exe` flag. If that flag isn't passed, this",
            " is the same as the name of the project. Note that when set, this name is used as-is,",
            " so on Windows, `.exe` will not be appended to the name given. You cannot pass",
            " `--extract-all` when this is set.",
        )))
        .arg(
            Arg::new("extract-all")
                .long("extract-all")
                .action(ArgAction::SetTrue)
                .help(concat!(
                    "Pass this to tell `ubi` to extract all files from the archive. By default",
                    " `ubi` will only extract an executable from an archive file. But if this is",
                    " true, it will simply unpack the archive file. If all of the contents of the",
                    " archive file share a top-level directory, that directory will be removed",
                    " during unpacking. In other words, if an archive contains",
                    " `./project/some-file` and `./project/docs.md`, it will extract them as",
                    " `some-file` and `docs.md`. You cannot pass `--exe` or `--rename-exe-to`",
                    " when this is set.",
                )),
        )
        .arg(
            Arg::new("matching")
                .long("matching")
                .short('m')
                .help(concat!(
                    "A string that will be matched against the release filename when there are",
                    " multiple matching files for your OS/arch. For example, there may be",
                    " multiple releases for an OS/arch that differ by compiler (MSVC vs. gcc)",
                    " or linked libc (glibc vs. musl). Note that this will be ignored if there",
                    " is only one matching release filename for your OS/arch.",
                )),
        )
        .arg(
            Arg::new("forge")
                .long("forge")
                .value_parser(clap::builder::PossibleValuesParser::new(
                    ForgeType::VARIANTS,
                ))
                .help(concat!(
                    "The forge to use. If this isn't set, then the value of --project or --url",
                    " will be checked for gitlab.com. If this contains any other domain _or_ if it",
                    " does not have a domain at all, then the default is GitHub.",
                )),
        )
        .arg(Arg::new("api-base-url").long("api-base-url").help(concat!(
            "The the base URL for the forge site's API. This is useful for testing or if you want",
            " to operate against an Enterprise version of GitHub or GitLab. This should be",
            " something like `https://github.my-corp.example.com/api/v4`.",
        )))
        .arg(
            Arg::new("verbose")
                .short('v')
                .long("verbose")
                .action(ArgAction::SetTrue)
                .help("Enable verbose output."),
        )
        .arg(
            Arg::new("debug")
                .short('d')
                .long("debug")
                .action(ArgAction::SetTrue)
                .help("Enable debugging output."),
        )
        .arg(
            Arg::new("quiet")
                .short('q')
                .long("quiet")
                .action(ArgAction::SetTrue)
                .help("Suppresses most output."),
        )
        .group(ArgGroup::new("log-level").args(["verbose", "debug", "quiet"]))
        .max_term_width(MAX_TERM_WIDTH)
}

pub(crate) fn init_logger_from_matches(matches: &ArgMatches) -> Result<(), log::SetLoggerError> {
    let level = if matches.get_flag("debug") {
        log::LevelFilter::Debug
    } else if matches.get_flag("verbose") {
        log::LevelFilter::Info
    } else if matches.get_flag("quiet") {
        log::LevelFilter::Error
    } else {
        log::LevelFilter::Warn
    };

    ubi::init_logger(level)
}

fn make_ubi<'a>(
    matches: &'a ArgMatches,
    ubi_exe_path: &'a Path,
) -> Result<(Ubi<'a>, Option<impl FnOnce()>)> {
    validate_args(matches)?;
    if matches.get_flag("self-upgrade") {
        return self_upgrade_ubi(ubi_exe_path);
    }

    let mut builder = UbiBuilder::new();
    if let Some(p) = matches.get_one::<String>("project") {
        builder = builder.project(p);
    }
    if let Some(t) = matches.get_one::<String>("tag") {
        builder = builder.tag(t);
    }
    if let Some(u) = matches.get_one::<String>("url") {
        builder = builder.url(u);
    }
    if let Some(dir) = matches.get_one::<String>("in") {
        builder = builder.install_dir(dir);
    }
    if let Some(m) = matches.get_one::<String>("matching") {
        builder = builder.matching(m);
    }
    if let Some(e) = matches.get_one::<String>("exe") {
        builder = builder.exe(e);
    }
    if let Some(e) = matches.get_one::<String>("rename-exe-to") {
        builder = builder.rename_exe_to(e);
    }
    if matches.get_flag("extract-all") {
        builder = builder.extract_all();
    }
    if let Some(ft) = matches.get_one::<String>("forge") {
        builder = builder.forge(ForgeType::from_str(ft)?);
    }
    if let Some(url) = matches.get_one::<String>("api-base-url") {
        builder = builder.api_base_url(url);
    }

    Ok((builder.build()?, None))
}

fn validate_args(matches: &ArgMatches) -> Result<()> {
    if matches.contains_id("url") {
        for a in &["project", "tag"] {
            if matches.contains_id(a) {
                return Err(UbiError::InvalidArgsError(format!(
                    "You cannot combine the --url and --{a} options"
                ))
                .into());
            }
        }
    }

    if matches.get_flag("self-upgrade") {
        for a in &["exe", "in", "project", "tag"] {
            if matches.contains_id(a) {
                return Err(UbiError::InvalidArgsError(format!(
                    "You cannot combine the --self-upgrade and --{a} options"
                ))
                .into());
            }
        }
    }

    if !(matches.contains_id("project")
        || matches.contains_id("url")
        || matches.get_flag("self-upgrade"))
    {
        return Err(
            UbiError::InvalidArgsError("You must pass a --project or --url.".to_string()).into(),
        );
    }

    Ok(())
}

fn self_upgrade_ubi(ubi_exe_path: &Path) -> Result<(Ubi<'_>, Option<impl FnOnce()>)> {
    let ubi =
        UbiBuilder::new()
            .project("houseabsolute/ubi")
            .install_dir(ubi_exe_path.parent().ok_or_else(|| {
                anyhow!("executable path `{}` has no parent", ubi_exe_path.display())
            })?)
            .build()?;

    let post_run = if cfg!(target_os = "windows") {
        let mut old_exe = ubi_exe_path.to_path_buf();
        old_exe.set_file_name("ubi-old.exe");
        debug!(
            "renaming {} to {}",
            ubi_exe_path.display(),
            old_exe.display()
        );
        std::fs::rename(ubi_exe_path, &old_exe)?;
        Some(move || {
            println!(
                "The self-upgrade operation left an old binary behind that must be deleted manually: {}",
                old_exe.display(),
            );
        })
    } else {
        None
    };

    Ok((ubi, post_run))
}

fn print_err(e: &Error) {
    error!("{e}");
    if let Some(ue) = e.downcast_ref::<UbiError>() {
        match ue {
            UbiError::InvalidArgsError(_) => {
                println!();
                cmd().print_help().unwrap();
            }
        }
    }
}
