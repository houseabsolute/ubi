// This code was written by Claude.ai and tweaked a fair bit by me.
//
// It provides traits that archive file are then implemented for various archive file types. This
// makes it easier to add support for new archive formats in the future.
use anyhow::Result;
use std::io::{self, Read};
use std::path::PathBuf;

pub(crate) trait ArchiveEntry {
    fn path(&self) -> Result<PathBuf>;
    fn is_file(&self) -> bool;
    fn is_executable(&self) -> Result<Option<bool>>;
}

pub(crate) struct TarEntriesIterator<'a, R: Read> {
    entries: binstall_tar::Entries<'a, R>,
}

impl<'a, R: Read> TarEntriesIterator<'a, R> {
    pub(crate) fn new(entries: binstall_tar::Entries<'a, R>) -> Self {
        Self { entries }
    }
}

impl<'a, R: Read> Iterator for TarEntriesIterator<'a, R> {
    type Item = Result<Box<dyn ArchiveEntry + 'a>, anyhow::Error>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.entries.next() {
            Some(Ok(entry)) => Some(Ok(Box::new(entry))),
            Some(Err(e)) => Some(Err(e.into())),
            None => None,
        }
    }
}

impl<R: Read> ArchiveEntry for binstall_tar::Entry<'_, R> {
    fn path(&self) -> Result<PathBuf> {
        Ok(self.path()?.to_path_buf())
    }

    fn is_file(&self) -> bool {
        self.header().entry_type().is_file()
    }

    fn is_executable(&self) -> Result<Option<bool>> {
        Ok(Some(self.header().mode()? & 0o111 != 0))
    }
}

pub(crate) struct SevenZipEntriesIterator<R: Read + io::Seek> {
    archive: sevenz_rust2::ArchiveReader<R>,
    current_index: usize,
}

impl<R: Read + io::Seek> SevenZipEntriesIterator<R> {
    pub(crate) fn new(archive: sevenz_rust2::ArchiveReader<R>) -> Self {
        Self {
            archive,
            current_index: 0,
        }
    }
}

impl<R: Read + io::Seek> Iterator for SevenZipEntriesIterator<R> {
    type Item = Result<Box<dyn ArchiveEntry>, anyhow::Error>;

    fn next(&mut self) -> Option<Self::Item> {
        let files = &self.archive.archive().files;
        if self.current_index >= files.len() {
            return None;
        }

        let entry = &files[self.current_index];

        self.current_index += 1;

        Some(Ok(Box::new(entry.clone())))
    }
}

impl ArchiveEntry for sevenz_rust2::ArchiveEntry {
    fn path(&self) -> Result<PathBuf> {
        Ok(PathBuf::from(self.name()))
    }

    fn is_file(&self) -> bool {
        !self.is_directory()
    }

    fn is_executable(&self) -> Result<Option<bool>> {
        // SevenZip entries do not mark whether something is executable.
        Ok(None)
    }
}

pub(crate) struct ZipEntriesIterator<'a, R: Read + io::Seek> {
    archive: &'a mut zip::ZipArchive<R>,
    current_index: usize,
}

impl<'a, R: Read + io::Seek> ZipEntriesIterator<'a, R> {
    pub(crate) fn new(archive: &'a mut zip::ZipArchive<R>) -> Self {
        Self {
            archive,
            current_index: 0,
        }
    }
}

impl<R: Read + io::Seek> Iterator for ZipEntriesIterator<'_, R> {
    type Item = Result<Box<dyn ArchiveEntry>, anyhow::Error>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.current_index >= self.archive.len() {
            return None;
        }

        let result = self
            .archive
            .by_index(self.current_index)
            .map(|file| OwnedZipEntry {
                name: file.name().to_string(),
                is_file: file.is_file(),
            });

        self.current_index += 1;

        match result {
            Ok(entry) => Some(Ok(Box::new(entry))),
            Err(e) => Some(Err(e.into())),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct OwnedZipEntry {
    name: String,
    is_file: bool,
}

impl ArchiveEntry for OwnedZipEntry {
    fn path(&self) -> Result<PathBuf> {
        Ok(PathBuf::from(&self.name))
    }

    fn is_file(&self) -> bool {
        self.is_file
    }

    fn is_executable(&self) -> Result<Option<bool>> {
        // Zip entries do not mark whether something is executable.
        Ok(None)
    }
}
