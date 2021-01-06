use anyhow::{anyhow, Context, Result};
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process;
use tempfile::{tempdir, TempDir};

#[cfg(target_family = "unix")]
use std::os::unix::fs::PermissionsExt;
#[cfg(target_family = "unix")]
use std::os::unix::prelude::*;

#[test]
fn tests() -> Result<()> {
    let cargo = make_pathbuf(&["cargo"]);
    run_command(&cargo, &["build"])?;

    let mut ubi = env::current_dir()?;
    ubi.push("target");
    ubi.push("debug");
    ubi.push("ubi");

    run_test(
        &ubi,
        &["--project", "houseabsolute/precious"],
        make_pathbuf(&["bin", "precious"]),
    )?;

    {
        let precious_bin = make_pathbuf(&["bin", "precious"]);
        let _td = run_test(
            &ubi,
            &["--project", "houseabsolute/precious", "--tag", "v0.0.6"],
            precious_bin.clone(),
        )?;
        match run_command(&precious_bin, &["--version"]) {
            Ok((code, stdout, _)) => {
                assert!(code == 0, "exit code is 0");
                assert!(stdout.is_some(), "got stdout from precious");
                assert!(
                    stdout.unwrap().contains("precious 0.0.6"),
                    "downloaded version 0.0.6"
                );
            }
            Err(e) => return Err(e),
        };
    }

    run_test(
        &ubi,
        &["--project", "https://github.com/houseabsolute/precious"],
        make_pathbuf(&["bin", "precious"]),
    )?;

    let in_dir = make_pathbuf(&["sub", "dir"]);
    run_test(
        &ubi,
        &[
            "--project",
            "houseabsolute/precious",
            "--in",
            &in_dir.to_string_lossy().into_owned(),
        ],
        make_pathbuf(&["sub", "dir", "precious"]),
    )?;

    run_test(
        &ubi,
        &["--project", "BurntSushi/ripgrep", "--exe", "rg"],
        make_pathbuf(&["bin", "rg"]),
    )?;

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

fn run_test(cmd: &PathBuf, args: &[&str], expect: PathBuf) -> Result<TempDir> {
    let td = tempdir()?;
    env::set_current_dir(td.path())?;

    match run_command(cmd, args) {
        Ok((code, stdout, stderr)) => {
            assert!(code == 0, "exit code is 0");
            assert!(stdout.is_none(), "no output to stdout");
            assert!(stderr.is_none(), "no output to stdout");
        }
        Err(e) => return Err(e),
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

pub fn run_command(cmd: &PathBuf, args: &[&str]) -> Result<(i32, Option<String>, Option<String>)> {
    let mut c = process::Command::new(cmd);
    for a in args.iter() {
        c.arg(a);
    }

    output_from_command(c, cmd, args)
}

fn output_from_command(
    mut c: process::Command,
    cmd: &PathBuf,
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

fn command_string(cmd: &PathBuf, args: &[&str]) -> String {
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
