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
//! installed executable will use the name of the project instead. For files with a `.exe`, `.pyz`
//! or `.AppImage`, the installed executable will be `$project_name.$extension`.
//!
//! This is a bit inconsistent, but it's how `ubi` has behaved since it was created, and I find this
//! to be the sanest behavior. Some projects, for example `rust-analyzer`, provide releases as
//! executables with names like `rust-analyzer-x86_64-apple-darwin` and
//! `rust-analyzer-x86_64-unknown-linux-musl`, so installing these as `rust-analyzer` seems like
//! better behavior.
//!
//!
//! ## How `ubi` Finds the Right Release Artifact
//!
//! <div class="warning">Note that the exact set of steps that are followed to find a release
//! artifacts is not considered part of the API, and may change in any future release.</div>
//!
//! When you call [`Ubi::install_binary`], it looks at the release assets (downloadable files) for a
//! project and tries to find the "right" asset for the platform it's running on. The matching logic
//! currently works like this:
//!
//! First it filters out assets with extensions it doesn't recognize. Right now this is anything that
//! doesn't match one of the following:
//!
//! - `.AppImage` (Linux only)
//! - `.bat` (Windows only)
//! - `.bz`
//! - `.bz2`
//! - `.exe` (Windows only)
//! - `.gz`
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
//! example `some-tool.linux.amd64`.
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
//! - Next it filters based on your CPU architecture, which is something like x86-64, ARM64, PowerPC,
//!   etc. Again, this is done with a regex.
//! - If you are running on a Linux system using musl as its libc, it will also filter out anything
//!   _not_ compiled against musl. This filter looks to see if the file name contains an indication
//!   of which libc it was compiled against. Typically, this is something like "-gnu" or "-musl". If
//!   it does contain this indicator, names that are _not_ musl are filtered out. However, if there
//!   is no libc indicator, the asset will still be included. You can use the
//!   [`UbiBuilder::is_musl`] method to explicitly say that the platform is using musl. If this
//!   isn't set, then it will try to detect if you are using musl by looking at the output of `ldd
//!   /bin/ls`.
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
mod builder;
mod extension;
mod forge;
mod github;
mod gitlab;
mod installer;
mod os;
mod picker;
#[cfg(test)]
mod test;
#[cfg(test)]
mod test_case;
mod ubi;

pub use crate::{builder::UbiBuilder, forge::ForgeType, ubi::Ubi};

// The version of the `ubi` crate.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(feature = "logging")]
use fern::{
    colors::{Color, ColoredLevelConfig},
    Dispatch,
};

/// This function initializes logging for the application. It's public for the sake of the `ubi`
/// binary, but it lives in the library crate so that test code can also enable logging.
///
/// # Errors
///
/// This can return a `log::SetLoggerError` error.
#[cfg(feature = "logging")]
pub fn init_logger(level: log::LevelFilter) -> Result<(), log::SetLoggerError> {
    let line_colors = ColoredLevelConfig::new()
        .error(Color::Red)
        .warn(Color::Yellow)
        .info(Color::BrightBlack)
        .debug(Color::BrightBlack)
        .trace(Color::BrightBlack);
    let level_colors = line_colors.info(Color::Green).debug(Color::Black);

    Dispatch::new()
        .format(move |out, message, record| {
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
        .level(level)
        // This is very noisy.
        .level_for("hyper", log::LevelFilter::Error)
        .chain(std::io::stderr())
        .apply()
}
