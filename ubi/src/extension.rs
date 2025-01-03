use crate::arch::ALL_ARCHES_RE;
use crate::os::ALL_OSES_RE;
use anyhow::Result;
use itertools::Itertools;
use lazy_regex::{regex, Lazy};
use log::debug;
use regex::Regex;
use std::{ffi::OsStr, path::Path};
use strum::{EnumIter, IntoEnumIterator};
use thiserror::Error;

#[derive(Debug, Error)]
pub(crate) enum ExtensionError {
    #[error("{path:} has unknown extension {ext:}")]
    UnknownExtension { path: String, ext: String },
}

#[derive(Debug, EnumIter, PartialEq, Eq)]
pub(crate) enum Extension {
    Bz,
    Bz2,
    Exe,
    Gz,
    Tar,
    TarBz,
    TarBz2,
    TarGz,
    TarXz,
    Tbz,
    Tgz,
    Txz,
    Xz,
    Zip,
}

impl Extension {
    pub(crate) fn extension(&self) -> &'static str {
        match self {
            Extension::Bz => ".bz",
            Extension::Bz2 => ".bz2",
            Extension::Exe => ".exe",
            Extension::Gz => ".gz",
            Extension::Tar => ".tar",
            Extension::TarBz => ".tar.bz",
            Extension::TarBz2 => ".tar.bz2",
            Extension::TarGz => ".tar.gz",
            Extension::TarXz => ".tar.xz",
            Extension::Tbz => ".tbz",
            Extension::Tgz => ".tgz",
            Extension::Txz => ".txz",
            Extension::Xz => ".xz",
            Extension::Zip => ".zip",
        }
    }

    pub(crate) fn is_archive(&self) -> bool {
        match self {
            Extension::Bz | Extension::Bz2 | Extension::Exe | Extension::Gz | Extension::Xz => {
                false
            }
            Extension::Tar
            | Extension::TarBz
            | Extension::TarBz2
            | Extension::TarGz
            | Extension::TarXz
            | Extension::Tbz
            | Extension::Tgz
            | Extension::Txz
            | Extension::Zip => true,
        }
    }

    pub(crate) fn from_path<S: AsRef<str>>(path: S) -> Result<Option<Extension>> {
        let path = path.as_ref();
        let Some(ext_str) = Path::new(path).extension() else {
            return Ok(None);
        };

        // We need to try the longest extensions first so that ".tar.gz" matches before ".gz" and so
        // on for other compression formats.
        if let Some(ext) = Extension::iter()
            .sorted_by(|a, b| Ord::cmp(&a.extension().len(), &b.extension().len()))
            .rev()
            .find(|e| path.ends_with(e.extension()))
        {
            return Ok(Some(ext));
        }

        if extension_is_part_of_version(path, ext_str) {
            debug!("the extension {ext_str:?} is part of the version, ignoring");
            return Ok(None);
        }

        if extension_is_platform(ext_str) {
            debug!("the extension {ext_str:?} is a platform name, ignoring");
            return Ok(None);
        }

        Err(ExtensionError::UnknownExtension {
            path: path.to_string(),
            ext: ext_str.to_string_lossy().to_string(),
        }
        .into())
    }
}

fn extension_is_part_of_version(path: &str, ext_str: &OsStr) -> bool {
    let ext_str = ext_str.to_string_lossy().to_string();

    let version_number_ext_re = regex!(r"^[0-9]+");
    if !version_number_ext_re.is_match(&ext_str) {
        return false;
    }

    // This matches something like "foo_3.2.1_linux_amd64" and captures "1_linux_amd64".
    let version_number_re = regex!(r"[0-9]+\.([0-9]+[^.]*)$");
    let Some(caps) = version_number_re.captures(path) else {
        return false;
    };
    let Some(dot_num) = caps.get(1) else {
        return false;
    };

    // If the extension starts with the last part of the version then it's not
    // a real extension.
    ext_str == dot_num.as_str()
}

fn extension_is_platform(ext_str: &OsStr) -> bool {
    static PLATFORM_RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(
            &[ALL_OSES_RE.as_str(), ALL_ARCHES_RE.as_str()]
                .iter()
                .map(|r| format!("(?:{r})"))
                .join("|"),
        )
        .unwrap()
    });

    PLATFORM_RE.is_match(ext_str.to_string_lossy().as_ref())
}

#[cfg(test)]
mod test {
    use super::*;
    use test_case::test_case;

    #[test_case("foo.bz", Ok(Some(Extension::Bz)))]
    #[test_case("foo.bz2", Ok(Some(Extension::Bz2)))]
    #[test_case("foo.exe", Ok(Some(Extension::Exe)))]
    #[test_case("foo.gz", Ok(Some(Extension::Gz)))]
    #[test_case("foo.tar", Ok(Some(Extension::Tar)))]
    #[test_case("foo.tar.bz", Ok(Some(Extension::TarBz)))]
    #[test_case("foo.tar.bz2", Ok(Some(Extension::TarBz2)))]
    #[test_case("foo.tar.gz", Ok(Some(Extension::TarGz)))]
    #[test_case("foo.tar.xz", Ok(Some(Extension::TarXz)))]
    #[test_case("foo.xz", Ok(Some(Extension::Xz)))]
    #[test_case("foo.zip", Ok(Some(Extension::Zip)))]
    #[test_case("foo", Ok(None))]
    #[test_case("foo_3.2.1_linux_amd64", Ok(None))]
    #[test_case("foo_3.9.1.linux.amd64", Ok(None))]
    #[test_case("i386-linux-ghcup-0.1.30.0", Ok(None))]
    #[test_case("i386-linux-ghcup-0.1.30.0-linux_amd64", Ok(None))]
    #[test_case("foo.bar", Err(ExtensionError::UnknownExtension { path: "foo.bar".to_string(), ext: "bar".to_string() }.into()))]
    fn from_path(path: &str, expect: Result<Option<Extension>>) {
        crate::test_case::init_logging();

        let ext = Extension::from_path(path);
        if expect.is_ok() {
            assert!(ext.is_ok());
            assert_eq!(ext.unwrap(), expect.unwrap());
        } else {
            assert_eq!(
                ext.unwrap_err().to_string(),
                expect.unwrap_err().to_string()
            );
        }
    }
}
