//! A library for downloading and installing pre-built binaries from GitHub.
//!
//! UBI stands for "Universal Binary Installer". It downloads and installs pre-built binaries from
//! GitHub releases. It is designed to be used in shell scripts and other automation.
//!
//! This project also ships a CLI tool named `ubi`. See [the project's GitHub
//! repo](https://github.com/houseabsolute/ubi) for more details on installing and using this tool.
//!
//! The main entry point for programmatic use is the [`UbiBuilder`] struct. Here is an example of its
//! usage:
//!
//! ```ignore
//! use ubi::UbiBuilder;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let ubi = UbiBuilder::new()
//!         .project("houseabsolute/precious")
//!         .install_dir("/usr/local/bin")
//!         .build()?;
//!
//!     ubi.install_binary().await?;
//!
//!     Ok(())
//! }
//! ```
//!
//! ## Installed Executable Naming
//!
//! If the release is in the form of a tarball or zip file, `ubi` will look in that archive file for
//! a file that matches the value given for the `exe` field, if any. Otherwise it looks for a file
//! with the same name as the project. In either case, the file will be installed with the name it
//! has in the archive file.
//!
//! If the release is in the form of a bare executable or a compressed executable, then the
//! installed executable will use the name of the project instead. For files with a `.exe`, `.jar`,
//! `.phar`, `.py`, `.pyz` `.sh`, or `.AppImage`, the installed executable will be
//! `$project_name.$extension`.
//!
//! This is a bit inconsistent, but it's how `ubi` has behaved since it was created, and I find this
//! to be the sanest behavior. Some projects, for example `rust-analyzer`, provide releases as
//! executables with names like `rust-analyzer-x86_64-apple-darwin` and
//! `rust-analyzer-x86_64-unknown-linux-musl`, so installing these as `rust-analyzer` seems like
//! better behavior.
//!
//! ## How `ubi` Finds the Right Release Artifact
//!
//! <div class="warning">Note that the exact set of steps that are followed to find a release
//! artifacts is not considered part of the API, and may change in any future release.</div>
//!
//! If you work on a project and you'd like to make sure that `ubi` can install it, please see [my
//! blog post, Naming Your Binary Executable
//! Releases](https://blog.urth.org/2023/04/16/naming-your-binary-executable-releases/) for more
//! details.
//!
//! When you call [`Ubi::install_binary`], it looks at the release assets (downloadable files) for a
//! project and tries to find the "right" asset for the platform it's running on. The matching logic
//! currently works like this:
//!
//! First it filters out assets with extensions it doesn't recognize. Right now this is anything that
//! doesn't match one of the following:
//!
//! - `.7z`
//! - `.AppImage` (Linux only)
//! - `.bat` (Windows only)
//! - `.bz`
//! - `.bz2`
//! - `.exe` (Windows only)
//! - `.gz`
//! - `.jar`
//! - `.phar`
//! - `.py`
//! - `.pyz`
//! - `.sh`
//! - `.tar`
//! - `.tar.bz`
//! - `.tar.bz2`
//! - `.tar.gz`
//! - `.tar.xz`
//! - `.tbz`
//! - `.tgz`
//! - `.txz`
//! - `.xz`
//! - `.zip`
//! - No extension
//!
//! It tries to be careful about what constitutes an extension. It's common for release filenames to
//! include a dot (`.`) in the filename before something that's _not_ intended as an extension, for
//! example `some-tool.linux.amd64` or `some-tools-linux-x86-64-1.3.5.tar.gz`.
//!
//! If, after filtering for extensions, there's only one asset, it will try to install this one, on
//! the assumption that this project releases assets which are not platform-specific (like a shell
//! script) _or_ that this project only releases for one platform and you're running `ubi` on that
//! platform.
//!
//! If there are multiple matching assets, it will first filter them based on your platform. It does
//! this in several stages:
//!
//! - First it filters based on your OS, which is something like Linux, macOS, Windows, FreeBSD,
//!   etc. It looks at the asset filenames to see which ones match your OS, using a (hopefully
//!   complete) regex.
//! - Next it filters based on your CPU architecture, which is something like x86-64, ARM64, `PowerPC`,
//!   etc. Again, this is done with a regex.
//! - If you are running on a Linux system using musl as its libc, it will also filter out anything
//!   _not_ compiled against musl. This filter looks to see if the file name contains an indication
//!   of which libc it was compiled against. Typically, this is something like "-gnu" or "-musl". If
//!   it does contain this indicator, names that are _not_ musl are filtered out. However, if there
//!   is no libc indicator, the asset will still be included. You can use the
//!   [`UbiBuilder::is_musl`] method to explicitly say that the platform is using musl. If this
//!   isn't set, then it will try to detect if you are using musl by looking at the output of `ldd
//!   /bin/ls`. However, if there is no libc indicator, the asset will still be included, but musl
//!   assets will be preferred over assets with no indication of which libc they use.
//!
//! At this point, any remaining assets should work on your platform, so if there's more than one
//! match, it attempts to pick the best one.
//!
//! - If it finds both 64-bit and 32-bit assets and you are on a 64-bit platform, it filters out the
//!   32-bit assets.
//! - If you've provided a string to [`UbiBuilder::matching`], this is used as a filter at this
//!   point.
//! - If your platform is macOS on ARM64 and there are assets for both x86-64 and ARM64, it filters
//!   out the non-ARM64 assets.
//!
//! Finally, if there are still multiple assets left, it sorts them by file name and picks the first
//! one. The sorting is done to make sure it always picks the same one every time it's run .
//!
//! ## How `ubi` Finds the Right Executable in an Archive File
//!
//! If the selected release artifact is an archive file (a tarball or zip file), then `ubi` will
//! look inside the archive to find the right executable.
//!
//! It first tries to find a file matching the exact name of the project (plus an extension on
//! Windows). So for example, if you're installing
//! [`houseabsolute/precious`](https://github.com/houseabsolute/precious), it will look in the
//! archive for a file named `precious` on Unix-like systems and `precious.bat` or `precious.exe` on
//! Windows. Note that if it finds an exact match, it does not check the file's mode.
//!
//! If it can't find an exact match it will look for a file that _starts with_ the project
//! name. This is mostly to account for projects that include things like platforms or release names
//! in their executables. Using
//! [`houseabsolute/precious`](https://github.com/houseabsolute/precious) as an example again, it
//! will match a file named `precious-linux-amd64` or `precious-v1.2.3`. In this case, it will
//! _rename_ the extracted file to `precious`. On Unix-like systems, these partial matches will only
//! be considered if the file's mode includes an executable bit. On Windows, it looks for a partial
//! match that is a `.bat` or `.exe` file, and the extracted file will be renamed to `precious.bat`
//! or `precious.exe`.
//!
//! ## Features
//!
//! This crate offers several features to control the TLS dependency used by `reqwest`:
//!
#![doc = document_features::document_features!()]

mod arch;
mod archive;
mod builder;
mod extension;
mod forge;
mod forgejo;
mod github;
mod gitlab;
mod installer;
mod os;
mod picker;
#[cfg(test)]
mod test;
#[cfg(test)]
mod test_log;
mod ubi;

pub use crate::{builder::UbiBuilder, forge::ForgeType, ubi::Ubi};

// The version of the `ubi` crate.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(feature = "logging")]
use fern::{
    colors::{Color, ColoredLevelConfig},
    Dispatch,
};
#[cfg(feature = "logging")]
use std::io::IsTerminal;

/// This determines whether `ubi` colorizes its logging output.
// It'd be nice to use clap::ValueEnum here, but then we'd need to add clap as a dependency for the
// library code, which would be annoying for downstream users who just want to use the library.
#[cfg(feature = "logging")]
#[derive(
    strum::AsRefStr,
    Clone,
    Copy,
    Debug,
    Default,
    strum::EnumString,
    PartialEq,
    Eq,
    strum::VariantNames,
)]
pub enum ColorChoice {
    /// Colorize output when stderr is a terminal and the `NO_COLOR` environment variable is not
    /// set to a value that asks us to disable color. See [`no_color_is_set`] for what counts.
    #[strum(serialize = "auto")]
    #[default]
    Auto,
    /// Always colorize output, even when stderr is not a terminal.
    #[strum(serialize = "always")]
    Always,
    /// Never colorize output.
    #[strum(serialize = "never")]
    Never,
}

/// Returns true if the `NO_COLOR` env var is set to a value that asks us to disable color.
///
/// <https://no-color.org/> says that any non-empty value disables color, regardless of what that
/// value is. We deviate from that in one place, by also treating `0` as unset. Someone who writes
/// `NO_COLOR=0` quite clearly means "do not disable color", and honoring the spec to the letter
/// here would do the opposite of what they asked for.
#[cfg(feature = "logging")]
fn no_color_is_set() -> bool {
    std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty() && v.to_str() != Some("0"))
}

#[cfg(feature = "logging")]
impl ColorChoice {
    fn use_color(self) -> bool {
        match self {
            ColorChoice::Always => true,
            ColorChoice::Never => false,
            ColorChoice::Auto => !no_color_is_set() && std::io::stderr().is_terminal(),
        }
    }
}

/// This function initializes logging for the application. It's public for the sake of the `ubi`
/// binary, but it lives in the library crate so that test code can also enable logging.
///
/// # Errors
///
/// This can return a `log::SetLoggerError` error.
#[cfg(feature = "logging")]
pub fn init_logger(level: log::LevelFilter, color: ColorChoice) -> Result<(), log::SetLoggerError> {
    let dispatch = if color.use_color() {
        let line_colors = ColoredLevelConfig::new()
            .error(Color::Red)
            .warn(Color::Yellow)
            .info(Color::BrightBlack)
            .debug(Color::BrightBlack)
            .trace(Color::BrightBlack);
        let level_colors = line_colors.info(Color::Green).debug(Color::Black);

        Dispatch::new().format(move |out, message, record| {
            out.finish(format_args!(
                "{color_line}[{target}][{level}{color_line}] {message}\x1B[0m",
                color_line = format_args!(
                    "\x1B[{}m",
                    line_colors.get_color(&record.level()).to_fg_str()
                ),
                target = record.target(),
                level = level_colors.color(record.level()),
                message = message,
            ));
        })
    } else {
        Dispatch::new().format(|out, message, record| {
            out.finish(format_args!(
                "[{target}][{level}] {message}",
                target = record.target(),
                level = record.level(),
                message = message,
            ));
        })
    };

    dispatch
        .level(level)
        // This is very noisy.
        .level_for("hyper", log::LevelFilter::Error)
        .chain(std::io::stderr())
        .apply()
}

#[cfg(all(test, feature = "logging"))]
mod color_choice_tests {
    use super::ColorChoice;
    use serial_test::serial;
    use std::env;

    /// Sets `NO_COLOR` to the given value and restores the original value when dropped.
    struct NoColorGuard(Option<std::ffi::OsString>);

    impl NoColorGuard {
        fn new(value: Option<&str>) -> Self {
            let saved = env::var_os("NO_COLOR");
            match value {
                Some(v) => env::set_var("NO_COLOR", v),
                None => env::remove_var("NO_COLOR"),
            }
            NoColorGuard(saved)
        }
    }

    impl Drop for NoColorGuard {
        fn drop(&mut self) {
            match self.0.take() {
                Some(v) => env::set_var("NO_COLOR", v),
                None => env::remove_var("NO_COLOR"),
            }
        }
    }

    #[test]
    #[serial]
    fn always_and_never_ignore_no_color() {
        for no_color in [None, Some(""), Some("0"), Some("1")] {
            let _guard = NoColorGuard::new(no_color);
            assert!(
                ColorChoice::Always.use_color(),
                "Always with NO_COLOR={no_color:?}"
            );
            assert!(
                !ColorChoice::Never.use_color(),
                "Never with NO_COLOR={no_color:?}"
            );
        }
    }

    #[test]
    #[serial]
    fn auto_respects_no_color() {
        // Tests do not run with a terminal attached to stderr, so `Auto` is never colorized here.
        // What we can check is that a meaningfully-set `NO_COLOR` disables color no matter what,
        // which is the half of the logic that doesn't depend on the environment the test runs in.
        for no_color in ["1", "true", "yes", "anything at all"] {
            let _guard = NoColorGuard::new(Some(no_color));
            assert!(!ColorChoice::Auto.use_color(), "NO_COLOR={no_color}");
        }
    }

    #[test]
    #[serial]
    fn auto_matches_stderr_is_a_terminal_when_no_color_is_not_meaningfully_set() {
        use std::io::IsTerminal;
        let is_terminal = std::io::stderr().is_terminal();
        // An unset, empty, or `0` value all mean "the user did not ask us to disable color".
        for no_color in [None, Some(""), Some("0")] {
            let _guard = NoColorGuard::new(no_color);
            assert_eq!(
                ColorChoice::Auto.use_color(),
                is_terminal,
                "Auto with NO_COLOR={no_color:?}"
            );
        }
    }
}
