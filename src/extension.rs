use itertools::Itertools;
use strum::{EnumIter, IntoEnumIterator};

#[derive(Debug, EnumIter)]
pub(crate) enum Extension {
    Bz,
    Exe,
    Gz,
    TarBz,
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
            Extension::Exe => ".exe",
            Extension::Gz => ".gz",
            Extension::TarBz => ".tar.bz",
            Extension::TarGz => ".tar.gz",
            Extension::TarXz => ".tar.xz",
            Extension::Tbz => ".tbz",
            Extension::Tgz => ".tgz",
            Extension::Txz => ".txz",
            Extension::Xz => ".xz",
            Extension::Zip => ".zip",
        }
    }

    pub(crate) fn from_path<S: AsRef<str>>(path: S) -> Option<Extension> {
        let path = path.as_ref();
        // We need to try the longest extension first so that ".tar.gz"
        // matches before ".gz" and so on for other compression formats.
        Extension::iter()
            .sorted_by(|a, b| Ord::cmp(&a.extension().len(), &b.extension().len()))
            .rev()
            .find(|e| path.ends_with(e.extension()))
    }
}
