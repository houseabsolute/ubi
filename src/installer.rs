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
use xz::read::XzDecoder;
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

    pub(crate) fn install(&self, download: Download) -> Result<()> {
        self.extract_binary(download.archive_path)?;
        self.make_binary_executable()?;
        info!("Installed binary into {}", self.install_path.display());

        Ok(())
    }

    fn extract_binary(&self, downloaded_file: PathBuf) -> Result<()> {
        let filename = downloaded_file
            .file_name()
            .unwrap_or_else(|| {
                panic!(
                    "downloaded file path {} has no filename!",
                    downloaded_file.to_string_lossy()
                )
            })
            .to_string_lossy();
        match Extension::from_path(filename) {
            Some(Extension::TarBz)
            | Some(Extension::TarGz)
            | Some(Extension::TarXz)
            | Some(Extension::Tbz)
            | Some(Extension::Tgz)
            | Some(Extension::Txz) => self.extract_tarball(downloaded_file),
            Some(Extension::Bz) => self.unbzip(downloaded_file),
            Some(Extension::Gz) => self.ungzip(downloaded_file),
            Some(Extension::Xz) => self.unxz(downloaded_file),
            Some(Extension::Zip) => self.extract_zip(downloaded_file),
            Some(Extension::Exe) | None => self.copy_executable(downloaded_file),
        }
    }

    fn extract_zip(&self, downloaded_file: PathBuf) -> Result<()> {
        debug!("extracting binary from zip file");

        let mut zip = ZipArchive::new(open_file(&downloaded_file)?)?;
        for i in 0..zip.len() {
            let mut zf = zip.by_index(i)?;
            let path = PathBuf::from(zf.name());
            if path.ends_with(&self.exe) {
                let mut buffer: Vec<u8> = Vec::with_capacity(zf.size() as usize);
                zf.read_to_end(&mut buffer)?;
                let mut file = File::create(&self.install_path)?;
                return file.write_all(&buffer).map_err(|e| e.into());
            }
        }

        debug!("could not find any entries named {}", self.exe);
        Err(anyhow!(
            "could not find any files named {} in the downloaded zip file",
            self.exe,
        ))
    }

    fn extract_tarball(&self, downloaded_file: PathBuf) -> Result<()> {
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

    fn unbzip(&self, downloaded_file: PathBuf) -> Result<()> {
        debug!("uncompressing binary from bzip file");
        let reader = BzDecoder::new(open_file(&downloaded_file)?);
        self.write_to_install_path(reader)
    }

    fn ungzip(&self, downloaded_file: PathBuf) -> Result<()> {
        debug!("uncompressing binary from gzip file");
        let reader = GzDecoder::new(open_file(&downloaded_file)?);
        self.write_to_install_path(reader)
    }

    fn unxz(&self, downloaded_file: PathBuf) -> Result<()> {
        debug!("uncompressing binary from xz file");
        let reader = XzDecoder::new(open_file(&downloaded_file)?);
        self.write_to_install_path(reader)
    }

    fn write_to_install_path(&self, mut reader: impl Read) -> Result<()> {
        let mut writer = File::create(&self.install_path)
            .with_context(|| format!("Cannot write to {}", self.install_path.to_string_lossy()))?;
        std::io::copy(&mut reader, &mut writer)?;
        Ok(())
    }

    fn copy_executable(&self, exe_file: PathBuf) -> Result<()> {
        debug!("copying binary to final location");
        std::fs::copy(exe_file, &self.install_path)?;

        Ok(())
    }

    fn make_binary_executable(&self) -> Result<()> {
        #[cfg(target_family = "windows")]
        return Ok(());

        #[cfg(target_family = "unix")]
        match set_permissions(&self.install_path, Permissions::from_mode(0o755)) {
            Ok(_) => Ok(()),
            Err(e) => Err(anyhow::Error::new(e)),
        }
    }
}

fn tar_reader_for(downloaded_file: PathBuf) -> Result<Archive<Box<dyn Read>>> {
    let file = open_file(&downloaded_file)?;

    let ext = downloaded_file.extension();
    match ext {
        Some(ext) => match ext.to_str() {
            Some("bz") | Some("tbz") => Ok(Archive::new(Box::new(BzDecoder::new(file)))),
            Some("gz") | Some("tgz") => Ok(Archive::new(Box::new(GzDecoder::new(file)))),
            Some("xz") | Some("txz") => Ok(Archive::new(Box::new(XzDecoder::new(file)))),
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
    use tempfile::tempdir;

    #[test]
    fn extract_binary() -> Result<()> {
        //crate::ubi::init_logger(log::LevelFilter::Debug)?;

        let td = tempdir()?;
        let mut install_path = td.path().to_path_buf();
        install_path.push("project");
        let installer = Installer::new(install_path, "project".to_string());
        installer.extract_binary(PathBuf::from("test-data/project.tar.gz"))?;

        let mut extracted_path = td.path().to_path_buf();
        extracted_path.push("project");
        assert!(extracted_path.exists());
        assert!(extracted_path.is_file());

        Ok(())
    }
}
