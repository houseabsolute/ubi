mod ubi;

use anyhow::{anyhow, Error, Result};
use clap::{Arg, ArgGroup, ArgMatches, Command};
use log::{debug, error};
use platforms::{Platform, PlatformReq};
use std::{
    env::{self, args_os},
    ffi::OsString,
    str::FromStr,
};
use thiserror::Error;
use ubi::Ubi;

#[derive(Debug, Error)]
enum UbiError {
    #[error("{0:}")]
    InvalidArgsError(&'static str),
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let cmd = cmd();
    let matches = cmd.get_matches();
    let res = init_logger(&matches);
    if let Err(e) = res {
        eprintln!("Error creating logger: {e}");
        std::process::exit(126);
    }
    let status = match make_ubi(matches) {
        Ok(u) => match u.run().await {
            Ok(()) => 0,
            Err(e) => {
                print_err(e);
                1
            }
        },
        Err(e) => {
            print_err(e);
            127
        }
    };
    std::process::exit(status);
}

fn cmd<'a>() -> Command<'a> {
    Command::new("ubi")
        .version(ubi::VERSION)
        .author("Dave Rolsky <autarch@urth.org>")
        .about("The universal binary release installer")
        .arg(
            Arg::new("project")
                .long("project")
                .short('p')
                .takes_value(true)
                .help(concat!(
                    "The project you want to install, like houseabsolute/precious",
                    " or https://github.com/houseabsolute/precious.",
                )),
        )
        .arg(
            Arg::new("tag")
                .long("tag")
                .short('t')
                .takes_value(true)
                .help("The tag to download. Defaults to the latest release."),
        )
        .arg(
            Arg::new("url")
                .long("url")
                .short('u')
                .takes_value(true)
                .help(concat!(
                    "The url of the file to download. This can be provided",
                    " instead of a project or tag. This will not use the GitHub API,",
                    " so you will never hit the GitHub API limits. This means you",
                    " do not need to set a GITHUB_TOKEN env var except for",
                    " private repos.",
                )),
        )
        .arg(
            Arg::new("self-upgrade")
                .long("self-upgrade")
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
                .takes_value(true)
                .help("The directory in which the binary should be placed. Defaults to ./bin."),
        )
        .arg(
            Arg::new("exe")
                .long("exe")
                .short('e')
                .takes_value(true)
                .help(concat!(
                    "The name of this project's executable. By default this is the same as the",
                    " project name, so for houseabsolute/precious we look for precious or",
                    r#" precious.exe. When running on Windows the ".exe" suffix will be added"#,
                    " as needed.",
                )),
        )
        .arg(
            Arg::new("matching")
                .long("matching")
                .short('m')
                .takes_value(true)
                .help(concat!(
                    "A string that will be matched against the release filename when there are",
                    r#" multiple files for your OS/arch, i.e. "gnu" or "musl". Note that this will"#,
                    " be ignored if there is only used when there is only one matching release",
                    " filename for your OS/arch",
                )),
        )
        .arg(
            Arg::new("verbose")
                .short('v')
                .long("verbose")
                .help("Enable verbose output"),
        )
        .arg(
            Arg::new("debug")
                .short('d')
                .long("debug")
                .help("Enable debugging output"),
        )
        .arg(
            Arg::new("quiet")
                .short('q')
                .long("quiet")
                .help("Suppresses most output"),
        )
        .group(ArgGroup::new("log-level").args(&["verbose", "debug", "quiet"]))
}

pub fn init_logger(matches: &ArgMatches) -> Result<(), log::SetLoggerError> {
    let level = if matches.is_present("debug") {
        log::LevelFilter::Debug
    } else if matches.is_present("verbose") {
        log::LevelFilter::Info
    } else if matches.is_present("quiet") {
        log::LevelFilter::Error
    } else {
        log::LevelFilter::Warn
    };

    ubi::init_logger(level)
}

const TARGET: &str = env!("TARGET");

fn make_ubi<'a>(mut matches: ArgMatches) -> Result<Ubi<'a>> {
    validate_args(&matches)?;
    if matches.is_present("self-upgrade") {
        let cmd = cmd();
        matches = cmd.try_get_matches_from(self_upgrade_args()?)?;
    }
    let req = PlatformReq::from_str(TARGET)?;
    let platform = Platform::ALL
        .iter()
        .find(|p| req.matches(p))
        .unwrap_or_else(|| panic!("Could not find any platform matching {TARGET}"));
    Ubi::new(
        matches.value_of("project"),
        matches.value_of("tag"),
        matches.value_of("url"),
        matches.value_of("in"),
        matches.value_of("matching"),
        matches.value_of("exe"),
        platform,
        None,
    )
}

fn validate_args(matches: &ArgMatches) -> Result<()> {
    if matches.is_present("url") {
        for a in &["project", "tag"] {
            if matches.is_present(a) {
                return Err(UbiError::InvalidArgsError(
                    "You cannot combine the --url and --{a} options",
                )
                .into());
            }
        }
    }

    if matches.is_present("self-upgrade") {
        for a in &["exe", "in", "project", "tag"] {
            if matches.is_present(a) {
                return Err(UbiError::InvalidArgsError(
                    "You cannot combine the --self-upgrade and --{a} options",
                )
                .into());
            }
        }
    }

    if !(matches.is_present("project")
        || matches.is_present("url")
        || matches.is_present("self-upgrade"))
    {
        return Err(UbiError::InvalidArgsError("You must pass a --project or --url.").into());
    }

    Ok(())
}

fn self_upgrade_args() -> Result<Vec<OsString>> {
    let mut munged: Vec<OsString> = vec![];
    let mut iter = args_os();
    while let Some(a) = iter.next() {
        if a == "--exe" || a == "--project" || a == "--tag" || a == "--url" {
            iter.next();
            continue;
        }
        munged.push(a);
    }
    munged.append(
        &mut vec!["--project", "houseabsolute/ubi", "--in"]
            .into_iter()
            .map(|a| a.into())
            .collect(),
    );
    let current = env::current_exe()?;
    munged.push(
        current
            .parent()
            .ok_or_else(|| anyhow!("executable path `{}` has no parent", current.display()))?
            .as_os_str()
            .to_owned(),
    );
    debug!("munged args for self-upgrade = [{munged:?}]");
    Ok(munged)
}

fn print_err(e: Error) {
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
