use anyhow::{anyhow, Context, Result};
use clap::ArgMatches;
use clap_complete_command::Shell;
use cmd::cmd;
use log::{debug, error};
use std::{
    env,
    hint::unreachable_unchecked,
    io::{self, Write},
    path::Path,
    str::FromStr,
};
use ubi::{ColorChoice, ForgeType, Ubi, UbiBuilder};

mod cmd;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let cmd = cmd();
    let matches = cmd.get_matches();
    let res = init_logger_from_matches(&matches);
    if let Err(e) = res {
        eprintln!("Error creating logger: {e:?}");
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
            error!("{e:?}");
            std::process::exit(127);
        }
    };
    if let Some(shell) = matches.get_one::<Shell>("completion") {
        generate_completions(shell);
        std::process::exit(0);
    }
    let status = match make_ubi(&matches, &ubi_exe_path) {
        Ok((mut u, post_run)) => match u.install_binary().await {
            Ok(()) => {
                if let Some(post_run) = post_run {
                    post_run();
                }
                0
            }
            Err(e) => {
                error!("{e:?}");
                1
            }
        },
        Err(e) => {
            error!("{e:?}");
            127
        }
    };
    std::process::exit(status);
}

fn make_ubi<'a>(
    matches: &'a ArgMatches,
    ubi_exe_path: &'a Path,
) -> Result<(Ubi<'a>, Option<impl FnOnce()>)> {
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
    if let Some(r) = matches.get_one::<String>("matching-regex") {
        builder = builder.matching_regex(r);
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
        builder = builder.forge(
            ForgeType::from_str(ft).with_context(|| format!("failed to parse forge type: {ft}"))?,
        );
    }
    if let Some(url) = matches.get_one::<String>("api-base-url") {
        builder = builder.api_base_url(url);
    }
    if let Some(days) = matches.get_one::<u32>("min-age-days") {
        builder = builder.min_age_days(*days);
    }

    Ok((builder.build()?, None))
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
        std::fs::rename(ubi_exe_path, &old_exe).with_context(|| {
            format!(
                "failed to rename {} to {}",
                ubi_exe_path.display(),
                old_exe.display()
            )
        })?;
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

fn generate_completions(shell: &Shell) {
    let buf: &[u8] = match shell {
        Shell::Bash => include_bytes!(concat!(env!("OUT_DIR"), "/ubi.bash")),
        Shell::Carapace => include_bytes!(concat!(env!("OUT_DIR"), "/ubi.yaml")),
        Shell::Elvish => include_bytes!(concat!(env!("OUT_DIR"), "/ubi.elv")),
        Shell::Fish => include_bytes!(concat!(env!("OUT_DIR"), "/ubi.fish")),
        Shell::Nu => include_bytes!(concat!(env!("OUT_DIR"), "/ubi.nu")),
        Shell::PowerShell => include_bytes!(concat!(env!("OUT_DIR"), "/_ubi.ps1")),
        Shell::Zsh => include_bytes!(concat!(env!("OUT_DIR"), "/_ubi")),
        _ => unsafe { unreachable_unchecked() },
    };
    _ = io::stdout().write_all(buf);
}

pub(crate) fn init_logger_from_matches(matches: &ArgMatches) -> Result<()> {
    let level = if matches.get_flag("debug") {
        log::LevelFilter::Debug
    } else if matches.get_flag("verbose") {
        log::LevelFilter::Info
    } else if matches.get_flag("quiet") {
        log::LevelFilter::Error
    } else {
        log::LevelFilter::Warn
    };

    let color = match matches.get_one::<String>("color") {
        Some(c) => ColorChoice::from_str(c)
            .with_context(|| format!("failed to parse color choice: {c}"))?,
        None => ColorChoice::default(),
    };

    ubi::init_logger(level, color).context("failed to initialize the logger")
}
