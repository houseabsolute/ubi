use crate::ubi::Download;
use anyhow::{anyhow, Result};
use digest::{Digest, DynDigest};
use itertools::Itertools;
use log::{debug, info};
use serde::Deserialize;
use std::{
    collections::{hash_map::Keys, HashMap},
    convert::TryFrom,
    ffi::OsStr,
    fs::File,
    io::{self, BufRead, BufReader, Write},
    path::Path,
};
use strum::{AsRefStr, EnumIter, IntoEnumIterator};
use url::Url;

// This returns a `String` instead of a ref because we will use the returned string to remove
// something from the `assets` `HashMap`. If we return a borrowed value then we can't mutate the
// `HashMap` while we still hold the borrowed key.
pub(crate) fn find_checksum_asset_for(name: &str, names: Keys<'_, String, Url>) -> Option<String> {
    for n in names.filter(|&n| n != name) {
        debug!("considering {} as a checksum asset for {}", n, name);
        let path = Path::new(n);
        let Some(path_stem) = path.file_stem() else {
            // This would only happen with an "all extension" name like ".txt", I think.
            continue;
        };

        if is_checksum_file_for(name, path) {
            debug!(
                "{} is a checksum file for {name}, using it for checksumming",
                path.display(),
            );
            return Some(n.to_string());
        }

        let stem_str = path_stem.to_string_lossy();
        if stem_str == "checksums" || stem_str.ends_with("-checksums") {
            continue;
            debug!(
                "{} may be a file with checksums for all assets",
                path.display(),
            );
            if let Some(ext) = path.extension() {
                if ext != "txt" {
                    debug!(
                        "{} has an extension that is not `.txt`, skipping",
                        path.display(),
                    );
                    continue;
                }
            }
            debug!(
                "{} is a checksum file for all assets, using it for checksumming",
                path.display(),
            );
            return Some(n.to_string());
        }
    }

    None
}

static EXTENSIONS: [&str; 5] = [".md5", ".sha1", ".sha256", ".sha512", ".sbom.json"];

// This returns true if the file name appears to be a checksum for the picked asset name, so
// something like "some-project-v1.2.3.tar.gz.sha256" or "some-project-v1.2.3.tar.gz.sbom.json".
fn is_checksum_file_for(name: &str, path: &Path) -> bool {
    EXTENSIONS
        .iter()
        .map(OsStr::new)
        .any(|e| path == Path::new(&format!("{name}{}", e.to_string_lossy())))
}

#[derive(AsRefStr, Debug, strum::Display, EnumIter, Hash, PartialEq, Eq)]
#[strum(serialize_all = "lowercase")]
enum HashAlgorithm {
    SHA512,
    SHA384,
    SHA256,
    SHA224,
    SHA1,
    MD5,
}

impl TryFrom<&str> for HashAlgorithm {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        for alg in HashAlgorithm::iter() {
            if value.to_lowercase() == alg.to_string() {
                return Ok(alg);
            }
        }

        Err(format!(
            "`{value}` does not match any our known hash algorithms: [{}]",
            HashAlgorithm::iter().join(" ")
        ))
    }
}

struct HashWriter<'a>(&'a mut dyn DynDigest);

impl Write for HashWriter<'_> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.update(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
impl HashAlgorithm {
    // This is the order in which we look for a checksum when given multiple choices. Longer is more
    // secure, AFAIK. This requires the enum definition to list them from most to least secure so
    // that `::iter` starts with the most secure.
    fn ordered_list() -> impl Iterator<Item = HashAlgorithm> {
        HashAlgorithm::iter()
    }

    fn from_hex_str(s: &str) -> Result<Self> {
        if !s.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(anyhow!("digest string contains non-hex characters"));
        }

        match s.len() {
            128 => Ok(HashAlgorithm::SHA512),
            96 => Ok(HashAlgorithm::SHA384),
            64 => Ok(HashAlgorithm::SHA256),
            56 => Ok(HashAlgorithm::SHA224),
            40 => Ok(HashAlgorithm::SHA1),
            32 => Ok(HashAlgorithm::MD5),
            _ => Err(anyhow!(
                "could not determine the hash algorithm from the hex string"
            )),
        }
    }

    fn checksum_for(&self, path: &Path) -> Result<String> {
        let file = File::open(path)?;
        Self::sha_digest_for(self.hasher(), file)
    }

    fn hasher(&self) -> Box<dyn DynDigest> {
        match self {
            HashAlgorithm::SHA512 => Box::new(sha2::Sha512::new()),
            HashAlgorithm::SHA384 => Box::new(sha2::Sha384::new()),
            HashAlgorithm::SHA256 => Box::new(sha2::Sha256::new()),
            HashAlgorithm::SHA224 => Box::new(sha2::Sha224::new()),
            HashAlgorithm::SHA1 => Box::new(sha1::Sha1::new()),
            HashAlgorithm::MD5 => Box::new(md5::Md5::new()),
        }
    }

    fn sha_digest_for(mut hasher: Box<dyn DynDigest>, mut file: File) -> Result<String> {
        let mut writer = HashWriter(hasher.as_mut());
        io::copy(&mut file, &mut writer)?;
        Ok(base16ct::lower::encode_string(&hasher.finalize()))
    }
}

pub(crate) fn verify(download: &Download, checksum_download: &Download) -> Result<()> {
    debug!(
        "verifying checksum of {} with {}",
        download.path.display(),
        checksum_download.path.display(),
    );

    let ext = checksum_download.path.extension();
    let downloaded_file_name = download
        .path
        .file_name()
        .expect("the downloaded file should always have a file name")
        .to_string_lossy();
    let (checksum, algorithm) = if ext.is_some() && ext.unwrap() == "json" {
        checksum_from_sbom(&checksum_download.path, &downloaded_file_name)?
    } else {
        checksum_from_text_file(&checksum_download.path, &downloaded_file_name)?
    };

    let actual_hash = algorithm.checksum_for(&download.path)?;
    if actual_hash == checksum {
        info!(
            "checksum for {} is correct: got {checksum}",
            download.path.display(),
        );
        Ok(())
    } else {
        Err(anyhow!(
            "checksum for {} is incorrect: expected {checksum}, got {actual_hash}",
            download.path.display(),
        ))
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Sbom {
    files: Vec<SbomFile>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SbomFile {
    file_name: String,
    checksums: Vec<SbomChecksum>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SbomChecksum {
    algorithm: String,
    checksum_value: String,
}

fn checksum_from_sbom(
    sbom_path: &Path,
    downloaded_file_name: &str,
) -> Result<(String, HashAlgorithm)> {
    debug!("{downloaded_file_name} is an SBOM, parsing it as JSON to find checksums");

    let file = File::open(sbom_path)?;
    let sbom: Sbom = serde_json::from_reader(file).map_err(|e| {
        anyhow!(
            "could not read expect SBOM JSON from {} file: {e}",
            sbom_path.display(),
        )
    })?;

    let sbom_file = sbom
        .files
        .into_iter()
        .find(|sf| sf.file_name == downloaded_file_name)
        .ok_or_else(|| {
            anyhow!(
                "could not find a matching file name in the SBOM for {}",
                downloaded_file_name,
            )
        })?;

    sbom_file.checksums.is_empty();

    let mut available_checksums: HashMap<HashAlgorithm, String> = HashMap::new();
    for cs in sbom_file.checksums {
        if let Ok(alg) = HashAlgorithm::try_from(cs.algorithm.as_str()) {
            debug!(
                "found a checksum using the {alg} algorithm: {}",
                cs.checksum_value,
            );
            available_checksums.insert(alg, cs.checksum_value);
        } else {
            info!("SBOM file contains an unknown algorithm: {}", cs.algorithm);
        }
    }

    for alg in HashAlgorithm::ordered_list() {
        if let Some(cs) = available_checksums.remove(&alg) {
            debug!("picking the {} checksum from the SBOM file", alg);
            return Ok((cs, alg));
        }
    }

    Err(anyhow!(
        "the SBOM file did not contain any checksums using known algorithms"
    ))
}

fn checksum_from_text_file(
    checksum_path: &Path,
    downloaded_file_name: &str,
) -> Result<(String, HashAlgorithm)> {
    let file = File::open(checksum_path)?;
    let buf = BufReader::new(file);
    let checksum = checksum_from_lines(buf, downloaded_file_name, checksum_path)?;
    let alg = if let Some(alg) = algorithm_from_path_name(checksum_path) {
        alg
    } else {
        debug!("choosing the hash algorithm based on the checksum content: {checksum}");
        let alg = HashAlgorithm::from_hex_str(&checksum)?;
        debug!("chose the {alg} hash algorithm");
        alg
    };

    Ok((checksum, alg))
}

fn algorithm_from_path_name(path: &Path) -> Option<HashAlgorithm> {
    let file_name = path
        .file_name()
        .expect("the checksum file should always have a name");
    if let Some(alg) = HashAlgorithm::iter().find(|a| path.to_string_lossy().contains(a.as_ref())) {
        debug!(
            "choosing the {alg} hash algorithm based on the checksum filename: `{}",
            file_name.to_string_lossy(),
        );
        return Some(alg);
    }
    debug!(
        "could not determine the hash algorithm based on the checksum filename: `{}",
        file_name.to_string_lossy(),
    );
    None
}

fn checksum_from_lines(
    buf: BufReader<File>,
    download_path: &str,
    checksum_path: &Path,
) -> Result<String> {
    debug!(
        "parsing {} as a text checksum file",
        checksum_path.display()
    );

    let mut relevant_lines: Vec<String> = vec![];
    for line in buf.lines() {
        let line = line?;
        if !line.chars().any(|c| !char::is_whitespace(c)) {
            continue;
        }
        if line.starts_with('#') || line.starts_with("//") {
            continue;
        }
        relevant_lines.push(line);
    }

    let line_count = relevant_lines.len();
    debug!("this file contains {line_count} relevant line(s) (not empty or comments)");

    for line in relevant_lines {
        let fields = line.split_whitespace().collect::<Vec<_>>();
        if line_count == 1 && fields.len() == 1 {
            debug!("checksum file has one relevant line and it has one field, assuming this is the checksum");
            return Ok(fields[0].to_string());
        }

        if fields.len() == 2 {
            debug!("found a line with two fields: {} {}", fields[0], fields[1]);
            if fields[1] == download_path {
                debug!("this line matches our downloaded file name, {download_path}");
                return Ok(fields[0].to_string());
            }
            debug!(
                "this line does not match our downloaded file name, {} - found {}",
                fields[1],
                checksum_path.display(),
            );
        }
    }

    Err(anyhow!(
        "the checksum file did not contain any lines with a checksum for the downloaded file"
    ))
}
