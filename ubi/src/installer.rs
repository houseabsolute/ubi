use crate::{extension::Extension, ubi::Download};
use anyhow::{anyhow, Context, Result};
use binstall_tar::Archive;
use bzip2::read::BzDecoder;
use flate2::read::GzDecoder;
use log::{debug, info};
use std::{
    ffi::OsStr,
    fs::{create_dir_all, File},
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
    install_dir: PathBuf,
    exe: String,
}

impl Installer {
    pub(crate) fn new(install_dir: PathBuf, exe: String) -> Self {
        Installer { install_dir, exe }
    }

    pub(crate) fn install(&self, download: &Download) -> Result<()> {
        let install_path = self.extract_binary(&download.archive_path)?;
        Self::make_binary_executable(&install_path)?;
        info!("Installed binary into {}", install_path.display());

        Ok(())
    }

    fn extract_binary(&self, downloaded_file: &Path) -> Result<PathBuf> {
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

    fn extract_zip(&self, downloaded_file: &Path) -> Result<PathBuf> {
        debug!("extracting binary from zip file");

        let mut zip = ZipArchive::new(open_file(downloaded_file)?)?;
        for i in 0..zip.len() {
            let mut zf = zip.by_index(i)?;
            let path = PathBuf::from(zf.name());
            if let Some(file_name) = path.file_name() {
                if file_name.to_string_lossy().to_lowercase() == self.exe.to_lowercase() {
                    create_dir_all(&self.install_dir)?;
                    let install_path = self.install_path_for_exe(Some(file_name));
                    debug!(
                        "extracting zip entry to {}",
                        self.install_dir.to_string_lossy(),
                    );
                    let mut buffer: Vec<u8> = Vec::with_capacity(usize::try_from(zf.size())?);
                    zf.read_to_end(&mut buffer)?;
                    let mut file = File::create(&install_path)?;
                    file.write_all(&buffer)?;
                    return Ok(install_path);
                }
            }
        }

        debug!("could not find any entries named {}", self.exe);
        Err(anyhow!(
            "could not find any files named {} in the downloaded zip file",
            self.exe,
        ))
    }

    fn extract_tarball(&self, downloaded_file: &Path) -> Result<PathBuf> {
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
            if let Some(file_name) = path.file_name() {
                if file_name.to_string_lossy().to_lowercase() == self.exe.to_lowercase() {
                    create_dir_all(&self.install_dir)?;
                    let install_path = self.install_path_for_exe(Some(file_name));
                    debug!("extracting tarball entry to {}", install_path.display());
                    entry.unpack(&install_path)?;
                    return Ok(install_path);
                }
            }
        }

        debug!("could not find any entries named {}", self.exe);
        Err(anyhow!(
            "could not find any files named {} in the downloaded tarball",
            self.exe,
        ))
    }

    fn unbzip(&self, downloaded_file: &Path) -> Result<PathBuf> {
        debug!("uncompressing binary from bzip file");
        let reader = BzDecoder::new(open_file(downloaded_file)?);
        self.write_to_install_path(reader)
    }

    fn ungzip(&self, downloaded_file: &Path) -> Result<PathBuf> {
        debug!("uncompressing binary from gzip file");
        let reader = GzDecoder::new(open_file(downloaded_file)?);
        self.write_to_install_path(reader)
    }

    fn unxz(&self, downloaded_file: &Path) -> Result<PathBuf> {
        debug!("uncompressing binary from xz file");
        let reader = XzDecoder::new(open_file(downloaded_file)?);
        self.write_to_install_path(reader)
    }

    fn write_to_install_path(&self, mut reader: impl Read) -> Result<PathBuf> {
        create_dir_all(&self.install_dir)?;
        let install_path = self.install_path_for_exe(None);

        debug!(
            "writing binary to final location at {}",
            install_path.display(),
        );
        let mut writer = File::create(&install_path)
            .with_context(|| format!("Cannot write to {}", install_path.display()))?;
        std::io::copy(&mut reader, &mut writer)?;

        Ok(install_path)
    }

    fn copy_executable(&self, exe_file: &Path) -> Result<PathBuf> {
        debug!(
            "copying binary from {} to final location at {}",
            exe_file.display(),
            self.install_dir.display()
        );
        create_dir_all(&self.install_dir)?;
        let install_path = self.install_path_for_exe(Some(exe_file.file_name().unwrap()));
        std::fs::copy(exe_file, &install_path)?;

        Ok(install_path)
    }

    fn install_path_for_exe(&self, exe_file: Option<&OsStr>) -> PathBuf {
        let mut path = self.install_dir.clone();
        if let Some(exe) = exe_file {
            path.push(exe);
        } else {
            path.push(&self.exe);
        }
        path
    }

    fn make_binary_executable(install_path: &Path) -> Result<()> {
        #[cfg(target_family = "windows")]
        return Ok(());

        #[cfg(target_family = "unix")]
        match set_permissions(install_path, Permissions::from_mode(0o755)) {
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
    use serial_test::serial;
    #[cfg(target_family = "unix")]
    use std::os::unix::fs::PermissionsExt;
    use std::{env, sync::Once};
    use tempfile::tempdir;
    use test_case::test_case;

    static INIT_LOGGING: Once = Once::new();

    #[test_case("test-data/lc/project.bz"; "bz")]
    #[test_case("test-data/lc/project.bz2"; "bz2")]
    #[test_case("test-data/lc/project.exe"; "exe")]
    #[test_case("test-data/lc/project.gz"; "gz")]
    #[test_case("test-data/lc/project.tar"; "tar")]
    #[test_case("test-data/lc/project.tar.bz"; "tar.bz")]
    #[test_case("test-data/lc/project.tar.bz2"; "tar.bz2")]
    #[test_case("test-data/lc/project.tar.gz"; "tar.gz")]
    #[test_case("test-data/lc/project.tar.xz"; "tar.xz")]
    #[test_case("test-data/lc/project.xz"; "xz")]
    #[test_case("test-data/lc/project.zip"; "zip")]
    #[test_case("test-data/lc/project"; "no extension")]
    #[test_case("test-data/uc/Project.bz"; "bz uppercase")]
    #[test_case("test-data/uc/Project.bz2"; "bz2 uppercase")]
    #[test_case("test-data/uc/Project.exe"; "exe uppercase")]
    #[test_case("test-data/uc/Project.gz"; "gz uppercase")]
    #[test_case("test-data/uc/Project.tar"; "tar uppercase")]
    #[test_case("test-data/uc/Project.tar.bz"; "tar.bz uppercase")]
    #[test_case("test-data/uc/Project.tar.bz2"; "tar.bz2 uppercase")]
    #[test_case("test-data/uc/Project.tar.gz"; "tar.gz uppercase")]
    #[test_case("test-data/uc/Project.tar.xz"; "tar.xz uppercase")]
    #[test_case("test-data/uc/Project.xz"; "xz uppercase")]
    #[test_case("test-data/uc/Project.zip"; "zip uppercase")]
    #[test_case("test-data/uc/Project"; "no extension uppercase")]
    #[serial]
    fn install(archive_path: &str) -> Result<()> {
        INIT_LOGGING.call_once(|| {
            if matches!(env::var("RUST_LOG"), Ok(v) if !v.is_empty()) {
                crate::init_logger(log::LevelFilter::Debug).expect("failed to initialize logging");
            }
        });

        for exe in ["project", "Project"] {
            let td = tempdir()?;
            let install_dir = td.path().to_path_buf();

            let installer = Installer::new(install_dir.clone(), exe.to_string());
            installer.install(&Download {
                // It doesn't matter what we use here. We're not actually going to
                // put anything in this temp dir.
                _temp_dir: tempdir()?,
                archive_path: PathBuf::from(archive_path),
            })?;

            let mut install_path = install_dir.clone();
            install_path.push(exe);

            assert!(install_path.exists());
            assert!(install_path.is_file());
            #[cfg(target_family = "unix")]
            assert!(install_path.metadata()?.permissions().mode() & 0o111 != 0);
        }

        Ok(())
    }
}
