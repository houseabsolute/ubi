use crate::{extension::Extension, release::Download};
use anyhow::{anyhow, Context, Result};
use binstall_tar::Archive;
use bzip2::read::BzDecoder;
use flate2::read::GzDecoder;
use log::{debug, info};
use std::{
    fs::File,
    io::{Read, Write},
    path::{Path, PathBuf},
};
use xz2::read::XzDecoder;
use zip::ZipArchive;

#[cfg(target_family = "unix")]
use std::fs::{set_permissions, Permissions};
#[cfg(target_family = "unix")]
use std::os::unix::fs::PermissionsExt;

#[derive(Debug)]
pub(crate) struct Installer {
    install_path: PathBuf,
    exe: String,
}

impl Installer {
    pub(crate) fn new(install_path: PathBuf, exe: String) -> Self {
        Installer { install_path, exe }
    }

    pub(crate) fn install(&self, download: &Download) -> Result<()> {
        self.extract_binary(&download.archive_path)?;
        self.make_binary_executable()?;
        info!("Installed binary into {}", self.install_path.display());

        Ok(())
    }

    fn extract_binary(&self, downloaded_file: &Path) -> Result<()> {
        let filename = downloaded_file
            .file_name()
            .unwrap_or_else(|| {
                panic!(
                    "downloaded file path {} has no filename!",
                    downloaded_file.to_string_lossy()
                )
            })
            .to_string_lossy();
        match Extension::from_path(filename)? {
            Some(
                Extension::Tar
                | Extension::TarBz
                | Extension::TarBz2
                | Extension::TarGz
                | Extension::TarXz
                | Extension::Tbz
                | Extension::Tgz
                | Extension::Txz,
            ) => self.extract_tarball(downloaded_file),
            Some(Extension::Bz | Extension::Bz2) => self.unbzip(downloaded_file),
            Some(Extension::Gz) => self.ungzip(downloaded_file),
            Some(Extension::Xz) => self.unxz(downloaded_file),
            Some(Extension::Zip) => self.extract_zip(downloaded_file),
            Some(Extension::Exe) | None => self.copy_executable(downloaded_file),
        }
    }

    fn extract_zip(&self, downloaded_file: &Path) -> Result<()> {
        debug!("extracting binary from zip file");

        let mut zip = ZipArchive::new(open_file(downloaded_file)?)?;
        for i in 0..zip.len() {
            let mut zf = zip.by_index(i)?;
            let path = PathBuf::from(zf.name());
            if path.ends_with(&self.exe) {
                let mut buffer: Vec<u8> = Vec::with_capacity(usize::try_from(zf.size())?);
                zf.read_to_end(&mut buffer)?;
                let mut file = File::create(&self.install_path)?;
                return file.write_all(&buffer).map_err(Into::into);
            }
        }

        debug!("could not find any entries named {}", self.exe);
        Err(anyhow!(
            "could not find any files named {} in the downloaded zip file",
            self.exe,
        ))
    }

    fn extract_tarball(&self, downloaded_file: &Path) -> Result<()> {
        debug!(
            "extracting binary from tarball at {}",
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
                        return match entry.unpack(&self.install_path) {
                            Ok(_) => Ok(()),
                            Err(e) => Err(anyhow::Error::new(e)),
                        };
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

    fn unbzip(&self, downloaded_file: &Path) -> Result<()> {
        debug!("uncompressing binary from bzip file");
        let reader = BzDecoder::new(open_file(downloaded_file)?);
        self.write_to_install_path(reader)
    }

    fn ungzip(&self, downloaded_file: &Path) -> Result<()> {
        debug!("uncompressing binary from gzip file");
        let reader = GzDecoder::new(open_file(downloaded_file)?);
        self.write_to_install_path(reader)
    }

    fn unxz(&self, downloaded_file: &Path) -> Result<()> {
        debug!("uncompressing binary from xz file");
        let reader = XzDecoder::new(open_file(downloaded_file)?);
        self.write_to_install_path(reader)
    }

    fn write_to_install_path(&self, mut reader: impl Read) -> Result<()> {
        let mut writer = File::create(&self.install_path)
            .with_context(|| format!("Cannot write to {}", self.install_path.to_string_lossy()))?;
        std::io::copy(&mut reader, &mut writer)?;
        Ok(())
    }

    fn copy_executable(&self, exe_file: &Path) -> Result<()> {
        debug!("copying binary to final location");
        std::fs::copy(exe_file, &self.install_path)?;

        Ok(())
    }

    fn make_binary_executable(&self) -> Result<()> {
        #[cfg(target_family = "windows")]
        return Ok(());

        #[cfg(target_family = "unix")]
        match set_permissions(&self.install_path, Permissions::from_mode(0o755)) {
            Ok(()) => Ok(()),
            Err(e) => Err(anyhow::Error::new(e)),
        }
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

    #[test_case("test-data/project.bz", "project")]
    #[test_case("test-data/project.bz2", "project")]
    #[test_case("test-data/project.exe", "project")]
    #[test_case("test-data/project.gz", "project")]
    #[test_case("test-data/project.tar", "project")]
    #[test_case("test-data/project.tar.bz", "project")]
    #[test_case("test-data/project.tar.bz2", "project")]
    #[test_case("test-data/project.tar.gz", "project")]
    #[test_case("test-data/project.tar.xz", "project")]
    #[test_case("test-data/project.xz", "project")]
    #[test_case("test-data/project.zip", "project")]
    #[test_case("test-data/project", "project")]
    fn install(archive_path: &str, exe: &str) -> Result<()> {
        //crate::ubi::init_logger(log::LevelFilter::Debug)?;

        let td = tempdir()?;
        let mut install_path = td.path().to_path_buf();
        install_path.push("project");
        let installer = Installer::new(install_path.clone(), exe.to_string());
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

        Ok(())
    }
}
