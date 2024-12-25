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
//
//! If the release is in the form of a bare executable or a compressed executable, then the installed
//! executable will use the name of the project instead.
//
//! This is a bit inconsistent, but it's how `ubi` has behaved since it was created, and I find this
//! to be the sanest behavior. Some projects, for example `rust-analyzer`, provide releases as
//! compressed executables with names like `rust-analyzer-x86_64-apple-darwin.gz` and
//! `rust-analyzer-x86_64-unknown-linux-musl.gz`, so installing these as `rust-analyzer` seems like
//! better behavior.
//!
//!
//! ## How `ubi` Finds the Right Release Artifact
//!
//! When you call [`Ubi::install_binary`], it looks at the release assets (downloadable files) for a
//! project and tries to find the "right" asset for the platform it's running on. The matching logic
//! currently works like this:
//!
//! First it filters out assets with extensions it doesn't recognize. Right now this is anything that
//! doesn't match one of the following:
//!
//! - `.bz`
//! - `.bz2`
//! - `.exe`
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
//! It tries to be careful about what consistutes an extension. It's common for releases to include a
//! dot (`.`) in the filename before something that's _not_ intended as an extension, for example
//! `some-tool.linux.amd64`.
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
//!  etc. It looks at the asset filenames to see which ones match your OS, using a (hopefully
//!   complete) regex.
//! - Next it filters based on your CPU architecture, which is something like x86-64, ARM64, PowerPC,
//!   etc. Again, this is done with a regex.
//! - If you are running on a Linux system using musl as its libc, it will also filter based on this
//!   to filter out anything _not_ compiled against musl. You can set this explicitly with the
//!   [`UbiBuilder::is_musl`] function. If this wasn't set, then it will try to detect if you are
//!   using musl by looking at the output of `ldd /bin/ls`.
//!
//! At this point, any remaining assets should work on your platform. If there's more than one at
//! this point, it attempts to pick the best one.
//!
//! - If it finds both 64-bit and 32-bit assets and you are on a 64-bit platform, it filters out the
//!   32-bit assets.
//! - If you've provided a `--matching` string, this is used as a filter at this point.
//! - If your platform is macOS on ARM64 and there are assets for both x86-64 and ARM64, it filters
//!   out the non-ARM64 assets.
//!
//! Finally, if there are still multiple assets left, it sorts them by file name and picks the first
//! one.
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
