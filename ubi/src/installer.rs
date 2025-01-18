use crate::{extension::Extension, ubi::Download};
use anyhow::{anyhow, Context, Result};
use binstall_tar::Archive;
use bzip2::read::BzDecoder;
use flate2::read::GzDecoder;
use log::{debug, info};
use std::{
    collections::HashSet,
    ffi::OsString,
    fmt::Debug,
    fs::{self, create_dir_all, File},
    io::{Read, Write},
    path::{Path, PathBuf},
};
use xz2::read::XzDecoder;
use zip::ZipArchive;

#[cfg(target_family = "unix")]
use std::fs::{set_permissions, Permissions};
#[cfg(target_family = "unix")]
use std::os::unix::fs::PermissionsExt;

pub(crate) trait Installer: Debug {
    fn install(&self, download: &Download) -> Result<()>;
}

#[derive(Debug)]
pub(crate) struct ExeInstaller {
    install_path: PathBuf,
    exe: String,
}

#[derive(Debug)]
pub(crate) struct ArchiveInstaller {
    install_root: PathBuf,
}

impl ExeInstaller {
    pub(crate) fn new(install_path: PathBuf, exe: String) -> Self {
        ExeInstaller { install_path, exe }
    }

    fn extract_executable(&self, downloaded_file: &Path) -> Result<()> {
        match Extension::from_path(downloaded_file.to_string_lossy())? {
            Some(
                Extension::Tar
                | Extension::TarBz
                | Extension::TarBz2
                | Extension::TarGz
                | Extension::TarXz
                | Extension::Tbz
                | Extension::Tgz
                | Extension::Txz,
            ) => self.extract_executable_from_tarball(downloaded_file),
            Some(Extension::Bz | Extension::Bz2) => self.unbzip(downloaded_file),
            Some(Extension::Gz) => self.ungzip(downloaded_file),
            Some(Extension::Xz) => self.unxz(downloaded_file),
            Some(Extension::Zip) => self.extract_executable_from_zip(downloaded_file),
            Some(Extension::Exe | Extension::Pyz) | None => self.copy_executable(downloaded_file),
        }
    }

    fn extract_executable_from_tarball(&self, downloaded_file: &Path) -> Result<()> {
        debug!(
            "extracting executable from tarball at {}",
            downloaded_file.to_string_lossy(),
        );

        let mut arch = tar_reader_for(downloaded_file)?;
        for entry in arch.entries()? {
            let mut entry = entry?;
            let path = entry.path()?;
            if !entry.header().entry_type().is_file() {
                continue;
            }
            debug!("found tarball entry with path {}", path.to_string_lossy());
            if let Some(os_name) = path.file_name() {
                if let Some(n) = os_name.to_str() {
                    if n == self.exe {
                        debug!(
                            "extracting tarball entry to {}",
                            self.install_path.to_string_lossy(),
                        );
                        self.create_install_dir()?;
                        entry.unpack(&self.install_path).unwrap();
                        return Ok(());
                    }
                }
            }
        }

        debug!("could not find any entries named {}", self.exe);
        Err(anyhow!(
            "could not find any files named {} in the downloaded tarball",
            self.exe,
        ))
    }

    fn extract_executable_from_zip(&self, downloaded_file: &Path) -> Result<()> {
        debug!("extracting executable from zip file");

        let mut zip = ZipArchive::new(open_file(downloaded_file)?)?;
        for i in 0..zip.len() {
            let mut zf = zip.by_index(i)?;
            let path = PathBuf::from(zf.name());
            if path.ends_with(&self.exe) {
                let mut buffer: Vec<u8> = Vec::with_capacity(usize::try_from(zf.size())?);
                zf.read_to_end(&mut buffer)?;
                self.create_install_dir()?;
                return File::create(&self.install_path)?
                    .write_all(&buffer)
                    .map_err(Into::into);
            }
        }

        debug!("could not find any entries named {}", self.exe);
        Err(anyhow!(
            "could not find any files named {} in the downloaded zip file",
            self.exe,
        ))
    }

    fn unbzip(&self, downloaded_file: &Path) -> Result<()> {
        debug!("uncompressing executable from bzip file");
        let reader = BzDecoder::new(open_file(downloaded_file)?);
        self.write_to_install_path(reader)
    }

    fn ungzip(&self, downloaded_file: &Path) -> Result<()> {
        debug!("uncompressing executable from gzip file");
        let reader = GzDecoder::new(open_file(downloaded_file)?);
        self.write_to_install_path(reader)
    }

    fn unxz(&self, downloaded_file: &Path) -> Result<()> {
        debug!("uncompressing executable from xz file");
        let reader = XzDecoder::new(open_file(downloaded_file)?);
        self.write_to_install_path(reader)
    }

    fn write_to_install_path(&self, mut reader: impl Read) -> Result<()> {
        self.create_install_dir()?;
        let mut writer = File::create(&self.install_path)
            .with_context(|| format!("Cannot write to {}", self.install_path.to_string_lossy()))?;
        std::io::copy(&mut reader, &mut writer)?;
        Ok(())
    }

    fn copy_executable(&self, exe_file: &Path) -> Result<()> {
        debug!("copying executable to final location");
        self.create_install_dir()?;
        std::fs::copy(exe_file, &self.install_path)?;

        Ok(())
    }

    fn create_install_dir(&self) -> Result<()> {
        let Some(path) = self.install_path.parent() else {
            return Err(anyhow!(
                "install path at {} has no parent",
                self.install_path.display()
            ));
        };

        debug!("creating directory at {}", path.display());
        create_dir_all(path)
            .with_context(|| format!("could not create a directory at {}", path.display()))
    }

    fn chmod_executable(&self) -> Result<()> {
        #[cfg(target_family = "windows")]
        return Ok(());

        #[cfg(target_family = "unix")]
        match set_permissions(&self.install_path, Permissions::from_mode(0o755)) {
            Ok(()) => Ok(()),
            Err(e) => Err(anyhow::Error::new(e)),
        }
    }
}

impl Installer for ExeInstaller {
    fn install(&self, download: &Download) -> Result<()> {
        self.extract_executable(&download.archive_path)?;
        self.chmod_executable()?;
        info!("Installed executable into {}", self.install_path.display());

        Ok(())
    }
}

impl ArchiveInstaller {
    pub(crate) fn new(install_path: PathBuf) -> Self {
        ArchiveInstaller {
            install_root: install_path,
        }
    }

    fn extract_entire_archive(&self, downloaded_file: &Path) -> Result<()> {
        match Extension::from_path(downloaded_file.to_string_lossy())? {
            Some(
                Extension::Tar
                | Extension::TarBz
                | Extension::TarBz2
                | Extension::TarGz
                | Extension::TarXz
                | Extension::Tbz
                | Extension::Tgz
                | Extension::Txz,
            ) => self.extract_entire_tarball(downloaded_file)?,
            Some(Extension::Zip) => self.extract_entire_zip(downloaded_file)?,
            _ => {
                return Err(anyhow!(
                    concat!(
                        "the downloaded release asset, {}, does not appear to be an",
                        " archive file so we cannopt extract all of its contents",
                    ),
                    downloaded_file.display(),
                ))
            }
        }

        if self.should_move_up_one_dir()? {
            Self::move_contents_up_one_dir(&self.install_root)?;
        } else {
            debug!("extracted archive did not contain a common top-level directory");
        }

        Ok(())
    }

    fn extract_entire_tarball(&self, downloaded_file: &Path) -> Result<()> {
        debug!(
            "extracting entire tarball at {}",
            downloaded_file.to_string_lossy(),
        );

        let mut arch = tar_reader_for(downloaded_file)?;

        arch.unpack(&self.install_root)?;

        Ok(())
    }

    // We do this because some projects use a top-level dir like `project-x86-64-Linux`, which is
    // pretty annoying to work with. In this case, it's a lot easier to install this into
    // `~/bin/project` so the directory tree ends up with the same structure on all platforms.
    fn should_move_up_one_dir(&self) -> Result<bool> {
        let mut prefixes: HashSet<OsString> = HashSet::new();
        for entry in fs::read_dir(&self.install_root).with_context(|| {
            format!(
                "could not read {} after unpacking the tarball into this directory",
                self.install_root.display(),
            )
        })? {
            let full_path = entry
                .context("could not get path for tarball entry")?
                .path();

            // If the entry is a file in the top-level of the install dir, then there's no common
            // directory prefix.
            if full_path.is_file()
                && full_path
                    .parent()
                    .expect("path of entry in install root somehow has no parent")
                    == self.install_root
            {
                return Ok(false);
            }

            let path = if let Ok(path) = full_path.strip_prefix(&self.install_root) {
                path
            } else {
                &full_path
            };

            if let Some(prefix) = path.components().next() {
                prefixes.insert(prefix.as_os_str().to_os_string());
            } else {
                return Err(anyhow!("directory entry has no path components"));
            }
        }

        // If all the entries
        Ok(prefixes.len() == 1)
    }

    fn move_contents_up_one_dir(path: &Path) -> Result<()> {
        let mut entries = fs::read_dir(path)?;
        let top_level_path = if let Some(dir_entry) = entries.next() {
            let dir_entry = dir_entry?;
            dir_entry.path()
        } else {
            return Err(anyhow!("no directory found in path"));
        };

        debug!(
            "moving extracted archive contents up one directory from {} to {}",
            top_level_path.display(),
            path.display(),
        );

        for entry in fs::read_dir(&top_level_path)? {
            let entry = entry?;
            let target = path.join(entry.file_name());
            fs::rename(entry.path(), target)?;
        }

        fs::remove_dir(top_level_path)?;

        Ok(())
    }

    fn extract_entire_zip(&self, downloaded_file: &Path) -> Result<()> {
        debug!(
            "extracting entire zip file at {}",
            downloaded_file.to_string_lossy(),
        );

        let mut zip = ZipArchive::new(open_file(downloaded_file)?)?;
        Ok(zip.extract(&self.install_root)?)
    }
}

impl Installer for ArchiveInstaller {
    fn install(&self, download: &Download) -> Result<()> {
        self.extract_entire_archive(&download.archive_path)?;
        info!(
            "Installed contents of archive file into {}",
            self.install_root.display()
        );

        Ok(())
    }
}

fn tar_reader_for(downloaded_file: &Path) -> Result<Archive<Box<dyn Read>>> {
    let file = open_file(downloaded_file)?;

    let ext = downloaded_file.extension();
    match ext {
        Some(ext) => match ext.to_str() {
            Some("tar") => Ok(Archive::new(Box::new(file))),
            Some("bz" | "tbz" | "bz2" | "tbz2") => Ok(Archive::new(Box::new(BzDecoder::new(file)))),
            Some("gz" | "tgz") => Ok(Archive::new(Box::new(GzDecoder::new(file)))),
            Some("xz" | "txz") => Ok(Archive::new(Box::new(XzDecoder::new(file)))),
            Some(e) => Err(anyhow!(
                "don't know how to uncompress a tarball with extension = {}",
                e,
            )),
            None => Err(anyhow!(
                "tarball {:?} has a non-UTF-8 extension",
                downloaded_file,
            )),
        },
        None => Ok(Archive::new(Box::new(file))),
    }
}

fn open_file(path: &Path) -> Result<File> {
    File::open(path).with_context(|| format!("Failed to open file at {}", path.to_string_lossy()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(target_family = "unix")]
    use std::os::unix::fs::PermissionsExt;
    use tempfile::tempdir;
    use test_case::test_case;
    use test_log::test;

    #[test_case("test-data/project.bz")]
    #[test_case("test-data/project.bz2")]
    #[test_case("test-data/project.exe")]
    #[test_case("test-data/project.gz")]
    #[test_case("test-data/project.pyz")]
    #[test_case("test-data/project.tar")]
    #[test_case("test-data/project.tar.bz")]
    #[test_case("test-data/project.tar.bz2")]
    #[test_case("test-data/project.tar.gz")]
    #[test_case("test-data/project.tar.xz")]
    #[test_case("test-data/project.xz")]
    #[test_case("test-data/project.zip")]
    #[test_case("test-data/project")]
    fn exe_installer(archive_path: &str) -> Result<()> {
        crate::test_case::init_logging();

        let exe = "project";

        let td = tempdir()?;
        let mut path_without_subdir = td.path().to_path_buf();
        path_without_subdir.push("project");
        let mut path_with_subdir = td.path().to_path_buf();
        path_with_subdir.extend(&["subdir", "project"]);

        for install_path in [path_without_subdir, path_with_subdir] {
            let installer = ExeInstaller::new(install_path.clone(), exe.to_string());
            installer.install(&Download {
                // It doesn't matter what we use here. We're not actually going to
                // put anything in this temp dir.
                _temp_dir: tempdir()?,
                archive_path: PathBuf::from(archive_path),
            })?;

            assert!(install_path.exists());
            assert!(install_path.is_file());
            #[cfg(target_family = "unix")]
            assert!(install_path.metadata()?.permissions().mode() & 0o111 != 0);
        }

        Ok(())
    }

    #[test_case("test-data/project.tar")]
    #[test_case("test-data/project.tar.bz")]
    #[test_case("test-data/project.tar.bz2")]
    #[test_case("test-data/project.tar.gz")]
    #[test_case("test-data/project.tar.xz")]
    #[test_case("test-data/project.zip")]
    fn archive_installer(archive_path: &str) -> Result<()> {
        crate::test_case::init_logging();

        let td = tempdir()?;
        let mut path_without_subdir = td.path().to_path_buf();
        path_without_subdir.push("project");
        let mut path_with_subdir = td.path().to_path_buf();
        path_with_subdir.extend(&["subdir", "project"]);

        for install_root in [path_without_subdir, path_with_subdir] {
            let installer = ArchiveInstaller::new(install_root.clone());
            installer.install(&Download {
                // It doesn't matter what we use here. We're not actually going to
                // put anything in this temp dir.
                _temp_dir: tempdir()?,
                archive_path: PathBuf::from(archive_path),
            })?;

            assert!(install_root.exists());
            assert!(install_root.is_dir());

            let bin_dir = install_root.join("bin");
            assert!(bin_dir.exists());
            assert!(bin_dir.is_dir());

            let exe = bin_dir.join("project");
            assert!(exe.exists());
            assert!(exe.is_file());
        }

        Ok(())
    }

    // This tests a bug in the initial implementation where a tarball that just contained files
    // caused us to try to move its contents up to a directory that didn't exist.
    #[test]
    fn archive_installer_one_file_in_archive_root() -> Result<()> {
        let td = tempdir()?;
        let mut path_without_subdir = td.path().to_path_buf();
        path_without_subdir.push("project");
        let mut path_with_subdir = td.path().to_path_buf();
        path_with_subdir.extend(&["subdir", "project"]);

        for install_root in [path_without_subdir, path_with_subdir] {
            let installer = ArchiveInstaller::new(install_root.clone());
            installer.install(&Download {
                // It doesn't matter what we use here. We're not actually going to
                // put anything in this temp dir.
                _temp_dir: tempdir()?,
                archive_path: PathBuf::from("test-data/project-with-one-file.tar.gz"),
            })?;

            assert!(install_root.exists());
            assert!(install_root.is_dir());

            let exe = install_root.join("project");
            assert!(exe.exists());
            assert!(exe.is_file());
        }

        Ok(())
    }

    #[test]
    fn archive_installer_no_root_path() -> Result<()> {
        let td = tempdir()?;
        let mut path_without_subdir = td.path().to_path_buf();
        path_without_subdir.push("project");
        let mut path_with_subdir = td.path().to_path_buf();
        path_with_subdir.extend(&["subdir", "project"]);

        for install_root in [path_without_subdir, path_with_subdir] {
            let installer = ArchiveInstaller::new(install_root.clone());
            installer.install(&Download {
                // It doesn't matter what we use here. We're not actually going to
                // put anything in this temp dir.
                _temp_dir: tempdir()?,
                archive_path: PathBuf::from("test-data/no-shared-root.tar.gz"),
            })?;

            assert!(install_root.exists());
            assert!(install_root.is_dir());

            let bin_dir = install_root.join("bin");
            assert!(bin_dir.exists());
            assert!(bin_dir.is_dir());

            let exe = bin_dir.join("project");
            assert!(exe.exists());
            assert!(exe.is_file());

            let readme = install_root.join("README.md");
            assert!(readme.exists());
            assert!(readme.is_file());
        }

        Ok(())
    }
}
