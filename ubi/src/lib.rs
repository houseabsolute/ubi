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
