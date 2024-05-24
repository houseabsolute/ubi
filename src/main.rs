mod arch;
mod ubi;

use anyhow::{anyhow, Error, Result};
use clap::{Arg, ArgAction, ArgGroup, ArgMatches, Command};
use log::{debug, error};
use platforms::{Platform, PlatformReq};
use std::{
    env::{self, args_os},
    ffi::OsString,
    path::PathBuf,
    str::FromStr,
};
use thiserror::Error;
use ubi::Ubi;

#[derive(Debug, Error)]
enum UbiError {
    #[error("{0:}")]
    InvalidArgsError(String),
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
        Ok((u, post_run)) => match u.run().await {
            Ok(()) => {
                if let Some(post_run) = post_run {
                    post_run();
                }
                0
            }
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

const MAX_TERM_WIDTH: usize = 100;

fn cmd() -> Command {
    Command::new("ubi")
        .version(ubi::VERSION)
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
            "The url of the file to download. This can be provided",
            " instead of a project or tag. This will not use the GitHub API,",
            " so you will never hit the GitHub API limits. This means you",
            " do not need to set a GITHUB_TOKEN env var except for",
            " private repos.",
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
            "The name of this project's executable. By default this is the same as the",
            " project name, so for houseabsolute/precious we look for precious or",
            r#" precious.exe. When running on Windows the ".exe" suffix will be added"#,
            " as needed.",
        )))
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

pub fn init_logger(matches: &ArgMatches) -> Result<(), log::SetLoggerError> {
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

const TARGET: &str = env!("TARGET");

fn make_ubi<'a>(mut matches: ArgMatches) -> Result<(Ubi<'a>, Option<impl FnOnce()>)> {
    validate_args(&matches)?;
    let mut post_run = None;
    if matches.get_flag("self-upgrade") {
        let cmd = cmd();
        let (args, to_delete) = self_upgrade_args()?;
        matches = cmd.try_get_matches_from(args)?;
        if let Some(to_delete) = to_delete {
            post_run = Some(move || {
                println!(
                    "The self-upgrade operation left an old binary behind that must be deleted manually: {}",
                    to_delete.display(),
                );
            });
        }
    }
    let req = PlatformReq::from_str(TARGET)?;
    let platform = Platform::ALL
        .iter()
        .find(|p| req.matches(p))
        .unwrap_or_else(|| panic!("Could not find any platform matching {TARGET}"));

    Ok((
        Ubi::new(
            matches.get_one::<String>("project").map(|s| s.as_str()),
            matches.get_one::<String>("tag").map(|s| s.as_str()),
            matches.get_one::<String>("url").map(|s| s.as_str()),
            matches.get_one::<String>("in").map(|s| s.as_str()),
            matches.get_one::<String>("matching").map(|s| s.as_str()),
            matches.get_one::<String>("exe").map(|s| s.as_str()),
            platform,
            None,
        )?,
        post_run,
    ))
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

fn self_upgrade_args() -> Result<(Vec<OsString>, Option<PathBuf>)> {
    let mut munged: Vec<OsString> = vec![];
    let mut iter = args_os();
    while let Some(a) = iter.next() {
        if a == "--exe" || a == "--project" || a == "--tag" || a == "--url" || a == "--self-upgrade"
        {
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
            .to_os_string(),
    );

    #[allow(unused_assignments, unused_mut)]
    let mut to_delete = None;
    #[cfg(target_os = "windows")]
    {
        let mut new_exe = current.clone();
        new_exe.set_file_name("ubi-old.exe");
        debug!("renaming {} to {}", current.display(), new_exe.display());
        std::fs::rename(&current, &new_exe)?;
        to_delete = Some(new_exe);
    }

    debug!("munged args for self-upgrade = [{munged:?}]");
    Ok((munged, to_delete))
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
