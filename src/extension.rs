use anyhow::Result;
use itertools::Itertools;
use std::path::Path;
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

    pub(crate) fn from_path<S: AsRef<str>>(path: S) -> Result<Option<Extension>> {
        let path = path.as_ref();
        let Some(ext_str) = Path::new(path).extension() else {
            return Ok(None);
        };

        // We need to try the longest extension first so that ".tar.gz"
        // matches before ".gz" and so on for other compression formats.
        match Extension::iter()
            .sorted_by(|a, b| Ord::cmp(&a.extension().len(), &b.extension().len()))
            .rev()
            .find(|e| path.ends_with(e.extension()))
        {
            Some(ext) => Ok(Some(ext)),
            None => Err(ExtensionError::UnknownExtension {
                path: path.to_string(),
                ext: ext_str.to_string_lossy().to_string(),
            }
            .into()),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use test_case::test_case;

    #[test_case("foo.bz", Ok(Some(Extension::Bz)))]
    #[test_case("foo.bz2", Ok(Some(Extension::Bz2)))]
    #[test_case("foo.exe", Ok(Some(Extension::Exe)))]
    #[test_case("foo.gz", Ok(Some(Extension::Gz)))]
    #[test_case("foo.tar.bz", Ok(Some(Extension::TarBz)))]
    #[test_case("foo.tar.bz2", Ok(Some(Extension::TarBz2)))]
    #[test_case("foo.tar.gz", Ok(Some(Extension::TarGz)))]
    #[test_case("foo.tar.xz", Ok(Some(Extension::TarXz)))]
    #[test_case("foo.xz", Ok(Some(Extension::Xz)))]
    #[test_case("foo.zip", Ok(Some(Extension::Zip)))]
    #[test_case("foo", Ok(None))]
    #[test_case("foo.bar", Err(ExtensionError::UnknownExtension { path: "foo.bar".to_string(), ext: "bar".to_string() }.into()))]
    fn from_path(path: &str, expect: Result<Option<Extension>>) {
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
