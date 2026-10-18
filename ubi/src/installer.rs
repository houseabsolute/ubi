use crate::{
    archive::{ArchiveEntry, SevenZipEntriesIterator, TarEntriesIterator, ZipEntriesIterator},
    extension::Extension,
    ubi::Download,
};
use anyhow::{anyhow, Context, Result};
use binstall_tar::Archive as TarArchive;
use bzip2::read::BzDecoder;
use flate2::read::GzDecoder;
use log::{debug, info};
use std::{
    collections::HashMap,
    ffi::OsString,
    fmt::Debug,
    fs::{self, create_dir_all, File},
    io::{Read, Write},
    path::{Path, PathBuf},
};
use strum::IntoEnumIterator;
use tempfile::{tempdir, TempDir};
use walkdir::WalkDir;
use xz2::read::XzDecoder;
use zip::ZipArchive;
use zstd::stream::read::Decoder as ZstdDecoder;

#[cfg(target_family = "unix")]
use std::fs::{set_permissions, Permissions};
#[cfg(target_family = "unix")]
use std::os::unix::fs::PermissionsExt;

pub(crate) trait Installer: Debug {
    /// Checks whatever we can know about the install path before downloading anything, so that we
    /// fail before spending time on a download that we already know we cannot install.
    fn check_install_path(&self) -> Result<()>;

    fn install(&self, download: &Download) -> Result<()>;
}

#[derive(Debug)]
pub(crate) struct ExeInstaller {
    install_path: PathBuf,
    install_path_is_from_rename_exe_to: bool,
    exe_file_stem_is_from_exe_flag: bool,
    exe_file_stem: String,
    is_windows: bool,
    extensions: Vec<&'static str>,
}

#[derive(Debug)]
pub(crate) struct ArchiveInstaller {
    project_name: String,
    install_root: PathBuf,
}

/// The install directory is one we create files under. If there's already a file at that path,
/// creating it fails with a bare OS error - `File exists` when we create the directory, `Not a
/// directory` when we write under it - and neither says which path is the problem or what to do
/// about it. The directory always comes from `--in`, even when `--rename-exe` named the last
/// component of the install path, so that's the only flag worth naming.
fn check_install_dir_is_not_a_file(install_dir: &Path) -> Result<()> {
    if install_dir.is_dir() {
        return Ok(());
    }

    // Both `is_dir` and `exists` follow symlinks, so a symlink to a directory is not in the way.
    // A symlink to nothing looks like an empty path to them, but it is still in the way, and it
    // produces the same bare `File exists` as a regular file does.
    let what = if install_dir.exists() {
        "a file"
    } else if install_dir.symlink_metadata().is_ok() {
        "a symlink that doesn't point to anything"
    } else {
        return Ok(());
    };

    Err(anyhow!(
        "the install directory at {} is {what}, not a directory - remove it or pass a different `--in` value",
        install_dir.display(),
    ))
}

impl ExeInstaller {
    pub(crate) fn new(
        install_path: PathBuf,
        install_path_is_from_rename_exe_to: bool,
        exe_file_stem_is_from_exe_flag: bool,
        exe: String,
        is_windows: bool,
    ) -> Self {
        let extensions = if is_windows {
            Extension::iter()
                .filter(super::extension::Extension::is_windows_only)
                .map(|e| e.extension())
                .collect()
        } else {
            vec![]
        };

        ExeInstaller {
            install_path,
            install_path_is_from_rename_exe_to,
            exe_file_stem_is_from_exe_flag,
            exe_file_stem: exe,
            is_windows,
            extensions,
        }
    }

    fn extract_executable(&self, downloaded_file: &Path) -> Result<PathBuf> {
        match Extension::from_path(downloaded_file)? {
            Some(
                Extension::Tar
                | Extension::TarBz
                | Extension::TarBz2
                | Extension::TarGz
                | Extension::TarXz
                | Extension::TarZst
                | Extension::Tbz
                | Extension::Tgz
                | Extension::Txz
                | Extension::Tzst,
            ) => self.extract_executable_from_tarball(downloaded_file),
            Some(Extension::Bz | Extension::Bz2) => self.unbzip(downloaded_file),
            Some(Extension::Gz) => self.ungzip(downloaded_file),
            Some(Extension::Xz) => self.unxz(downloaded_file),
            Some(Extension::Zst) => self.unzstd(downloaded_file),
            Some(Extension::SevenZip) => self.extract_executable_from_7z(downloaded_file),
            Some(Extension::Zip) => self.extract_executable_from_zip(downloaded_file),
            Some(
                Extension::AppImage
                | Extension::Bat
                | Extension::Exe
                | Extension::Jar
                | Extension::Phar
                | Extension::Py
                | Extension::Pyz
                | Extension::Sh,
            )
            | None => self.copy_executable(downloaded_file),
        }
    }

    fn extract_executable_from_tarball(&self, downloaded_file: &Path) -> Result<PathBuf> {
        debug!(
            "extracting executable from tarball at {}",
            downloaded_file.display(),
        );

        // Iterating through the archive both here and in `best_match_from_archive` is really
        // gross. But this is necessary because the underlying `Entry` structs returned by
        // `arch.entries` are only valid for the duration of the loop iteration. That's because they
        // rely on the position of the underlying file handle. It'd be nice to just be able to seek
        // that handle back to the start of the file, but the readers provided by various decoders,
        // like `BzDecoder`, do not implement the `Seek` trait.
        //
        // So the only viable solution is find the entry, then _re-open_ the file and go through the
        // entries again until we find the one we want.

        let mut arch = tar_reader_for(downloaded_file)?;
        let entries = arch.entries().with_context(|| {
            format!(
                "failed to get entries from tarball at {}",
                downloaded_file.display()
            )
        })?;
        if let Some(idx) =
            self.best_match_from_archive(TarEntriesIterator::new(entries), "tarball")?
        {
            let mut arch2 = tar_reader_for(downloaded_file)?;
            for (i, entry) in arch2
                .entries()
                .with_context(|| {
                    format!(
                        "failed to get entries from tarball at {}",
                        downloaded_file.display()
                    )
                })?
                .enumerate()
            {
                let mut entry = entry.with_context(|| {
                    format!(
                        "failed to read tarball entry at index {i} from {}",
                        downloaded_file.display()
                    )
                })?;
                if i != idx {
                    continue;
                }

                let entry_path = entry
                    .path()
                    .with_context(|| {
                        format!(
                            "failed to get path from tarball entry at index {i} in {}",
                            downloaded_file.display()
                        )
                    })?
                    .into_owned();
                let install_path = self.maybe_munged_install_path(&entry_path)?;
                debug!(
                    "extracting tarball entry named {} to {}",
                    entry_path.display(),
                    install_path.display(),
                );
                self.create_install_dir().with_context(|| {
                    format!(
                        "failed to create installation directory for {}",
                        install_path.display()
                    )
                })?;
                self.check_install_path_is_not_a_dir(&install_path)?;
                entry.unpack(&install_path).with_context(|| {
                    format!(
                        "failed to extract executable named {} into {}",
                        entry_path.display(),
                        install_path.display()
                    )
                })?;

                return Ok(install_path);
            }
        }

        self.could_not_find_archive_matches_error()
    }

    fn extract_executable_from_7z(&self, downloaded_file: &Path) -> Result<PathBuf> {
        debug!(
            "extracting executable from 7z file at {}",
            downloaded_file.display()
        );

        let best_match = self.best_match_from_archive(
            SevenZipEntriesIterator::new(
                sevenz_rust2::ArchiveReader::new(
                    open_file(downloaded_file)?,
                    sevenz_rust2::Password::empty(),
                )
                .with_context(|| {
                    format!(
                        "failed to create 7z archive reader for {}",
                        downloaded_file.display()
                    )
                })?,
            ),
            "sevenzip",
        )?;

        if let Some(idx) = best_match {
            let mut archive = sevenz_rust2::ArchiveReader::new(
                open_file(downloaded_file)?,
                sevenz_rust2::Password::empty(),
            )
            .with_context(|| {
                format!(
                    "failed to create 7z archive reader for {}",
                    downloaded_file.display()
                )
            })?;

            let entry = archive.archive().files[idx].clone();
            let path = entry.path().with_context(|| {
                format!(
                    "failed to get path from 7z entry at index {idx} in {}",
                    downloaded_file.display()
                )
            })?;
            let install_path = self.maybe_munged_install_path(&path)?;
            debug!(
                "extracting 7z entry named {} to {}",
                path.display(),
                install_path.display(),
            );
            let buffer = archive.read_file(entry.name()).with_context(|| {
                format!(
                    "failed to read 7z entry named {} from {}",
                    entry.name(),
                    downloaded_file.display()
                )
            })?;
            self.create_install_dir().with_context(|| {
                format!(
                    "failed to create installation directory for {}",
                    install_path.display()
                )
            })?;

            self.check_install_path_is_not_a_dir(&install_path)?;
            File::create(&install_path)
                .with_context(|| format!("failed to create file at {}", install_path.display()))?
                .write_all(&buffer)
                .with_context(|| {
                    format!(
                        "failed to write extracted content to {}",
                        install_path.display()
                    )
                })?;

            return Ok(install_path);
        }

        self.could_not_find_archive_matches_error()
    }

    fn extract_executable_from_zip(&self, downloaded_file: &Path) -> Result<PathBuf> {
        debug!(
            "extracting executable from zip file at {}",
            downloaded_file.display()
        );

        let mut zip = ZipArchive::new(open_file(downloaded_file)?).with_context(|| {
            format!(
                "failed to create zip archive reader for {}",
                downloaded_file.display()
            )
        })?;
        if let Some(idx) = self.best_match_from_archive(ZipEntriesIterator::new(&mut zip), "zip")? {
            let mut zf = zip.by_index(idx).with_context(|| {
                format!(
                    "failed to get zip entry at index {idx} from {}",
                    downloaded_file.display()
                )
            })?;

            let zf_name = zf.name()?.to_string();
            let zf_path = Path::new(&zf_name);
            let install_path = self.maybe_munged_install_path(zf_path)?;
            debug!(
                "extracting zip file entry named {} to {}",
                zf_name,
                install_path.display(),
            );
            let mut buffer: Vec<u8> =
                Vec::with_capacity(usize::try_from(zf.size()).with_context(|| {
                    format!(
                        "failed to convert zip file size to usize for entry {} in {}",
                        zf_name,
                        downloaded_file.display()
                    )
                })?);
            zf.read_to_end(&mut buffer).with_context(|| {
                format!(
                    "failed to read zip entry {} from {}",
                    zf_name,
                    downloaded_file.display()
                )
            })?;
            self.create_install_dir().with_context(|| {
                format!(
                    "failed to create installation directory for {}",
                    install_path.display()
                )
            })?;

            self.check_install_path_is_not_a_dir(&install_path)?;
            File::create(&install_path)
                .with_context(|| format!("failed to create file at {}", install_path.display()))?
                .write_all(&buffer)
                .with_context(|| {
                    format!(
                        "failed to write extracted content to {}",
                        install_path.display()
                    )
                })?;

            return Ok(install_path);
        }

        self.could_not_find_archive_matches_error()
    }

    fn best_match_from_archive<'a>(
        &self,
        archive: impl Iterator<Item = Result<Box<dyn ArchiveEntry + 'a>>>,
        archive_type: &'static str,
    ) -> Result<Option<usize>> {
        let mut possible_matches: Vec<usize> = vec![];

        for (i, entry) in archive.enumerate() {
            let entry = entry
                .with_context(|| format!("failed to read {archive_type} entry at index {i}"))?;
            if !entry.is_file() {
                continue;
            }

            let path = entry.path().with_context(|| {
                format!("failed to get path from {archive_type} entry at index {i}")
            })?;

            debug!("found {archive_type} entry with path `{}`", path.display());
            if let Some(file_name) = path.file_name() {
                if let Some(file_name) = file_name.to_str() {
                    if self.archive_member_is_exact_match(file_name) {
                        debug!("found {archive_type} file entry with exact match: `{file_name}`");
                        return Ok(Some(i));
                    } else if self.archive_member_is_partial_match(file_name) {
                        // On Windows, we assume that the file is executable if it matches the
                        // expected name, because Windows doesn't have executable bits. We treat
                        // "None" as true because some archive types don't record whether a file is
                        // executable.
                        if self.is_windows
                            || matches!(
                                entry.is_executable().with_context(|| {
                                    format!(
                                        "failed to check if {archive_type} entry at index {i} is executable"
                                    )
                                })?,
                                None | Some(true)
                            )
                        {
                            debug!(
                                "found {archive_type} file entry with partial match: `{file_name}`"
                            );
                            possible_matches.push(i);
                        }
                    }
                }
            }
        }

        Ok(possible_matches.into_iter().next())
    }

    fn archive_member_is_exact_match(&self, file_name: &str) -> bool {
        if self.extensions.is_empty() {
            return file_name == self.exe_file_stem;
        }

        self.extensions
            .iter()
            .map(|&ext| format!("{}{}", self.exe_file_stem.to_lowercase(), ext))
            .any(|n| n == file_name)
    }

    fn archive_member_is_partial_match(&self, file_name: &str) -> bool {
        if !file_name.starts_with(&self.exe_file_stem) {
            return false;
        }
        if self.extensions.is_empty() {
            return true;
        }
        self.extensions
            .iter()
            .any(|&ext| file_name.to_lowercase().ends_with(ext))
    }

    fn could_not_find_archive_matches_error(&self) -> Result<PathBuf> {
        let expect_names = if self.extensions.is_empty() {
            format!("{}*", self.exe_file_stem)
        } else {
            self.extensions
                .iter()
                .map(|ext| format!("{}*{}", self.exe_file_stem, ext))
                .collect::<Vec<_>>()
                .join(" ")
        };

        debug!("could not find any entries matching [{expect_names}]");
        Err(anyhow!(
            "could not find any files matching [{expect_names}] in the downloaded archive file",
        ))
    }

    fn unbzip(&self, downloaded_file: &Path) -> Result<PathBuf> {
        debug!("uncompressing executable from bzip file");
        let reader = BzDecoder::new(open_file(downloaded_file)?);
        self.write_to_install_path(reader)
    }

    fn ungzip(&self, downloaded_file: &Path) -> Result<PathBuf> {
        debug!("uncompressing executable from gzip file");
        let reader = GzDecoder::new(open_file(downloaded_file)?);
        self.write_to_install_path(reader)
    }

    fn unxz(&self, downloaded_file: &Path) -> Result<PathBuf> {
        debug!("uncompressing executable from xz file");
        let reader = XzDecoder::new(open_file(downloaded_file)?);
        self.write_to_install_path(reader)
    }

    fn unzstd(&self, downloaded_file: &Path) -> Result<PathBuf> {
        debug!("uncompressing executable from zstd file");
        let reader = ZstdDecoder::new(open_file(downloaded_file)?)?;
        self.write_to_install_path(reader)
    }

    fn write_to_install_path(&self, mut reader: impl Read) -> Result<PathBuf> {
        self.create_install_dir().with_context(|| {
            format!(
                "failed to create installation directory for {}",
                self.install_path.display()
            )
        })?;
        let install_path = if self.install_path_is_from_rename_exe_to {
            self.install_path.clone()
        } else {
            self.maybe_add_windows_exe_extension(self.install_path.clone())
        };
        self.check_install_path_is_not_a_dir(&install_path)?;
        let mut writer = File::create(&install_path)
            .with_context(|| format!("Cannot write to {}", install_path.display()))?;
        std::io::copy(&mut reader, &mut writer)
            .with_context(|| format!("failed to copy content to {}", install_path.display()))?;
        Ok(install_path)
    }

    fn copy_executable(&self, exe_file: &Path) -> Result<PathBuf> {
        debug!("copying executable to final location");
        self.create_install_dir().with_context(|| {
            format!(
                "failed to create installation directory for {}",
                self.install_path.display()
            )
        })?;

        let install_path = self.maybe_munged_install_path(exe_file)?;
        self.check_install_path_is_not_a_dir(&install_path)?;
        std::fs::copy(exe_file, &install_path).with_context(|| {
            format!(
                "error copying file from {} to {}",
                exe_file.display(),
                install_path.display()
            )
        })?;

        Ok(install_path)
    }

    fn create_install_dir(&self) -> Result<()> {
        let Some(path) = self.install_path.parent() else {
            return Err(anyhow!(
                "install path at {} has no parent",
                self.install_path.display()
            ));
        };

        check_install_dir_is_not_a_file(path)?;

        debug!("creating directory at {}", path.display());
        create_dir_all(path)
            .with_context(|| format!("could not create a directory at {}", path.display()))
    }

    /// Installing to a path that already exists as a directory fails deep inside the standard
    /// library, where the only thing we can report is a bare `Is a directory (os error 21)`.
    /// Checking for this case lets us tell the user what actually went wrong and what to do about
    /// it. We name only the flag that produced the path, so we don't suggest changing one that has
    /// nothing to do with the conflict.
    fn check_install_path_is_not_a_dir(&self, install_path: &Path) -> Result<()> {
        if !install_path.is_dir() {
            return Ok(());
        }

        // The install path is the `--in` directory plus a last component, so `--in` is always
        // worth changing. The last component comes from `--rename-exe` if that was passed, and
        // otherwise from `--exe` if that was passed. When it comes from neither, it's the project
        // name, and there's no flag to change.
        let flags = if self.install_path_is_from_rename_exe_to {
            "`--in` or `--rename-exe`"
        } else if self.exe_file_stem_is_from_exe_flag {
            "`--in` or `--exe`"
        } else {
            "`--in`"
        };
        Err(anyhow!(
            "cannot install to {} because there is already a directory at that path - remove it or pass a different {flags} value",
            install_path.display(),
        ))
    }

    fn maybe_munged_install_path<P: AsRef<Path>>(&self, path: P) -> Result<PathBuf> {
        let mut install_path = self.install_path.clone();

        // If the user specifies an explicit path with `--rename-exe`, then we want to use that
        // exactly as given.
        if self.install_path_is_from_rename_exe_to {
            debug!("install path was explicitly set, not munging the install path");
            return Ok(install_path);
        }

        if let Some(ext) = Extension::from_path(path.as_ref())? {
            if ext.should_preserve_extension_on_install() {
                debug!("preserving the {} extension on install", ext.extension());
                install_path.set_extension(ext.extension_without_dot());
                return Ok(install_path);
            }
        }

        Ok(self.maybe_add_windows_exe_extension(install_path))
    }

    // Windows only runs a file as a program if it has an extension like `.exe`. When the file we
    // install has no extension of its own to keep, like a gzipped binary, we add `.exe` so the
    // installed file can be run. We append it instead of using `set_extension` so that a name with
    // a dot, like `tool.v2`, keeps all of its parts.
    fn maybe_add_windows_exe_extension(&self, install_path: PathBuf) -> PathBuf {
        if !self.is_windows {
            return install_path;
        }
        if install_path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("exe") || ext.eq_ignore_ascii_case("bat"))
        {
            return install_path;
        }

        debug!("adding the .exe extension to the install path for Windows");
        let mut name = install_path.clone().into_os_string();
        name.push(".exe");
        PathBuf::from(name)
    }

    #[cfg(target_family = "windows")]
    fn chmod_executable(_exe: &Path) -> Result<()> {
        Ok(())
    }

    #[cfg(target_family = "unix")]
    fn chmod_executable(exe: &Path) -> Result<()> {
        match set_permissions(exe, Permissions::from_mode(0o755)) {
            Ok(()) => Ok(()),
            Err(e) => Err(anyhow::Error::new(e)),
        }
    }
}

impl Installer for ExeInstaller {
    fn check_install_path(&self) -> Result<()> {
        // The install directory is fixed no matter what the downloaded file turns out to be, so a
        // file in the way there can always be reported before we download anything.
        if let Some(parent) = self.install_path.parent() {
            check_install_dir_is_not_a_file(parent)?;
        }

        // We only know the final install path up front when the user named it exactly. Otherwise
        // the downloaded file may have an extension that we preserve, which we cannot know until
        // we have seen the file, and the install path grows that extension. On Windows we may also
        // add `.exe`. Checking the path
        // without the extension here would reject an install that was going to write to the path
        // with the extension and succeed. Those cases are caught by the check at each install site
        // instead, just not as early.
        if !self.install_path_is_from_rename_exe_to {
            return Ok(());
        }

        self.check_install_path_is_not_a_dir(&self.install_path)
    }

    fn install(&self, download: &Download) -> Result<()> {
        let real_exe = self.extract_executable(&download.archive_path)?;
        Self::chmod_executable(&real_exe).with_context(|| {
            format!(
                "failed to set executable permissions on {}",
                real_exe.display()
            )
        })?;
        info!("Installed executable into {}", real_exe.display());

        Ok(())
    }
}

impl ArchiveInstaller {
    pub(crate) fn new(project_name: String, install_path: PathBuf) -> Self {
        ArchiveInstaller {
            project_name,
            install_root: install_path,
        }
    }

    fn extract_entire_archive(&self, downloaded_file: &Path) -> Result<()> {
        let td = tempdir().with_context(|| {
            format!(
                "failed to create temporary directory for extracting {}",
                downloaded_file.display()
            )
        })?;

        match Extension::from_path(downloaded_file)? {
            Some(
                Extension::Tar
                | Extension::TarBz
                | Extension::TarBz2
                | Extension::TarGz
                | Extension::TarXz
                | Extension::TarZst
                | Extension::Tbz
                | Extension::Tgz
                | Extension::Txz
                | Extension::Tzst,
            ) => Self::extract_entire_tarball(downloaded_file, td.path()).with_context(|| {
                format!("failed to extract tarball at {}", downloaded_file.display())
            })?,
            Some(Extension::SevenZip) => {
                Self::extract_entire_7z(downloaded_file, td.path()).with_context(|| {
                    format!(
                        "failed to extract 7z archive at {}",
                        downloaded_file.display()
                    )
                })?;
            }
            Some(Extension::Zip) => Self::extract_entire_zip(downloaded_file, td.path())
                .with_context(|| {
                    format!(
                        "failed to extract zip archive at {}",
                        downloaded_file.display()
                    )
                })?,
            _ => {
                return Err(anyhow!(
                    concat!(
                        "the downloaded release asset, {}, does not appear to be an",
                        " archive file so we cannot extract all of its contents",
                    ),
                    downloaded_file.display(),
                ))
            }
        }

        self.copy_extracted_contents(&td)?;

        Ok(())
    }

    fn extract_entire_tarball(downloaded_file: &Path, into: &Path) -> Result<()> {
        debug!(
            "extracting entire tarball at {} to {}",
            downloaded_file.display(),
            into.display()
        );

        let mut arch = tar_reader_for(downloaded_file)?;
        arch.unpack(into)
            .with_context(|| format!("failed to unpack tarball to {}", into.display()))?;

        Ok(())
    }

    fn extract_entire_7z(downloaded_file: &Path, into: &Path) -> Result<()> {
        debug!(
            "extracting entire 7z file at {} to {}",
            downloaded_file.display(),
            into.display()
        );

        sevenz_rust2::decompress_file(downloaded_file, into)
            .with_context(|| format!("failed to decompress 7z file to {}", into.display()))?;
        Ok(())
    }

    fn extract_entire_zip(downloaded_file: &Path, into: &Path) -> Result<()> {
        debug!(
            "extracting entire zip file at {} to {}",
            downloaded_file.display(),
            into.display(),
        );

        let mut zip = ZipArchive::new(open_file(downloaded_file)?).with_context(|| {
            format!(
                "failed to create zip archive reader for {}",
                downloaded_file.display()
            )
        })?;
        zip.extract(into).with_context(|| {
            format!(
                "failed to extract zip archive from {} to {}",
                downloaded_file.display(),
                into.display()
            )
        })?;
        Ok(())
    }

    fn copy_extracted_contents(&self, td: &TempDir) -> Result<()> {
        let copy_from = match self.extracted_contents_top_level_dir(td.path())? {
            Some(dir) => dir,
            None => td.path().to_path_buf(),
        };
        check_extracted_symlinks(&copy_from)?;

        debug!(
            "copying extracted contents from {} to {}",
            copy_from.display(),
            self.install_root.display(),
        );

        for entry in WalkDir::new(&copy_from) {
            let entry = entry.with_context(|| {
                format!(
                    "failed to read the extracted contents in {}",
                    copy_from.display()
                )
            })?;
            let full_path = entry.path();
            let rel_path = full_path.strip_prefix(&copy_from).with_context(|| {
                format!(
                    "failed to strip prefix {} from path {}",
                    copy_from.display(),
                    full_path.display()
                )
            })?;
            let target_path = self.install_root.join(rel_path);

            // We look at the entry's own type, not the type of what a symlink points to. Following
            // links would turn a link to a directory into an empty directory, a link to a file into
            // a copy, and a dangling link into an error.
            let file_type = entry.file_type();
            check_for_symlinks_in_install_path(
                &self.install_root,
                rel_path,
                file_type.is_symlink(),
            )?;
            if file_type.is_symlink() {
                copy_symlink(full_path, &target_path)?;
            } else if file_type.is_dir() {
                debug!("creating directory {}", target_path.display());
                create_dir_all(&target_path).with_context(|| {
                    format!("failed to create directory at {}", target_path.display())
                })?;
            } else {
                debug!(
                    "copying file {} to {}",
                    full_path.display(),
                    target_path.display(),
                );
                fs::copy(full_path, &target_path).with_context(|| {
                    format!(
                        "failed to copy file from {} to {}",
                        full_path.display(),
                        target_path.display()
                    )
                })?;
            }
        }

        Ok(())
    }

    // We check for this because some projects use a top-level dir like `project-x86-64-Linux`,
    // which is pretty annoying to work with. In this case, it's a lot friendlier to install this
    // into `~/bin/project`, so the directory tree ends up with the same structure on all platforms.
    fn extracted_contents_top_level_dir(&self, contents_dir: &Path) -> Result<Option<PathBuf>> {
        let mut prefixes: HashMap<PathBuf, OsString> = HashMap::new();
        debug!(
            "checking whether extracted contents at {} share a single-top level dir with the project name `{}`",
            contents_dir.display(),
            self.project_name,
        );
        for entry in fs::read_dir(contents_dir).with_context(|| {
            format!(
                "could not read {} after unpacking the tarball into this directory",
                self.install_root.display(),
            )
        })? {
            let full_path = entry
                .with_context(|| {
                    format!(
                        "could not read directory entry in {}",
                        contents_dir.display()
                    )
                })?
                .path();

            debug!("found entry in temp dir: {}", full_path.display());

            // If the entry is a file in the top-level of the install dir, then there's no common
            // directory prefix.
            if full_path.is_file()
                && full_path
                    .parent()
                    .expect("path of entry in temp dir somehow has no parent")
                    == contents_dir
            {
                return Ok(None);
            }

            let Ok(path) = full_path.strip_prefix(contents_dir) else {
                return Err(anyhow!(
                    "temp dir {} contains a path {} that somehow isn't in itself",
                    contents_dir.display(),
                    full_path.display(),
                ));
            };

            if let Some(prefix) = path.components().next() {
                prefixes.insert(full_path.clone(), prefix.as_os_str().to_os_string());
            } else {
                return Err(anyhow!("directory entry has no path components"));
            }
        }

        if prefixes.len() == 1 {
            let (path, prefix) = prefixes.into_iter().next().unwrap();
            debug!(
                "the extracted archive has a single common prefix: `{}`",
                prefix.to_string_lossy(),
            );
            return Ok(Some(path));
        }

        debug!("the extracted archive has multiple top-level files or directories");

        Ok(None)
    }
}

// A symlink that is already in the install root, maybe from an earlier install, could point
// anywhere. Creating a directory or copying a file through it could write outside the install
// root, so we refuse to do that. The last part of the path is allowed to be a symlink when we are
// about to replace it with a new symlink, since we remove the old one first and never write
// through it.
fn check_for_symlinks_in_install_path(
    install_root: &Path,
    rel_path: &Path,
    replacing_last_part: bool,
) -> Result<()> {
    let parts = rel_path.components().collect::<Vec<_>>();
    let mut path = install_root.to_path_buf();
    for (i, part) in parts.iter().enumerate() {
        path.push(part);
        if replacing_last_part && i == parts.len() - 1 {
            break;
        }
        if fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err(anyhow!(
                "cannot install into {} because {} is a symlink, and writing through it could write outside the install directory",
                install_root.display(),
                path.display(),
            ));
        }
    }
    Ok(())
}

// We check every link in the archive before we copy anything, so that a bad link doesn't leave a
// partial install behind. The links are already on disk in the temp dir, so we let the OS resolve
// them. That handles chains of links and `..` the same way the OS will after we install them.
fn check_extracted_symlinks(copy_from: &Path) -> Result<()> {
    if fs::symlink_metadata(copy_from).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err(anyhow!(
            "the top-level directory in the archive, {}, is a symlink, which ubi will not install",
            file_name_for_error(copy_from),
        ));
    }

    let root = fs::canonicalize(copy_from)
        .with_context(|| format!("failed to resolve the path {}", copy_from.display()))?;
    for entry in WalkDir::new(copy_from) {
        let entry = entry.with_context(|| {
            format!(
                "failed to read the extracted contents in {}",
                copy_from.display()
            )
        })?;
        if !entry.file_type().is_symlink() {
            continue;
        }

        let link = entry.path();
        let target = fs::read_link(link)
            .with_context(|| format!("failed to read the symlink at {}", link.display()))?;
        let resolved = match fs::canonicalize(link) {
            Ok(resolved) => resolved,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                resolve_broken_symlink(link, &target)?
            }
            Err(e) => {
                return Err(e).with_context(|| {
                    format!("failed to resolve the symlink at {}", link.display())
                })
            }
        };
        if !resolved.starts_with(&root) {
            return Err(anyhow!(
                "the symlink at {} in the archive points outside the install directory, to {}",
                link.strip_prefix(copy_from).unwrap_or(link).display(),
                target.display(),
            ));
        }
    }

    Ok(())
}

fn file_name_for_error(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    )
}

// The OS can't resolve a broken link, so we walk the target one part at a time. We let the OS
// resolve each part that exists, since it might be another link. Once we reach a part that doesn't
// exist, nothing after it can be a link, so we handle the rest without looking at the disk.
fn resolve_broken_symlink(link: &Path, target: &Path) -> Result<PathBuf> {
    use std::path::Component;

    let link_dir = link.parent().unwrap_or(link);
    let mut resolved = fs::canonicalize(link_dir)
        .with_context(|| format!("failed to resolve the path {}", link_dir.display()))?;
    let mut exists = true;
    for part in target.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                resolved.pop();
            }
            Component::Prefix(_) | Component::RootDir => {
                resolved = PathBuf::from(part.as_os_str());
            }
            Component::Normal(_) => {
                resolved.push(part);
                if exists && fs::symlink_metadata(&resolved).is_ok() {
                    // If this fails, this part is itself a broken link. We don't try to follow a
                    // chain of broken links, so we treat it as pointing outside.
                    resolved = fs::canonicalize(&resolved).map_err(|_| {
                        anyhow!(
                            "the symlink at {} points to another broken symlink, {}, which ubi will not install",
                            link.display(),
                            resolved.display(),
                        )
                    })?;
                } else {
                    exists = false;
                }
            }
        }
    }
    Ok(resolved)
}

#[cfg(target_family = "unix")]
fn copy_symlink(from: &Path, to: &Path) -> Result<()> {
    let target = fs::read_link(from)
        .with_context(|| format!("failed to read the symlink at {}", from.display()))?;
    debug!(
        "creating symlink {} pointing to {}",
        to.display(),
        target.display()
    );
    // Creating a symlink fails if something is already at the path, so we remove an old file or
    // link left from an earlier install. We leave a directory alone so that we don't delete a tree.
    if fs::symlink_metadata(to).is_ok_and(|m| !m.is_dir()) {
        fs::remove_file(to)
            .with_context(|| format!("failed to remove the old file at {}", to.display()))?;
    }
    std::os::unix::fs::symlink(&target, to).with_context(|| {
        format!(
            "failed to create a symlink at {} pointing to {}",
            to.display(),
            target.display()
        )
    })
}

// Creating symlinks on Windows needs special rights, so we copy what the link points to.
#[cfg(target_family = "windows")]
fn copy_symlink(from: &Path, to: &Path) -> Result<()> {
    if from.is_dir() {
        return create_dir_all(to)
            .with_context(|| format!("failed to create directory at {}", to.display()));
    }
    fs::copy(from, to)
        .with_context(|| {
            format!(
                "failed to copy file from {} to {}",
                from.display(),
                to.display()
            )
        })
        .map(|_| ())
}

impl Installer for ArchiveInstaller {
    /// The install root is a directory we create files under, so a file sitting there is in the
    /// way. Note that this only checks the root itself. A file in the way further down the tree
    /// still fails the old way.
    fn check_install_path(&self) -> Result<()> {
        check_install_dir_is_not_a_file(&self.install_root)
    }

    fn install(&self, download: &Download) -> Result<()> {
        self.extract_entire_archive(&download.archive_path)?;
        info!(
            "Installed contents of archive file into {}",
            self.install_root.display()
        );

        Ok(())
    }
}

fn tar_reader_for(downloaded_file: &Path) -> Result<TarArchive<Box<dyn Read>>> {
    let file = open_file(downloaded_file)?;

    let ext = downloaded_file.extension();
    match ext {
        Some(ext) => match ext.to_str() {
            Some("tar") => Ok(TarArchive::new(Box::new(file))),
            Some("bz" | "tbz" | "bz2" | "tbz2") => {
                Ok(TarArchive::new(Box::new(BzDecoder::new(file))))
            }
            Some("gz" | "tgz") => Ok(TarArchive::new(Box::new(GzDecoder::new(file)))),
            Some("xz" | "txz") => Ok(TarArchive::new(Box::new(XzDecoder::new(file)))),
            Some("zst" | "tzst") => Ok(TarArchive::new(Box::new(ZstdDecoder::new(file)?))),
            Some(e) => Err(anyhow!(
                "don't know how to uncompress a tarball with extension = {e}",
            )),
            None => Err(anyhow!(
                "tarball {} has a non-UTF-8 extension",
                downloaded_file.display(),
            )),
        },
        None => Ok(TarArchive::new(Box::new(file))),
    }
}

fn open_file(path: &Path) -> Result<File> {
    File::open(path).with_context(|| format!("Failed to open file at {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;
    #[cfg(target_family = "unix")]
    use std::os::unix::fs::{symlink, PermissionsExt};
    use tempfile::tempdir;

    #[test]
    fn exe_installer_rejects_a_dir_at_the_install_path() -> Result<()> {
        let td = tempdir()?;

        let installer = |path: &Path, from_rename_exe_to, from_exe| {
            ExeInstaller::new(
                path.to_path_buf(),
                from_rename_exe_to,
                from_exe,
                "some-exe".to_string(),
                false,
            )
        };

        let file = td.path().join("some-exe");
        File::create(&file)?;
        assert!(installer(&file, false, false)
            .check_install_path_is_not_a_dir(&file)
            .is_ok());

        let missing = td.path().join("does-not-exist");
        assert!(installer(&missing, false, false)
            .check_install_path_is_not_a_dir(&missing)
            .is_ok());

        // The error names every flag that produced the path, and only those.
        let dir = td.path().join("in-the-way");
        create_dir_all(&dir)?;
        for (from_rename_exe_to, from_exe, expect, not_expect) in [
            (false, false, vec!["--in"], vec!["--rename-exe", "--exe"]),
            (false, true, vec!["--in", "--exe"], vec!["--rename-exe"]),
            (true, false, vec!["--in", "--rename-exe"], vec!["--exe"]),
            // `--exe` can be passed alongside `--rename-exe`, but then it only says what to look
            // for in the archive. The installed name comes from `--rename-exe`.
            (true, true, vec!["--in", "--rename-exe"], vec!["--exe"]),
        ] {
            let err = installer(&dir, from_rename_exe_to, from_exe)
                .check_install_path_is_not_a_dir(&dir)
                .unwrap_err()
                .to_string();
            assert!(
                err.contains("there is already a directory at that path"),
                "got error: {err}",
            );
            for flag in expect {
                assert!(err.contains(&format!("`{flag}`")), "got error: {err}");
            }
            for flag in not_expect {
                assert!(!err.contains(&format!("`{flag}`")), "got error: {err}");
            }
        }

        Ok(())
    }

    // The install path grows an extension for assets like `.AppImage`, so a directory sitting at
    // the path without that extension is not actually in the way. We can only know that after
    // seeing the downloaded file, so the check before the download has to stay quiet here.
    #[test]
    fn exe_installer_does_not_reject_a_dir_that_the_install_extension_avoids() -> Result<()> {
        let td = tempdir()?;

        let dir = td.path().join("some-exe");
        create_dir_all(&dir)?;

        let installer = ExeInstaller::new(dir.clone(), false, false, "some-exe".to_string(), false);
        assert_ne!(
            installer.maybe_munged_install_path("some-exe.AppImage")?,
            dir,
            "the install path grows the AppImage extension",
        );
        assert!(installer.check_install_path().is_ok());

        // When the user named the install path exactly, we never munge it, so the directory really
        // is in the way and we can say so before downloading anything.
        let installer = ExeInstaller::new(dir.clone(), true, false, "some-exe".to_string(), false);
        assert_eq!(
            installer.maybe_munged_install_path("some-exe.AppImage")?,
            dir
        );
        assert!(installer.check_install_path().is_err());

        Ok(())
    }

    #[test]
    fn exe_installer_rejects_a_file_at_the_install_dir() -> Result<()> {
        let td = tempdir()?;

        let installer = |dir: &Path, from_rename_exe_to| {
            ExeInstaller::new(
                dir.join("some-exe"),
                from_rename_exe_to,
                false,
                "some-exe".to_string(),
                false,
            )
        };

        let dir = td.path().join("some-dir");
        create_dir_all(&dir)?;
        assert!(installer(&dir, false).check_install_path().is_ok());

        // The install directory is created later if it doesn't exist yet.
        let missing = td.path().join("does-not-exist");
        assert!(installer(&missing, false).check_install_path().is_ok());

        // This is checked before we download anything, whether or not the last component of the
        // install path was munged, because the directory itself is never munged.
        let file = td.path().join("in-the-way");
        File::create(&file)?;
        for from_rename_exe_to in [false, true] {
            let err = installer(&file, from_rename_exe_to)
                .check_install_path()
                .unwrap_err()
                .to_string();
            assert!(
                err.contains("is a file, not a directory"),
                "got error: {err}",
            );
            assert!(err.contains("`--in`"), "got error: {err}");
            assert!(!err.contains("`--rename-exe`"), "got error: {err}");
        }

        // The check also guards the point where we create the directory, which is what library
        // users reach when they don't go through `Ubi::install_binary`.
        let err = installer(&file, false)
            .create_install_dir()
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("is a file, not a directory"),
            "got error: {err}",
        );

        Ok(())
    }

    // `is_dir` and `exists` both follow symlinks, so the check has to look past them in both
    // directions: a symlink to a directory is a perfectly good install directory, and a symlink to
    // nothing looks like an empty path to those two while still being in the way.
    #[test]
    #[cfg(target_family = "unix")]
    fn exe_installer_looks_through_a_symlink_at_the_install_dir() -> Result<()> {
        let td = tempdir()?;

        let installer = |dir: &Path| {
            ExeInstaller::new(
                dir.join("some-exe"),
                false,
                false,
                "some-exe".to_string(),
                false,
            )
        };

        let dir = td.path().join("real-dir");
        create_dir_all(&dir)?;
        let to_dir = td.path().join("to-dir");
        symlink(&dir, &to_dir)?;
        assert!(installer(&to_dir).check_install_path().is_ok());

        let dangling = td.path().join("dangling");
        symlink(td.path().join("does-not-exist"), &dangling)?;
        let err = installer(&dangling)
            .check_install_path()
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("is a symlink that doesn't point to anything"),
            "got error: {err}",
        );

        Ok(())
    }

    #[test]
    fn archive_installer_rejects_a_file_at_the_install_root() -> Result<()> {
        let td = tempdir()?;

        let installer =
            |path: &Path| ArchiveInstaller::new("some-project".to_string(), path.to_path_buf());

        let dir = td.path().join("some-dir");
        create_dir_all(&dir)?;
        assert!(installer(&dir).check_install_path().is_ok());

        // The install root is created later if it doesn't exist yet.
        let missing = td.path().join("does-not-exist");
        assert!(installer(&missing).check_install_path().is_ok());

        let file = td.path().join("in-the-way");
        File::create(&file)?;
        let err = installer(&file)
            .check_install_path()
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("is a file, not a directory"),
            "got error: {err}",
        );

        Ok(())
    }

    #[rstest]
    #[case("test-data/project.7z", None)]
    #[case("test-data/project.AppImage", Some("AppImage"))]
    #[case("test-data/project.bat", Some("bat"))]
    #[case("test-data/project.bz", None)]
    #[case("test-data/project.bz2", None)]
    #[case("test-data/project.exe", Some("exe"))]
    #[case("test-data/project.gz", None)]
    #[case("test-data/project.jar", Some("jar"))]
    #[case("test-data/project.phar", Some("phar"))]
    #[case("test-data/project.py", Some("py"))]
    #[case("test-data/project.pyz", Some("pyz"))]
    #[case("test-data/project.sh", Some("sh"))]
    #[case("test-data/project.tar", None)]
    #[case("test-data/project.tar.bz", None)]
    #[case("test-data/project.tar.bz2", None)]
    #[case("test-data/project.tar.gz", None)]
    #[case("test-data/project.tar.xz", None)]
    #[case("test-data/project.tar.zst", None)]
    #[case("test-data/project.xz", None)]
    #[case("test-data/project.zip", None)]
    #[case("test-data/project.zst", None)]
    #[case("test-data/project", None)]
    // This tests a bug where zip files with partial matches before an exact match would pick the wrong file.
    #[case("test-data/project-with-partial-before-exact.zip", None)]
    // These are archive files that just contain a partial match for the expected executable.
    #[case("test-data/project-with-partial-match.tar.gz", None)]
    #[case("test-data/project-with-partial-match.zip", None)]
    fn exe_installer(
        #[case] archive_path: &str,
        #[case] installed_extension: Option<&str>,
    ) -> Result<()> {
        crate::test_log::init_logging();

        let td = tempdir()?;
        let path_without_subdir = td.path().to_path_buf();
        test_exe_installer(
            archive_path,
            installed_extension,
            path_without_subdir,
            false,
        )?;

        let td = tempdir()?;
        let mut path_with_subdir = td.path().to_path_buf();
        path_with_subdir.push("subdir");
        test_exe_installer(archive_path, installed_extension, path_with_subdir, false)
    }

    // These tests check that we look for project.bat and project.exe in archive files when running
    // on Windows.
    #[rstest]
    #[case("test-data/windows-project-exe.7z", "exe")]
    #[case("test-data/windows-project-bat.tar.gz", "bat")]
    #[case("test-data/windows-project-exe.tar.gz", "exe")]
    #[case("test-data/windows-project-bat.zip", "bat")]
    #[case("test-data/windows-project-exe.zip", "exe")]
    // And these check that we match project-with-stuff.exe.
    #[case("test-data/windows-project-exe-with-partial-match.tar.gz", "exe")]
    #[case("test-data/windows-project-exe-with-partial-match.zip", "exe")]
    // A single compressed file or a bare file has no extension of its own to keep, but Windows
    // still needs `.exe` to run it.
    #[case("test-data/project.bz", "exe")]
    #[case("test-data/project.bz2", "exe")]
    #[case("test-data/project.gz", "exe")]
    #[case("test-data/project.xz", "exe")]
    #[case("test-data/project.zst", "exe")]
    #[case("test-data/project", "exe")]
    fn exe_installer_on_windows(#[case] archive_path: &str, #[case] extension: &str) -> Result<()> {
        crate::test_log::init_logging();

        let td = tempdir()?;
        let install_dir = td.path().to_path_buf();

        test_exe_installer(archive_path, Some(extension), install_dir, true)
    }

    #[rstest]
    #[case::adds_exe("project", true, "project.exe")]
    #[case::keeps_exe("project.exe", true, "project.exe")]
    #[case::keeps_upper_case_exe("PROJECT.EXE", true, "PROJECT.EXE")]
    #[case::keeps_bat("project.bat", true, "project.bat")]
    #[case::keeps_dotted_name("tool.v2", true, "tool.v2.exe")]
    #[case::not_on_windows("project", false, "project")]
    fn maybe_add_windows_exe_extension(
        #[case] name: &str,
        #[case] is_windows: bool,
        #[case] expect: &str,
    ) {
        let installer = ExeInstaller::new(
            PathBuf::from(name),
            false,
            false,
            "project".to_string(),
            is_windows,
        );
        assert_eq!(
            installer.maybe_add_windows_exe_extension(PathBuf::from(name)),
            PathBuf::from(expect),
        );
    }

    fn test_exe_installer(
        archive_path: &str,
        installed_extension: Option<&str>,
        install_dir: PathBuf,
        is_windows: bool,
    ) -> Result<()> {
        let mut install_path = install_dir.clone();
        install_path.push("project");

        run_one_exe_installer_test(
            &install_path,
            archive_path,
            installed_extension,
            is_windows,
            false,
        )?;

        let mut install_path = install_dir;
        install_path.push("project.foo");

        run_one_exe_installer_test(
            &install_path,
            archive_path,
            installed_extension,
            is_windows,
            true,
        )?;

        Ok(())
    }

    fn run_one_exe_installer_test(
        install_path: &Path,
        archive_path: &str,
        installed_extension: Option<&str>,
        is_windows: bool,
        install_path_is_from_rename_exe_to: bool,
    ) -> Result<()> {
        let exe_file_stem = "project";

        let installer = ExeInstaller::new(
            install_path.to_path_buf(),
            install_path_is_from_rename_exe_to,
            false,
            exe_file_stem.to_string(),
            is_windows,
        );

        installer.install(&Download {
            // It doesn't matter what we use here. We're not actually going to
            // put anything in this temp dir.
            _temp_dir: tempdir()?,
            archive_path: PathBuf::from(archive_path),
        })?;

        let mut expect_install_path = install_path.to_path_buf();
        if !install_path_is_from_rename_exe_to {
            if let Some(installed_extension) = installed_extension {
                let path = PathBuf::from(format!("foo.{installed_extension}"));
                let ext = Extension::from_path(&path).unwrap().unwrap();
                if ext.should_preserve_extension_on_install() {
                    expect_install_path.set_extension(ext.extension_without_dot());
                }
            }
        }

        assert!(
            fs::exists(&expect_install_path)?,
            "{} file exists",
            expect_install_path.display()
        );
        // Testing the installed file's length is a shortcut to make sure we install the file we
        // expected to install.
        let expect_len = if Path::new(archive_path)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("pyz"))
        {
            fs::metadata(archive_path)?.len()
        } else {
            3
        };
        let meta = expect_install_path.metadata()?;
        assert_eq!(meta.len(), expect_len);
        #[cfg(target_family = "unix")]
        assert!(meta.permissions().mode() & 0o111 != 0);

        Ok(())
    }

    #[rstest]
    #[case("test-data/project.7z")]
    #[case("test-data/project.tar")]
    #[case("test-data/project.tar.bz")]
    #[case("test-data/project.tar.bz2")]
    #[case("test-data/project.tar.gz")]
    #[case("test-data/project.tar.xz")]
    #[case("test-data/project.tar.zst")]
    #[case("test-data/project.zip")]
    fn archive_installer(#[case] archive_path: &str) -> Result<()> {
        crate::test_log::init_logging();

        let td = tempdir()?;
        let mut path_without_subdir = td.path().to_path_buf();
        path_without_subdir.push("project");
        let mut path_with_subdir = td.path().to_path_buf();
        path_with_subdir.extend(&["subdir", "project"]);

        for install_root in [path_without_subdir, path_with_subdir] {
            let installer = ArchiveInstaller::new(String::from("project"), install_root.clone());
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

    #[cfg(target_family = "unix")]
    #[rstest]
    #[case::tarball("project.tar.gz")]
    #[case::zip("project.zip")]
    fn archive_installer_keeps_symlinks(#[case] archive_name: &str) -> Result<()> {
        crate::test_log::init_logging();

        let td = tempdir()?;
        let archive_path = td.path().join(archive_name);
        write_archive(
            &archive_path,
            &[
                ("project/bin/tool-alias", "tool"),
                ("project/lib/current", "v1"),
                ("project/bin/dangling", "missing"),
                ("project/bin/v1", "../lib/v1"),
            ],
        )?;

        let install_root = td.path().join("installed");
        let installer = ArchiveInstaller::new(String::from("project"), install_root.clone());
        installer.install(&Download {
            _temp_dir: tempdir()?,
            archive_path,
        })?;

        for (link, target) in [
            ("bin/tool-alias", "tool"),
            ("lib/current", "v1"),
            ("bin/dangling", "missing"),
            ("bin/v1", "../lib/v1"),
        ] {
            let path = install_root.join(link);
            let meta = fs::symlink_metadata(&path)
                .with_context(|| format!("{} should exist", path.display()))?;
            assert!(meta.file_type().is_symlink(), "{link} is a symlink");
            assert_eq!(
                fs::read_link(&path)?,
                PathBuf::from(target),
                "{link} target"
            );
        }
        assert!(install_root.join("bin/tool").is_file());
        assert!(install_root.join("lib/v1/data").is_file());

        Ok(())
    }

    // The `zip` crate refuses some of these links itself, before our own check sees them, so those
    // cases expect its error instead of ours.
    #[cfg(target_family = "unix")]
    #[rstest]
    #[case::absolute_tarball("project.tar.gz", "/etc/passwd", OUTSIDE_INSTALL_DIR)]
    #[case::absolute_zip("project.zip", "/etc/passwd", ZIP_SYMLINK_ESCAPES)]
    #[case::parent_dirs_tarball("project.tar.gz", "../../outside", OUTSIDE_INSTALL_DIR)]
    #[case::parent_dirs_zip("project.zip", "../../outside", OUTSIDE_INSTALL_DIR)]
    // The first link points at the install root, which is fine by itself. But the OS follows `up`
    // before it applies the `..` after it, so the second link points outside the root.
    #[case::chain_tarball("project.tar.gz", "up/../../outside", OUTSIDE_INSTALL_DIR)]
    #[case::chain_zip("project.zip", "up/../../outside", ZIP_PATH_ESCAPES)]
    #[case::broken_parent_dirs("project.tar.gz", "missing/../../../outside", OUTSIDE_INSTALL_DIR)]
    #[case::broken_absolute("project.tar.gz", "/nonexistent/outside", OUTSIDE_INSTALL_DIR)]
    fn archive_installer_rejects_symlinks_that_leave_the_install_root(
        #[case] archive_name: &str,
        #[case] target: &str,
        #[case] expect_err: &str,
    ) -> Result<()> {
        crate::test_log::init_logging();

        let td = tempdir()?;
        let archive_path = td.path().join(archive_name);
        write_archive(
            &archive_path,
            &[("project/bin/up", ".."), ("project/bin/escape", target)],
        )?;

        let install_root = td.path().join("installed");
        let installer = ArchiveInstaller::new(String::from("project"), install_root.clone());
        let err = installer
            .install(&Download {
                _temp_dir: tempdir()?,
                archive_path,
            })
            .unwrap_err();
        let err = format!("{err:#}");
        assert!(err.contains(expect_err), "got error: {err}");
        // We check the whole archive before installing anything, so a bad link doesn't leave a
        // partial install behind.
        assert!(
            !install_root.exists(),
            "nothing was installed to {}",
            install_root.display(),
        );

        Ok(())
    }

    #[cfg(target_family = "unix")]
    #[rstest]
    #[case::loop_(&[("project/bin/a", "b"), ("project/bin/b", "a")], "failed to resolve the symlink")]
    #[case::link_to_broken_link(
        &[("project/bin/broken", "missing"), ("project/bin/x", "broken/../../outside")],
        "points to another broken symlink",
    )]
    fn archive_installer_rejects_links_it_cannot_check(
        #[case] links: &[(&str, &str)],
        #[case] expect_err: &str,
    ) -> Result<()> {
        crate::test_log::init_logging();

        let td = tempdir()?;
        let archive_path = td.path().join("project.tar.gz");
        write_archive(&archive_path, links)?;

        let install_root = td.path().join("installed");
        let installer = ArchiveInstaller::new(String::from("project"), install_root.clone());
        let err = installer
            .install(&Download {
                _temp_dir: tempdir()?,
                archive_path,
            })
            .unwrap_err();
        let err = format!("{err:#}");
        assert!(err.contains(expect_err), "got error: {err}");
        assert!(!install_root.exists());

        Ok(())
    }

    // Installing over an earlier install has to replace the old links, not write through them.
    #[cfg(target_family = "unix")]
    #[test]
    fn archive_installer_replaces_symlinks_on_reinstall() -> Result<()> {
        crate::test_log::init_logging();

        let td = tempdir()?;
        let archive_path = td.path().join("project.tar.gz");
        write_archive(&archive_path, &[("project/bin/tool-alias", "tool")])?;

        let install_root = td.path().join("installed");
        let installer = ArchiveInstaller::new(String::from("project"), install_root.clone());
        for _ in 0..2 {
            installer.install(&Download {
                _temp_dir: tempdir()?,
                archive_path: archive_path.clone(),
            })?;
        }
        assert_eq!(
            fs::read_link(install_root.join("bin/tool-alias"))?,
            PathBuf::from("tool"),
        );

        Ok(())
    }

    // A link left by an earlier install must not let this install write outside the install root.
    #[cfg(target_family = "unix")]
    #[test]
    fn archive_installer_does_not_write_through_an_existing_symlink() -> Result<()> {
        crate::test_log::init_logging();

        let td = tempdir()?;
        let archive_path = td.path().join("project.tar.gz");
        write_archive(&archive_path, &[])?;

        let outside = td.path().join("outside");
        create_dir_all(&outside)?;
        let install_root = td.path().join("installed");
        create_dir_all(&install_root)?;
        symlink(&outside, install_root.join("lib"))?;

        let installer = ArchiveInstaller::new(String::from("project"), install_root.clone());
        let err = installer
            .install(&Download {
                _temp_dir: tempdir()?,
                archive_path,
            })
            .unwrap_err();
        let err = format!("{err:#}");
        assert!(err.contains("is a symlink"), "got error: {err}");
        assert!(
            fs::read_dir(&outside)?.next().is_none(),
            "nothing was written to {}",
            outside.display(),
        );

        Ok(())
    }

    // If the archive's only top-level entry is a link, we must not follow it and copy whatever it
    // points to into the install root.
    // As above, the `zip` crate refuses this link itself.
    #[cfg(target_family = "unix")]
    #[rstest]
    #[case::tarball(
        "project.tar.gz",
        "top-level directory in the archive, project, is a symlink"
    )]
    #[case::zip("project.zip", ZIP_SYMLINK_ESCAPES)]
    fn archive_installer_does_not_follow_a_top_level_symlink(
        #[case] archive_name: &str,
        #[case] expect_err: &str,
    ) -> Result<()> {
        crate::test_log::init_logging();

        let td = tempdir()?;
        let outside = td.path().join("outside");
        create_dir_all(&outside)?;
        fs::write(outside.join("secret"), "abc")?;

        let archive_path = td.path().join(archive_name);
        let outside_str = outside.to_string_lossy();
        write_archive_with_files(&archive_path, &[], &[("project", outside_str.as_ref())])?;

        let install_root = td.path().join("installed");
        let installer = ArchiveInstaller::new(String::from("project"), install_root.clone());
        let res = installer.install(&Download {
            _temp_dir: tempdir()?,
            archive_path,
        });
        let err = format!("{:#}", res.unwrap_err());
        assert!(err.contains(expect_err), "got error: {err}");
        assert!(!install_root.join("secret").exists());

        Ok(())
    }

    // Every archive gets the same two files, `project/bin/tool` and `project/lib/v1/data`, plus
    // the given `(name, target)` symlinks.
    #[cfg(target_family = "unix")]
    fn write_archive(path: &Path, links: &[(&str, &str)]) -> Result<()> {
        write_archive_with_files(path, &ARCHIVE_FILES, links)
    }

    #[cfg(target_family = "unix")]
    fn write_archive_with_files(path: &Path, files: &[&str], links: &[(&str, &str)]) -> Result<()> {
        if path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("zip"))
        {
            write_zip(path, files, links)
        } else {
            write_tarball(path, files, links)
        }
    }

    #[cfg(target_family = "unix")]
    const ARCHIVE_FILES: [&str; 2] = ["project/bin/tool", "project/lib/v1/data"];

    #[cfg(target_family = "unix")]
    const OUTSIDE_INSTALL_DIR: &str = "points outside the install directory";
    #[cfg(target_family = "unix")]
    const ZIP_SYMLINK_ESCAPES: &str = "Symlink target escapes the destination directory";
    #[cfg(target_family = "unix")]
    const ZIP_PATH_ESCAPES: &str = "Path escapes the destination directory";

    #[cfg(target_family = "unix")]
    fn write_tarball(path: &Path, files: &[&str], links: &[(&str, &str)]) -> Result<()> {
        let mut builder = binstall_tar::Builder::new(flate2::write::GzEncoder::new(
            File::create(path)?,
            flate2::Compression::default(),
        ));
        for name in files {
            let mut header = binstall_tar::Header::new_gnu();
            header.set_size(3);
            header.set_mode(0o755);
            header.set_cksum();
            builder.append_data(&mut header, name, &b"abc"[..])?;
        }
        for (name, target) in links {
            let mut header = binstall_tar::Header::new_gnu();
            header.set_entry_type(binstall_tar::EntryType::Symlink);
            header.set_size(0);
            header.set_mode(0o777);
            builder.append_link(&mut header, name, target)?;
        }
        builder.into_inner()?.finish()?;
        Ok(())
    }

    #[cfg(target_family = "unix")]
    fn write_zip(path: &Path, files: &[&str], links: &[(&str, &str)]) -> Result<()> {
        let mut zip = zip::ZipWriter::new(File::create(path)?);
        let options = zip::write::SimpleFileOptions::default().unix_permissions(0o755);
        for name in files {
            zip.start_file(name, options)?;
            zip.write_all(b"abc")?;
        }
        for (name, target) in links {
            zip.add_symlink(*name, *target, options)?;
        }
        zip.finish()?;
        Ok(())
    }

    // This tests a bug in the initial implementation where a tarball that just contained files
    // caused us to try to move its contents up to a directory that didn't exist.
    #[test_log::test]
    fn archive_installer_one_file_in_archive_root() -> Result<()> {
        let td = tempdir()?;
        let mut path_without_subdir = td.path().to_path_buf();
        path_without_subdir.push("project");
        let mut path_with_subdir = td.path().to_path_buf();
        path_with_subdir.extend(&["subdir", "project"]);

        for install_root in [path_without_subdir, path_with_subdir] {
            let installer = ArchiveInstaller::new(String::from("project"), install_root.clone());
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

    #[test_log::test]
    fn archive_installer_no_shared_root_path() -> Result<()> {
        let td = tempdir()?;
        let mut path_without_subdir = td.path().to_path_buf();
        path_without_subdir.push("project");
        let mut path_with_subdir = td.path().to_path_buf();
        path_with_subdir.extend(&["subdir", "project"]);

        for install_root in [path_without_subdir, path_with_subdir] {
            let installer = ArchiveInstaller::new(String::from("project"), install_root.clone());
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

    #[test_log::test]
    fn archive_installer_to_existing_tree() -> Result<()> {
        let td = tempdir()?;
        let mut path_without_subdir = td.path().to_path_buf();
        path_without_subdir.push("project");
        let mut path_with_subdir = td.path().to_path_buf();
        path_with_subdir.extend(&["subdir", "project"]);

        {
            let install_root = path_without_subdir;
            //, path_with_subdir] {
            let bin_dir = install_root.join("bin");
            create_dir_all(&bin_dir)?;

            let share_dir = install_root.join("share");
            create_dir_all(&share_dir)?;

            let installer = ArchiveInstaller::new(String::from("project"), install_root.clone());
            installer.install(&Download {
                // It doesn't matter what we use here. We're not actually going to
                // put anything in this temp dir.
                _temp_dir: tempdir()?,
                archive_path: PathBuf::from("test-data/shared-root.tar.gz"),
            })?;

            assert!(install_root.exists());
            assert!(install_root.is_dir());

            let exe = bin_dir.join("project");
            assert!(exe.exists());
            assert!(exe.is_file());

            let rsrc = share_dir.join("resources.toml");
            assert!(rsrc.exists());
            assert!(rsrc.is_file());
        }

        Ok(())
    }
}
