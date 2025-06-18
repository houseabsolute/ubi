use crate::arch::ALL_ARCHES_RE;
use crate::os::ALL_OSES_RE;
use anyhow::Result;
use itertools::Itertools;
use lazy_regex::regex;
use log::debug;
use platforms::{Platform, OS};
use regex::Regex;
use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
    sync::LazyLock,
};
use strum::{EnumIter, IntoEnumIterator};
use thiserror::Error;

#[derive(Debug, Error)]
pub(crate) enum ExtensionError {
    #[error("{} has unknown extension {ext:}", path.display())]
    UnknownExtension { path: PathBuf, ext: String },
}

#[derive(Debug, EnumIter, PartialEq, Eq)]
pub(crate) enum Extension {
    AppImage,
    Bat,
    Bz,
    Bz2,
    Exe,
    Gz,
    Jar,
    Phar,
    Pyz,
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
            Extension::AppImage => ".AppImage",
            Extension::Bat => ".bat",
            Extension::Bz => ".bz",
            Extension::Bz2 => ".bz2",
            Extension::Exe => ".exe",
            Extension::Gz => ".gz",
            Extension::Jar => ".jar",
            Extension::Phar => ".phar",
            Extension::Pyz => ".pyz",
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

    pub(crate) fn extension_without_dot(&self) -> &str {
        self.extension().strip_prefix('.').unwrap()
    }

    pub(crate) fn is_archive(&self) -> bool {
        match self {
            Extension::AppImage
            | Extension::Bat
            | Extension::Bz
            | Extension::Bz2
            | Extension::Exe
            | Extension::Gz
            | Extension::Jar
            | Extension::Phar
            | Extension::Pyz
            | Extension::Xz => false,
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

    pub(crate) fn should_preserve_extension_on_install(&self) -> bool {
        match self {
            Extension::AppImage
            | Extension::Bat
            | Extension::Exe
            | Extension::Jar
            | Extension::Phar
            | Extension::Pyz => true,
            Extension::Bz
            | Extension::Gz
            | Extension::Bz2
            | Extension::Tar
            | Extension::TarBz
            | Extension::TarBz2
            | Extension::TarGz
            | Extension::TarXz
            | Extension::Tbz
            | Extension::Tgz
            | Extension::Txz
            | Extension::Xz
            | Extension::Zip => false,
        }
    }

    pub(crate) fn matches_platform(&self, platform: &Platform) -> bool {
        match self {
            Extension::AppImage => platform.target_os == OS::Linux,
            Extension::Bat | Extension::Exe => platform.target_os == OS::Windows,
            _ => true,
        }
    }

    pub(crate) fn is_windows_only(&self) -> bool {
        matches!(self, Extension::Bat | Extension::Exe)
    }

    pub(crate) fn from_path(path: &Path) -> Result<Option<Extension>> {
        let Some(ext_str_from_path) = path.extension() else {
            return Ok(None);
        };
        let path_str = path.to_string_lossy();

        // We need to try the longest extensions first so that ".tar.gz" matches before ".gz" and so
        // on for other compression formats.
        if let Some(ext) = Extension::iter()
            .sorted_by(|a, b| Ord::cmp(&a.extension().len(), &b.extension().len()))
            .rev()
            // This is intentionally using a string comparison instead of looking at
            // path.extension(). That's because the `.extension()` method returns `"bz"` for paths
            // like "foo.tar.bz", instead of "tar.bz".
            .find(|e| path_str.ends_with(e.extension()))
        {
            return Ok(Some(ext));
        }

        if extension_is_part_of_version(path, ext_str_from_path) {
            debug!(
                "the extension {} is part of the version, ignoring",
                ext_str_from_path.to_string_lossy(),
            );
            return Ok(None);
        }

        if extension_is_platform(ext_str_from_path) {
            debug!(
                "the extension {} is a platform name, ignoring",
                ext_str_from_path.to_string_lossy(),
            );
            return Ok(None);
        }

        Err(ExtensionError::UnknownExtension {
            path: path.to_path_buf(),
            ext: ext_str_from_path.to_string_lossy().to_string(),
        }
        .into())
    }
}

fn extension_is_part_of_version(path: &Path, ext_str: &OsStr) -> bool {
    let ext_str = ext_str.to_string_lossy().to_string();

    let version_number_ext_re = regex!(r"^[0-9]+");
    if !version_number_ext_re.is_match(&ext_str) {
        return false;
    }

    // This matches something like "foo_3.2.1_linux_amd64" and captures "1_linux_amd64".
    let version_number_re = regex!(r"[0-9]+\.([0-9]+[^.]*)$");
    let Some(caps) = version_number_re.captures(path.to_str().expect(
        "this path came from a UTF-8 string originally so it should always convert back to one",
    )) else {
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
    static PLATFORM_RE: LazyLock<Regex> = LazyLock::new(|| {
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
    use test_log::test;

    #[test_case("foo.AppImage", Ok(Some(Extension::AppImage)))]
    #[test_case("foo.bz", Ok(Some(Extension::Bz)))]
    #[test_case("foo.bz2", Ok(Some(Extension::Bz2)))]
    #[test_case("foo.exe", Ok(Some(Extension::Exe)))]
    #[test_case("foo.gz", Ok(Some(Extension::Gz)))]
    #[test_case("foo.jar", Ok(Some(Extension::Jar)))]
    #[test_case("foo.phar", Ok(Some(Extension::Phar)))]
    #[test_case("foo.pyz", Ok(Some(Extension::Pyz)))]
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
    #[test_case("foo.bar", Err(ExtensionError::UnknownExtension { path: PathBuf::from("foo.bar"), ext: "bar".to_string() }.into()))]
    fn from_path(path: &str, expect: Result<Option<Extension>>) {
        crate::test_case::init_logging();

        let ext = Extension::from_path(Path::new(path));
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

    #[test]
    fn matches_platform() -> Result<()> {
        let freebsd = Platform::find("x86_64-unknown-freebsd").unwrap().clone();
        let linux = Platform::find("x86_64-unknown-linux-gnu").unwrap().clone();
        let macos = Platform::find("aarch64-apple-darwin").unwrap().clone();
        let windows = Platform::find("x86_64-pc-windows-msvc").unwrap().clone();

        let ext = Extension::from_path(Path::new("foo.exe"))?.unwrap();
        assert!(
            ext.matches_platform(&windows),
            "foo.exe is valid on {windows}"
        );
        for p in [&freebsd, &linux, &macos] {
            assert!(!ext.matches_platform(p), "foo.exe is not valid on {p}");
        }

        let ext = Extension::from_path(Path::new("foo.AppImage"))?.unwrap();
        assert!(
            ext.matches_platform(&linux),
            "foo.exe is valid on {windows}"
        );
        for p in [&freebsd, &macos, &windows] {
            assert!(!ext.matches_platform(p), "foo.AppImage is not valid on {p}");
        }

        let ext = Extension::from_path(Path::new("foo.tar.gz"))?.unwrap();
        for p in [&freebsd, &linux, &macos, &windows] {
            assert!(ext.matches_platform(p), "foo.tar.gz is valid on {p}");
        }

        Ok(())
    }
}
