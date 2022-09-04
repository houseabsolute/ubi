use anyhow::{anyhow, Context, Result};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;
use tempfile::{tempdir, TempDir};

#[cfg(target_family = "unix")]
use std::os::unix::fs::PermissionsExt;
#[cfg(target_family = "unix")]
use std::os::unix::prelude::*;

#[test]
fn tests() -> Result<()> {
    let cargo = make_pathbuf(&["cargo"]);
    run_command(cargo.as_ref(), &["build"])?;

    let mut ubi = env::current_dir()?;
    ubi.push("target");
    ubi.push("debug");
    ubi.push("ubi");

    run_test(
        ubi.as_ref(),
        &["--project", "houseabsolute/precious"],
        make_pathbuf(&["bin", "precious"]),
    )?;

    {
        let precious_bin = make_pathbuf(&["bin", "precious"]);
        let _td = run_test(
            ubi.as_ref(),
            &["--project", "houseabsolute/precious", "--tag", "v0.0.6"],
            precious_bin.clone(),
        )?;
        match run_command(precious_bin.as_ref(), &["--version"]) {
            Ok((code, stdout, _)) => {
                assert!(code == 0, "exit code is 0");
                assert!(stdout.is_some(), "got stdout from precious");
                assert!(
                    stdout.unwrap().contains("precious 0.0.6"),
                    "downloaded version 0.0.6"
                );
            }
            Err(e) => return Err(e),
        }
    }

    run_test(
        ubi.as_ref(),
        &["--project", "https://github.com/houseabsolute/precious"],
        make_pathbuf(&["bin", "precious"]),
    )?;

    run_test(
        ubi.as_ref(),
        &[
            "--project",
            "https://github.com/houseabsolute/precious/releases",
        ],
        make_pathbuf(&["bin", "precious"]),
    )?;

    let in_dir = make_pathbuf(&["sub", "dir"]);
    run_test(
        ubi.as_ref(),
        &[
            "--project",
            "houseabsolute/precious",
            "--in",
            &in_dir.to_string_lossy(),
        ],
        make_pathbuf(&["sub", "dir", "precious"]),
    )?;

    #[cfg(target_os = "linux")]
    {
        let precious_bin = make_pathbuf(&["bin", "precious"]);
        let _td =     run_test(
            ubi.as_ref(),
            &["--url", "https://github.com/houseabsolute/precious/releases/download/v0.1.7/precious-Linux-x86_64-musl.tar.gz"],
            make_pathbuf(&["bin", "precious"]),
        )?;
        match run_command(precious_bin.as_ref(), &["--version"]) {
            Ok((code, stdout, _)) => {
                assert!(code == 0, "exit code is 0");
                assert!(stdout.is_some(), "got stdout from precious");
                assert!(
                    stdout.unwrap().contains("precious 0.1.7"),
                    "downloaded version 0.1.7"
                );
            }
            Err(e) => return Err(e),
        }
    }

    run_test(
        ubi.as_ref(),
        &["--project", "BurntSushi/ripgrep", "--exe", "rg"],
        make_pathbuf(&["bin", "rg"]),
    )?;

    {
        let rust_analyzer_bin = make_pathbuf(&["bin", "rust-analyzer"]);
        let _td = run_test(
            ubi.as_ref(),
            &["--project", "rust-analyzer/rust-analyzer"],
            rust_analyzer_bin.clone(),
        )?;
        match run_command(rust_analyzer_bin.as_ref(), &["--help"]) {
            Ok((code, stdout, _)) => {
                assert!(code == 0, "exit code is 0");
                assert!(stdout.is_some(), "got stdout from rust-analyzer");
                assert!(
                    stdout
                        .unwrap()
                        .contains("LSP server for the Rust programming language"),
                    "got expected --help output"
                );
            }
            Err(e) => return Err(e),
        }
    }

    {
        let golangci_lint_bin = make_pathbuf(&["bin", "golangci-lint"]);
        let _td = run_test(
            ubi.as_ref(),
            &["--project", "golangci/golangci-lint"],
            golangci_lint_bin.clone(),
        )?;
        match run_command(golangci_lint_bin.as_ref(), &["--version"]) {
            Ok((code, stdout, _)) => {
                assert!(code == 0, "exit code is 0");
                assert!(stdout.is_some(), "got stdout from golangci-lint");
                assert!(
                    stdout.unwrap().contains("golangci-lint has version"),
                    "got the expected stdout",
                );
            }
            Err(e) => return Err(e),
        }
    }

    // This project only has a Linux release. It has a single `.xz` file in
    // its releases which uncompresses to an ELF binary.
    #[cfg(target_os = "linux")]
    {
        let prettycrontab_bin = make_pathbuf(&["bin", "prettycrontab"]);
        let _td = run_test(
            ubi.as_ref(),
            &["--project", "mfontani/prettycrontab", "--tag", "v0.0.2"],
            prettycrontab_bin.clone(),
        )?;
        match run_command(prettycrontab_bin.as_ref(), &["-version"]) {
            Ok((code, stdout, _)) => {
                assert!(code == 0, "exit code is 0");
                assert!(stdout.is_some(), "got stdout from prettycrontab");
                assert!(
                    stdout.unwrap().contains("v0.0.2"),
                    "got the expected stdout",
                );
            }
            Err(e) => return Err(e),
        }
    }

    #[cfg(target_os = "linux")]
    {
        let delta_bin = make_pathbuf(&["bin", "delta"]);
        let _td = run_test(
            ubi.as_ref(),
            &[
                "--project",
                "dandavison/delta",
                "--tag",
                "0.13.0",
                "--matching",
                "musl",
            ],
            delta_bin.clone(),
        )?;
        match run_command(delta_bin.as_ref(), &["--version"]) {
            Ok((code, stdout, _)) => {
                assert!(code == 0, "exit code is 0");
                assert!(stdout.is_some(), "got stdout from delta");
                assert!(
                    stdout.unwrap().contains("delta 0.13.0"),
                    "got the expected stdout",
                );
            }
            Err(e) => return Err(e),
        }
        match run_command(
            &PathBuf::from("file"),
            &[delta_bin.to_string_lossy().as_ref()],
        ) {
            Ok((code, stdout, _)) => {
                assert!(code == 0, "exit code is 0");
                assert!(stdout.is_some(), "got stdout from file");
                assert!(
                    stdout.unwrap().contains("statically linked"),
                    "got the expected stdout",
                );
            }
            Err(e) => return Err(e),
        }
    }

    Ok(())
}

fn make_pathbuf(path: &[&str]) -> PathBuf {
    let mut iter = path.iter();
    let mut pb = PathBuf::from(iter.next().unwrap());
    for v in iter {
        pb.push(v);
    }
    pb
}

fn run_test(cmd: &Path, args: &[&str], mut expect: PathBuf) -> Result<TempDir> {
    let td = tempdir()?;
    env::set_current_dir(td.path())?;

    match run_command(cmd, args) {
        Ok((code, stdout, stderr)) => {
            assert!(code == 0, "exit code is 0");
            assert_eq!(
                stdout.unwrap_or_default(),
                String::new(),
                "no output to stdout",
            );
            assert_eq!(
                stderr.unwrap_or_default(),
                String::new(),
                "no output to stderr",
            );
        }
        Err(e) => return Err(e),
    }

    if cfg!(windows) && !expect.to_string_lossy().ends_with(".exe") {
        expect.set_extension("exe");
    }

    let expect_str = expect.to_string_lossy().into_owned();

    let meta = fs::metadata(expect).context(format!("getting fs metadata for {}", expect_str))?;
    assert!(meta.is_file(), "downloaded file into expected location",);
    #[cfg(target_family = "unix")]
    assert!(
        meta.permissions().mode() & 0o111 != 0,
        "downloaded file is executable",
    );

    Ok(td)
}

pub fn run_command(cmd: &Path, args: &[&str]) -> Result<(i32, Option<String>, Option<String>)> {
    let mut c = process::Command::new(cmd);
    for a in args.iter() {
        c.arg(a);
    }

    output_from_command(c, cmd, args)
}

fn output_from_command(
    mut c: process::Command,
    cmd: &Path,
    args: &[&str],
) -> Result<(i32, Option<String>, Option<String>)> {
    let cstr = command_string(cmd, args);
    println!("running {}", cstr);

    let output = c.output()?;
    match output.status.code() {
        Some(code) => match code {
            0 => Ok((
                code,
                to_option_string(output.stdout),
                to_option_string(output.stderr),
            )),
            _ => {
                let mut msg = format!("ran {} and got non-zero exit code: {}", cstr, code);
                if !output.stderr.is_empty() {
                    msg.push('\n');
                    msg.push_str(&String::from_utf8_lossy(&output.stderr));
                }
                Err(anyhow!(msg))
            }
        },
        None => {
            let cstr = command_string(cmd, args);
            match output.status.success() {
                true => Err(anyhow!("ran {} successfully but it had no exit code", cstr)),
                false => {
                    let signal = signal_from_status(output.status);
                    Err(anyhow!(
                        "ran {} successfully but was killed by signal {}",
                        cstr,
                        signal,
                    ))
                }
            }
        }
    }
}

fn command_string(cmd: &Path, args: &[&str]) -> String {
    let mut cstr = cmd.to_string_lossy().into_owned();
    if !args.is_empty() {
        cstr.push(' ');
        cstr.push_str(args.join(" ").as_str());
    }
    cstr
}

fn to_option_string(v: Vec<u8>) -> Option<String> {
    if v.is_empty() {
        None
    } else {
        Some(String::from_utf8_lossy(&v).into_owned())
    }
}

#[cfg(target_family = "unix")]
fn signal_from_status(status: process::ExitStatus) -> i32 {
    status.signal().unwrap_or(0)
}

#[cfg(target_family = "windows")]
fn signal_from_status(_: process::ExitStatus) -> i32 {
    0
}
