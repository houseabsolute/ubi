use anyhow::{anyhow, Context, Result};
#[cfg(target_family = "unix")]
use std::os::unix::fs::PermissionsExt;
#[cfg(target_family = "unix")]
use std::os::unix::prelude::*;
use std::{
    env, fs,
    path::{Path, PathBuf},
    process,
};
use tempfile::TempDir;
use which::which;

// Getting these running on other archs would be challenging. First, we'd have to use `cross build`
// when building the ubi that's run for tests. Second, the tests would have to be adjusted to
// account for every platform with a release for one of the projects that's used in these tests.
#[cfg(any(
    all(target_os = "linux", target_arch = "x86_64"),
    target_os = "macos",
    target_os = "windows"
))]
#[test]
#[allow(clippy::too_many_lines)]
fn integration_tests() -> Result<()> {
    let cargo = make_exe_pathbuf(&["cargo"]);
    run_command(&cargo, &["build"])?;

    let mut ubi = env::current_dir()?;
    ubi.push("..");
    ubi.push("target");
    ubi.push("debug");
    ubi.push(if cfg!(windows) { "ubi.exe" } else { "ubi" });
    ubi = ubi.canonicalize()?;

    let mut td = TempDir::new()?;
    if let Ok(p) = env::var("UBI_TESTS_PRESERVE_TEMPDIR") {
        if !(p.is_empty() || p == "0") {
            println!("Preserving tempdir: {}", td.path().display());
            td.disable_cleanup(true);
        }
    }

    run_test(
        td.path(),
        ubi.as_ref(),
        &["--project", "houseabsolute/precious"],
        make_exe_pathbuf(&["bin", "precious"]),
    )?;

    {
        let precious_bin = make_exe_pathbuf(&["bin", "precious"]);
        if run_test(
            td.path(),
            ubi.as_ref(),
            &["--project", "houseabsolute/precious", "--tag", "v0.7.2"],
            precious_bin.clone(),
        )? {
            match run_command(precious_bin.as_ref(), &["--version"]) {
                Ok((stdout, _)) => {
                    assert!(stdout.is_some(), "got stdout from precious");
                    assert!(
                        stdout.unwrap().contains("precious 0.7.2"),
                        "downloaded version 0.7.2"
                    );
                }
                Err(e) => return Err(e),
            }
        }
    }

    run_test(
        td.path(),
        ubi.as_ref(),
        &["--project", "https://github.com/houseabsolute/precious"],
        make_exe_pathbuf(&["bin", "precious"]),
    )?;

    let rename_to = if cfg!(windows) {
        "gollum.exe"
    } else {
        "gollum"
    };
    run_test(
        td.path(),
        ubi.as_ref(),
        &[
            "--project",
            "https://github.com/houseabsolute/precious",
            "--rename-exe",
            rename_to,
        ],
        make_exe_pathbuf(&["bin", "gollum"]),
    )?;

    run_test(
        td.path(),
        ubi.as_ref(),
        &[
            "--project",
            "https://github.com/houseabsolute/precious/releases",
        ],
        make_exe_pathbuf(&["bin", "precious"]),
    )?;

    let in_dir = make_dir_pathbuf(&["sub", "dir"]);
    run_test(
        td.path(),
        ubi.as_ref(),
        &[
            "--project",
            "houseabsolute/precious",
            "--in",
            &in_dir.to_string_lossy(),
        ],
        make_exe_pathbuf(&["sub", "dir", "precious"]),
    )?;

    #[cfg(target_os = "linux")]
    {
        let precious_bin = make_exe_pathbuf(&["bin", "precious"]);
        if run_test(td.path(),
            ubi.as_ref(),
            &["--url", "https://github.com/houseabsolute/precious/releases/download/v0.1.7/precious-Linux-x86_64-musl.tar.gz"],
            make_exe_pathbuf(&["bin", "precious"]),
        )?{
        match run_command(precious_bin.as_ref(), &["--version"]) {
            Ok((stdout, _)) => {
                assert!(stdout.is_some(), "got stdout from precious");
                assert!(
                    stdout.unwrap().contains("precious 0.1.7"),
                    "downloaded version 0.1.7"
                );
            }
            Err(e) => return Err(e),
        }}
    }

    run_test(
        td.path(),
        ubi.as_ref(),
        &["--project", "BurntSushi/ripgrep", "--exe", "rg"],
        make_exe_pathbuf(&["bin", "rg"]),
    )?;

    {
        let rust_analyzer_bin = make_exe_pathbuf(&["bin", "rust-analyzer"]);
        if run_test(
            td.path(),
            ubi.as_ref(),
            &["--project", "rust-analyzer/rust-analyzer"],
            rust_analyzer_bin.clone(),
        )? {
            match run_command(rust_analyzer_bin.as_ref(), &["--help"]) {
                Ok((stdout, _)) => {
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
    }

    {
        let golangci_lint_bin = make_exe_pathbuf(&["bin", "golangci-lint"]);
        if run_test(
            td.path(),
            ubi.as_ref(),
            &["--project", "golangci/golangci-lint"],
            golangci_lint_bin.clone(),
        )? {
            match run_command(golangci_lint_bin.as_ref(), &["--version"]) {
                Ok((stdout, _)) => {
                    assert!(stdout.is_some(), "got stdout from golangci-lint");
                    assert!(
                        stdout.unwrap().contains("golangci-lint has version"),
                        "got the expected stdout",
                    );
                }
                Err(e) => return Err(e),
            }
        }
    }

    let ref_name = env::var("GITHUB_REF_NAME").unwrap_or_default();
    // This test can fail if run on a tag that triggers a release, because it will find a new
    // in-progress release.
    if !(ref_name.starts_with('v') || ref_name.starts_with("ubi-v")) {
        let new_ubi_dir = TempDir::new()?;
        let ubi_copy = make_exe_pathbuf(&[
            new_ubi_dir
                .path()
                .to_str()
                .ok_or_else(|| anyhow!("Could not convert path to str"))?,
            "ubi",
        ]);
        fs::copy(ubi.as_path(), ubi_copy.as_path())?;

        #[cfg(target_family = "unix")]
        let old_stat = fs::metadata(ubi_copy.as_path())?;

        if run_test(
            td.path(),
            ubi_copy.as_ref(),
            &["--self-upgrade"],
            ubi_copy.clone(),
        )? {
            #[cfg(target_family = "unix")]
            {
                let new_stat = fs::metadata(ubi_copy)?;
                // The "new" version will have an older modified time, since it's the
                // creation time from the tarball/zip file entry, not the time it's
                // written to disk after downloading.
                let old_modified = old_stat.modified()?;
                let new_modified = new_stat.modified()?;
                assert!(
                    old_modified > new_modified,
                    "new version of ubi was downloaded ({old_modified:?} > {new_modified:?})",
                );
            }
        }
    }

    // This project's 22.08.1 release has an xz-compressed tarball. This project's Linux release
    // does not work on a musl platform. Strictly speaking, this test can be run when the _target_
    // is musl but the platform is not, but it's easiest to skip it with a `cfg` directive.
    #[cfg(not(target_env = "musl"))]
    {
        let hx_bin = make_exe_pathbuf(&["bin", "hx"]);
        if run_test(
            td.path(),
            ubi.as_ref(),
            &[
                "--project",
                "helix-editor/helix",
                "--exe",
                "hx",
                "--tag",
                "22.08.1",
            ],
            hx_bin.clone(),
        )? {
            match run_command(hx_bin.as_ref(), &["--version"]) {
                Ok((stdout, _)) => {
                    assert!(stdout.is_some(), "got stdout from hx");
                    assert!(
                        stdout.unwrap().contains("22.08.1"),
                        "got the expected stdout",
                    );
                }
                Err(e) => return Err(e),
            }
        }
    }

    // This project only has a Linux release. It has a single `.xz` file in
    // its releases which uncompresses to an ELF binary.
    #[cfg(target_os = "linux")]
    {
        let prettycrontab_bin = make_exe_pathbuf(&["bin", "prettycrontab"]);
        if run_test(
            td.path(),
            ubi.as_ref(),
            &["--project", "mfontani/prettycrontab", "--tag", "v0.0.2"],
            prettycrontab_bin.clone(),
        )? {
            match run_command(prettycrontab_bin.as_ref(), &["-version"]) {
                Ok((stdout, _)) => {
                    assert!(stdout.is_some(), "got stdout from prettycrontab");
                    assert!(
                        stdout.unwrap().contains("v0.0.2"),
                        "got the expected stdout",
                    );
                }
                Err(e) => return Err(e),
            }
        }
    }

    // This project has multiple releases, which are binaries.
    // The darwin and linux executables are xz'ed.
    #[cfg(target_os = "linux")]
    {
        let tstdin_bin = make_exe_pathbuf(&["bin", "tstdin"]);
        if run_test(
            td.path(),
            ubi.as_ref(),
            &["--project", "mfontani/tstdin", "--tag", "v0.2.3"],
            tstdin_bin.clone(),
        )? {
            match run_command(tstdin_bin.as_ref(), &["-version"]) {
                Ok((stdout, _)) => {
                    assert!(stdout.is_some(), "got stdout from tstdin");
                    assert!(
                        stdout.unwrap().contains("v0.2.3"),
                        "got the expected stdout",
                    );
                }
                Err(e) => return Err(e),
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        let delta_bin = make_exe_pathbuf(&["bin", "delta"]);
        if run_test(
            td.path(),
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
        )? {
            match run_command(delta_bin.as_ref(), &["--version"]) {
                Ok((stdout, _)) => {
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
                Ok((stdout, _)) => {
                    assert!(stdout.is_some(), "got stdout from file");
                    assert!(
                        stdout.unwrap().contains("statically linked"),
                        "got the expected stdout",
                    );
                }
                Err(e) => return Err(e),
            }
        }
    }

    // The omegasort release for macOS on an M1 is named
    // "omegasort_0.0.5_Darwin_arm64.tar.gz", but macOS reports its
    // architecture as "aarch64". This was fixed in ubi 0.0.16.
    run_test(
        td.path(),
        ubi.as_ref(),
        &["--project", "https://github.com/houseabsolute/omegasort"],
        make_exe_pathbuf(&["bin", "omegasort"]),
    )?;

    // This is a bare binary. The Windows release has a filename ending in
    // ".exe" which ubi initially rejected.
    run_test(
        td.path(),
        ubi.as_ref(),
        &["--project", "tailwindlabs/tailwindcss", "--tag", "v3.2.7"],
        make_exe_pathbuf(&["bin", "tailwindcss"]),
    )?;

    // The tarball for the Darwin x86-64 release uses the GNU sparse format,
    // which isn't supported by the tar crate
    // (https://github.com/alexcrichton/tar-rs/issues/295). Switching to the
    // binstall-tar fork of the tar crate fixes this.
    //
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    {
        let ubi_bin = make_exe_pathbuf(&["bin", "ubi"]);
        if run_test(
            td.path(),
            ubi.as_ref(),
            &["--project", "houseabsolute/ubi", "--tag", "v0.0.27"],
            ubi_bin.clone(),
        )? {
            match run_command(ubi_bin.as_ref(), &["--version"]) {
                Ok((stdout, _)) => {
                    assert!(stdout.is_some(), "got stdout from ubi");
                    assert!(
                        stdout.unwrap().contains("ubi 0.0.27"),
                        "got the expected stdout",
                    );
                }
                Err(e) => return Err(e),
            }
        }
    }

    // This project has some releases that contain an architecture in the name
    // and some that don't, e.g. `yt-dlp_linux` and
    // `yt-dlp_linux_aarch64`. This tests that we pick one of the
    // no-architecture releases when there's no file matching our
    // architecture.
    run_test(
        td.path(),
        ubi.as_ref(),
        &["--project", "yt-dlp/yt-dlp", "--tag", "2024.04.09"],
        make_exe_pathbuf(&["bin", "yt-dlp"]),
    )?;

    // This project releases bare binaries with the platform name looking like an extension,
    // e..g. "direnv.darwin-amd64".
    run_test(
        td.path(),
        ubi.as_ref(),
        &["--project", "direnv/direnv", "--tag", "v2.35.0"],
        make_exe_pathbuf(&["bin", "direnv"]),
    )?;

    // This project used just "mac" in the macOS release names, which `ubi` didn't look for until
    // 0.2.4.
    run_test(
        td.path(),
        ubi.as_ref(),
        &[
            "--project",
            "wren-lang/wren-cli",
            "--tag",
            "0.4.0",
            "--exe",
            "wren_cli",
        ],
        make_exe_pathbuf(&["bin", "wren_cli"]),
    )?;

    run_test(
        td.path(),
        ubi.as_ref(),
        &[
            "--project",
            "gitlab-org/cli",
            "--exe",
            "glab",
            "--forge",
            "gitlab",
        ],
        make_exe_pathbuf(&["bin", "glab"]),
    )?;

    run_test(
        td.path(),
        ubi.as_ref(),
        &[
            "--project",
            "gitlab-org/cli",
            "--tag",
            "v1.49.0",
            "--exe",
            "glab",
            "--forge",
            "gitlab",
        ],
        make_exe_pathbuf(&["bin", "glab"]),
    )?;

    run_test(
        td.path(),
        ubi.as_ref(),
        &[
            "--project",
            "https://gitlab.com/gitlab-org/cli/-/releases",
            "--tag",
            "v1.49.0",
            "--exe",
            "glab",
        ],
        make_exe_pathbuf(&["bin", "glab"]),
    )?;

    // Test deeply nested GitLab project - skip on Windows there is no windows binary for target
    #[cfg(not(target_os = "windows"))]
    {
        run_test(
            td.path(),
            ubi.as_ref(),
            &[
                "--project",
                "gitlab-com/gl-infra/terra-transformer",
                "--forge",
                "gitlab",
            ],
            make_exe_pathbuf(&["bin", "terra-transformer"]),
        )?;
    }

    // Test deeply nested GitLab project - skip on Windows there is no windows binary for target
    #[cfg(not(target_os = "windows"))]
    {
        run_test(
            td.path(),
            ubi.as_ref(),
            &[
                "--project",
                "https://gitlab.com/gitlab-com/gl-infra/terra-transformer/-/issues/1",
                "--tag",
                "v1.31.17",
            ],
            make_exe_pathbuf(&["bin", "terra-transformer"]),
        )?;
    }

    #[cfg(target_os = "linux")]
    {
        run_test(
            td.path(),
            ubi.as_ref(),
            &["--project", "hzeller/timg", "--tag", "v1.6.1"],
            make_exe_pathbuf(&["bin", "timg.AppImage"]),
        )?;
    }

    // This project releases macOS binaries named "golines_0.12.2_darwin_all.tar.gz".
    #[cfg(target_os = "macos")]
    {
        run_test(
            td.path(),
            ubi.as_ref(),
            &["--project", "segmentio/golines", "--tag", "v0.12.2"],
            make_exe_pathbuf(&["bin", "golines"]),
        )?;
    }

    // As of their v5.0.0 release, this project produced archive file that contained executables
    // with names like `scorecard-linux-amd64`. They have since changed that started with v5.1.0.
    run_test(
        td.path(),
        ubi.as_ref(),
        &["--project", "ossf/scorecard", "--tag", "v5.0.0"],
        make_exe_pathbuf(&["bin", "scorecard"]),
    )?;

    run_test(
        td.path(),
        ubi.as_ref(),
        &[
            "--project",
            "shader-slang/slang",
            "--tag",
            "v2025.9.2",
            "--exe",
            "slangc",
            "--matching-regex",
            r"\d+\.tar",
        ],
        make_exe_pathbuf(&["bin", "slangc"]),
    )?;

    #[cfg(target_os = "windows")]
    {
        run_test(
            td.path(),
            ubi.as_ref(),
            &[
                "--project",
                "ip7z/7zip",
                "--tag",
                "25.00",
                "--exe",
                "7za.exe",
                "--matching-regex",
                r"extra\.7z",
            ],
            make_exe_pathbuf(&["bin", "7za.exe"]),
        )?;

        run_test(
            td.path(),
            ubi.as_ref(),
            &[
                "--project",
                "ip7z/7zip",
                "--tag",
                "25.00",
                "--matching-regex",
                r"extra\.7z",
                "--extract-all",
            ],
            make_exe_pathbuf(&["bin", "7za.exe"]),
        )?;
    }

    {
        if run_test(
            td.path(),
            ubi.as_ref(),
            &[
                "--project",
                "neovim/neovim",
                "--tag",
                "v0.11.2",
                "--extract-all",
                "--in",
                ".",
            ],
            make_exe_pathbuf(&["bin", "nvim"]),
        )? {
            const DYN_LIB: &str = if cfg!(windows) { "c.dll" } else { "c.so" };

            let root = td.path().to_path_buf();

            let build_path = |components| {
                let mut pb = root.clone();
                pb.extend(components);
                pb
            };

            let mut extra_paths = vec![build_path(vec!["lib", "nvim", "parser", DYN_LIB])];
            if cfg!(target_os = "linux") {
                extra_paths.push(build_path(vec!["share", "applications", "nvim.desktop"]));
            }

            for p in extra_paths {
                assert!(fs::exists(&p)?, "expected file to exist: {}", p.display());
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        run_test(
            td.path(),
            ubi.as_ref(),
            &[
                "--project",
                "https://codeberg.org/Cyborus/forgejo-cli/",
                "--tag",
                "v0.3.0",
            ],
            make_exe_pathbuf(&["bin", "forgejo-cli"]),
        )?;
    }

    Ok(())
}

fn make_exe_pathbuf(path: &[&str]) -> PathBuf {
    let mut pb = make_dir_pathbuf(path);
    if cfg!(windows) {
        pb.set_extension("exe");
    }
    pb
}

fn make_dir_pathbuf(path: &[&str]) -> PathBuf {
    let mut iter = path.iter();
    let mut pb = PathBuf::from(iter.next().unwrap());
    for v in iter {
        pb.push(v);
    }
    pb
}

fn run_test(td: &Path, cmd: &Path, args: &[&str], expect: PathBuf) -> Result<bool> {
    if let Ok(v) = env::var("UBI_TESTS_INTEGRATION_ONLY") {
        if !(v.is_empty() || (args.len() > 1 && args[1].contains(&v))) {
            println!(
                "Skipping test for [{}] because it does not contain {v}",
                args.join(" "),
            );
            return Ok(false);
        }
    }

    println!("Running test [{}]", args.join(" "));

    clean_tempdir(td)?;
    env::set_current_dir(td)?;

    let debug = matches!(env::var("UBI_TESTS_DEBUG"), Ok(v) if !(v.is_empty() || v == "0"));
    let mut args = args.to_vec();
    if debug {
        args.push("--debug");
    }

    check_command_result(cmd, &args, debug)?;
    if let Err(e) = check_installed_binary(td, expect) {
        dump_tree(td)?;
        return Err(e);
    }

    Ok(true)
}

fn clean_tempdir(td: &Path) -> Result<()> {
    for entry in fs::read_dir(td)? {
        let entry = entry?;
        if entry.metadata()?.is_dir() {
            fs::remove_dir_all(entry.path())?;
        } else {
            fs::remove_file(entry.path())?;
        }
    }

    Ok(())
}

fn check_command_result(cmd: &Path, args: &[&str], debug: bool) -> Result<()> {
    let (stdout, stderr) = run_command(cmd, args)?;

    if cfg!(windows) && args.contains(&"--self-upgrade") {
        assert!(stdout.unwrap_or_default().contains(
            "The self-upgrade operation left an old binary behind that must be deleted manually"
        ));
    } else {
        assert_eq!(
            stdout.unwrap_or_default(),
            String::new(),
            "no output to stdout",
        );
    }
    if debug {
        print!("{}", stderr.unwrap());
    } else {
        assert_eq!(
            stderr.unwrap_or_default(),
            String::new(),
            "no output to stderr",
        );
    }

    Ok(())
}

fn run_command(cmd: &Path, args: &[&str]) -> Result<(Option<String>, Option<String>)> {
    let mut c = process::Command::new(cmd);
    for a in args {
        c.arg(a);
    }
    c.env(
        "GITHUB_TOKEN",
        env::var("GITHUB_TOKEN").as_deref().unwrap_or(""),
    );
    // Without this golangci-lint will try to use /.cache as its cache dir in
    // Docker containers, which it may not have access to.
    c.env("GOLANGCI_LINT_CACHE", env::temp_dir());

    output_from_command(c, cmd, args)
}

fn output_from_command(
    mut c: process::Command,
    cmd: &Path,
    args: &[&str],
) -> Result<(Option<String>, Option<String>)> {
    let cstr = command_string(cmd, args);
    println!("running {cstr}");

    let output = c.output()?;
    match output.status.code() {
        Some(0) => Ok((
            to_option_string(&output.stdout),
            to_option_string(&output.stderr),
        )),
        Some(code) => {
            let mut msg = format!("ran {cstr} and got non-zero exit code: {code}");
            if !output.stdout.is_empty() {
                msg.push_str("\nStdout:\n");
                msg.push_str(&String::from_utf8_lossy(&output.stdout));
            }
            if !output.stderr.is_empty() {
                msg.push_str("\nStderr:\n");
                msg.push_str(&String::from_utf8_lossy(&output.stderr));
            }
            Err(anyhow!(msg))
        }
        None => {
            let cstr = command_string(cmd, args);
            if output.status.success() {
                return Err(anyhow!("ran {} successfully but it had no exit code", cstr));
            }
            let signal = signal_from_status(output.status);
            Err(anyhow!(
                "ran {} successfully but was killed by signal {}",
                cstr,
                signal,
            ))
        }
    }
}

fn command_string(cmd: &Path, args: &[&str]) -> String {
    let mut cstr = cmd.to_string_lossy().to_string();
    if !args.is_empty() {
        cstr.push(' ');
        cstr.push_str(args.join(" ").as_str());
    }
    cstr
}

fn to_option_string(v: &[u8]) -> Option<String> {
    if v.is_empty() {
        None
    } else {
        Some(String::from_utf8_lossy(v).to_string())
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

fn check_installed_binary(td: &Path, mut expect: PathBuf) -> Result<()> {
    if cfg!(windows) && !expect.to_string_lossy().ends_with(".exe") {
        expect.set_extension("exe");
    }

    let expect_str = expect.to_string_lossy();

    let exists = fs::exists(&expect)?;
    if !exists {
        dump_tree(td)?;
    }
    assert!(fs::exists(&expect)?);
    let meta = fs::metadata(&expect).context(format!("getting fs metadata for {expect_str}"))?;
    assert!(meta.is_file(), "downloaded file into expected location");
    #[cfg(target_family = "unix")]
    assert!(
        meta.permissions().mode() & 0o111 != 0,
        "downloaded file is executable",
    );

    Ok(())
}

fn dump_tree(td: &Path) -> Result<()> {
    if let Ok(tree) = which("tree") {
        let output = process::Command::new(tree)
            .arg(td)
            .output()
            .context("running tree")?;
        println!("{}", String::from_utf8_lossy(&output.stdout));
    }

    Ok(())
}
