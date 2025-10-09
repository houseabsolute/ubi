use std::path::Path;

use crate::{
    arch::{
        aarch64_re, arm_re, macos_aarch64_and_x86_64_re, macos_aarch64_only_re, mips64_re,
        mips64le_re, mips_re, mipsle_re, ppc32_re, ppc64_re, ppc64le_re, riscv64_re, s390x_re,
        sparc64_re, x86_32_re, x86_64_re, ALL_ARCHES_RE,
    },
    extension::Extension,
    os::{
        android_re, freebsd_re, fuchsia, illumos_re, linux_re, macos_re, netbsd_re, solaris_re,
        windows_re,
    },
    ubi::Asset,
};
use anyhow::{anyhow, Result};
use itertools::Itertools;
use lazy_regex::{regex, Lazy};
use log::debug;
use platforms::{Arch, Endian, Platform, OS};
use regex::Regex;

#[derive(Debug)]
pub(crate) struct AssetPicker<'a> {
    matching: Option<&'a str>,
    matching_regex: Option<&'a str>,
    platform: Platform,
    is_musl: bool,
    archive_only: bool,
}

impl<'a> AssetPicker<'a> {
    pub(crate) fn new(
        matching: Option<&'a str>,
        matching_regex: Option<&'a str>,
        platform: Platform,
        is_musl: bool,
        archive_only: bool,
    ) -> Self {
        Self {
            matching,
            matching_regex,
            platform,
            is_musl,
            archive_only,
        }
    }

    pub(crate) fn pick_asset(&mut self, assets: Vec<Asset>) -> Result<Asset> {
        let all_names = assets.iter().map(|a| &a.name).join(", ");

        let mut assets = self.filter_by_extension(assets);
        if assets.is_empty() {
            let filter = if self.archive_only {
                "for archive files (tarball or zip)"
            } else {
                "for valid extensions"
            };
            return Err(anyhow!(
                "could not find a release asset after filtering {filter} from {all_names}",
            ));
        }

        if let Some(r) = self.matching_regex {
            let re = Regex::new(r)?;
            assets.retain(|a| {
                debug!("matching regex `{r}` against asset name = {}", a.name);
                re.is_match(&a.name)
            });
            if assets.is_empty() {
                return Err(anyhow!(
                    "could not find a release asset matching the regex {} from {all_names}",
                    r,
                ));
            }
        }

        if assets.len() == 1 {
            debug!("there is only one asset to pick");
            return Ok(assets.remove(0));
        }

        let mut matches = self.os_matches(assets);
        if matches.is_empty() {
            return Err(anyhow!(
                "could not find a release asset for this OS ({}) from {all_names}",
                self.platform.target_os,
            ));
        }

        matches = self.arch_matches(matches);
        if matches.is_empty() {
            return Err(anyhow!(
                "could not find a release asset for this OS ({}) and architecture ({}) from {all_names}",
                self.platform.target_os,
                self.platform.target_arch,
            ));
        }

        matches = self.libc_matches(matches);
        if matches.is_empty() {
            let libc_name = self.libc_name();
            return Err(anyhow!(
                "could not find a release asset for this OS ({}), architecture ({}), and libc ({}) from {all_names}",
                self.platform.target_os,
                self.platform.target_arch,
                libc_name,
            ));
        }

        let picked = self.pick_asset_from_matches(matches)?;
        debug!("picked asset from matches named {}", picked.name);
        Ok(picked)
    }

    fn filter_by_extension(&self, assets: Vec<Asset>) -> Vec<Asset> {
        debug!("filtering out assets that do not have a valid extension");
        assets
            .into_iter()
            .filter(|a| match Extension::from_path(Path::new(&a.name)) {
                Err(e) => {
                    debug!("skipping asset with invalid extension: {e}");
                    false
                }
                Ok(Some(ext)) => {
                    debug!("found valid extension, `{}`", ext.extension());
                    if self.archive_only {
                        if ext.is_archive() {
                            debug!("including this asset because it is an archive file");
                            return true;
                        }
                        debug!("not including this asset because it is not an archive file");
                        false
                    } else if ext.matches_platform(&self.platform) {
                        debug!("including this asset because this extension is valid for this platform");
                        true
                    } else {
                        debug!("skipping asset because this extension is not valid for this platform");
                        false
                    }
                }
                Ok(None) => {
                    debug!("found asset with no extension, `{}`", a.name);
                    if self.archive_only {
                        debug!("not including this asset because it is not an archive file");
                        return false;
                    }
                    true
                }
            })
            .collect()
    }

    fn os_matches(&self, assets: Vec<Asset>) -> Vec<Asset> {
        let os_matcher = self.os_matcher();
        debug!("matching assets against OS using {}", os_matcher.as_str());

        let mut matches: Vec<Asset> = vec![];

        // This could all be done much more simply with the iterator's .find()
        // method, but then there's no place to put all the debugging output.
        for asset in assets {
            debug!("matching OS against asset name = {}", asset.name);

            if os_matcher.is_match(&asset.name) {
                if self.platform.target_os != OS::Android && android_re().is_match(&asset.name) {
                    debug!("does not match our OS");
                    continue;
                }

                debug!("matches our OS");
                matches.push(asset);
            } else {
                debug!("does not match our OS");
            }
        }

        matches
    }

    fn arch_matches(&self, mut os_matches: Vec<Asset>) -> Vec<Asset> {
        let arch_matcher = self.arch_matcher();
        debug!(
            "matching assets against CPU architecture using {}",
            arch_matcher.as_str(),
        );

        let mut matches: Vec<Asset> = vec![];
        if os_matches.len() == 1 {
            debug!("there is only one asset that matches our OS");
            if arch_matcher.is_match(&os_matches[0].name) {
                debug!("matches our CPU architecture");
                matches.push(os_matches.remove(0));
            } else if ALL_ARCHES_RE.is_match(&os_matches[0].name) {
                debug!("it matches a CPU architecture which is not ours");
            } else {
                debug!("does not match any CPU architecture, so we will try it");
                matches.push(os_matches.remove(0));
            }
        } else {
            for asset in &os_matches {
                debug!(
                    "matching CPU architecture against asset name = {}",
                    asset.name,
                );
                if arch_matcher.is_match(&asset.name) {
                    debug!("matches our CPU architecture");
                    matches.push(asset.clone());
                } else {
                    debug!("does not match our CPU architecture");
                }
            }

            if matches.is_empty() {
                debug!("no assets matched our CPU architecture, will look for assets without an architecture");
                for asset in os_matches {
                    debug!("matching against asset name = {}", asset.name);
                    if ALL_ARCHES_RE.is_match(&asset.name) {
                        debug!("matches a CPU architecture which is not ours");
                    } else {
                        debug!("does not match any CPU architecture, so we will try it");
                        matches.push(asset);
                    }
                }
            }
        }

        matches
    }

    fn libc_matches(&mut self, matches: Vec<Asset>) -> Vec<Asset> {
        if !self.is_musl {
            return matches;
        }

        debug!("filtering out glibc assets since this is a musl platform");

        let mut libc_matches: Vec<Asset> = vec![];
        for asset in &matches {
            debug!("checking for glibc in asset name = {}", asset.name);
            if asset.name.contains("-gnu") || asset.name.contains("-glibc") {
                debug!("indicates glibc and is not compatible with a musl platform");
                continue;
            } else if asset.name.contains("-musl") {
                debug!("indicates musl");
            } else {
                debug!("name does not indicate the libc it was compiled against");
            }

            libc_matches.push(asset.clone());
        }

        libc_matches
    }

    fn libc_name(&mut self) -> &'static str {
        if self.is_musl {
            "musl"
        } else if self.platform.target_os == OS::Linux {
            "glibc"
        } else {
            "native"
        }
    }

    fn pick_asset_from_matches(&mut self, mut matches: Vec<Asset>) -> Result<Asset> {
        if matches.len() == 1 {
            debug!("only found one candidate asset");
            return Ok(matches.remove(0));
        }

        // Apply --matching filter if there's multiple matches.
        let matches = self.maybe_filter_for_matching_string(matches)?;

        // This comes before 64-bit filtering so that we pick assets with just "arm" in the name
        // (not "arm64") on macOS ARM over something with "x86-64" in the name.
        let (filtered, asset) = self.maybe_pick_asset_for_macos_arm(matches);
        if let Some(asset) = asset {
            return Ok(asset);
        }

        let mut filtered = self.maybe_filter_for_64_bit_arch(filtered);

        if filtered.len() == 1 {
            debug!("only found one candidate asset after filtering");
            return Ok(filtered.remove(0));
        }

        debug!(
            "cannot disambiguate multiple asset names, picking the first one after sorting by name"
        );
        // We don't have any other criteria we could use to pick the right
        // one, and we want to pick the same one every time.
        Ok(filtered
            .into_iter()
            .sorted_by_key(|a| a.name.clone())
            .next()
            .unwrap())
    }

    fn maybe_filter_for_64_bit_arch(&self, matches: Vec<Asset>) -> Vec<Asset> {
        if !matches!(
            self.platform.target_arch,
            Arch::AArch64
                | Arch::Mips64
                | Arch::PowerPc64
                | Arch::Riscv64
                | Arch::S390X
                | Arch::Sparc64
                | Arch::X86_64
        ) {
            return matches.into_iter().collect();
        }

        let asset_names = matches.iter().map(|a| a.name.as_str()).collect::<Vec<_>>();
        debug!("found multiple candidate assets, filtering for 64-bit binaries in {asset_names:?}",);

        if !matches.iter().any(|a| a.name.contains("64")) {
            debug!("no 64-bit assets found, falling back to all assets");
            return matches;
        }

        let sixty_four_bit = matches
            .into_iter()
            .filter(|a| a.name.contains("64"))
            .collect::<Vec<_>>();
        debug!(
            "found 64-bit assets: {}",
            sixty_four_bit.iter().map(|a| a.name.as_str()).join(",")
        );
        sixty_four_bit
    }

    fn maybe_filter_for_matching_string(&self, matches: Vec<Asset>) -> Result<Vec<Asset>> {
        if self.matching.is_none() {
            return Ok(matches);
        }

        let m = self.matching.unwrap();
        debug!(r#"looking for assets matching the string "{m}" passed in --matching"#);
        let filtered: Vec<Asset> = matches.into_iter().filter(|a| a.name.contains(m)).collect();

        if filtered.is_empty() {
            return Err(anyhow!(
                r#"could not find any assets containing our --matching string, "{}""#,
                m,
            ));
        }

        debug!("found {} asset(s) matching the string", filtered.len());
        Ok(filtered)
    }

    fn maybe_pick_asset_for_macos_arm(
        &self,
        mut matches: Vec<Asset>,
    ) -> (Vec<Asset>, Option<Asset>) {
        if !self.running_on_macos_arm() {
            return (matches, None);
        }

        let asset_names = matches.iter().map(|a| a.name.as_str()).collect::<Vec<_>>();
        debug!(
            "found multiple candidate assets and running on macOS ARM, filtering for arm64 binaries in {asset_names:?}",
        );

        let arch_matcher = macos_aarch64_only_re();

        if let Some(idx) = matches.iter().position(|a| arch_matcher.is_match(&a.name)) {
            debug!("found ARM binary named {}", matches[idx].name);
            return (vec![], Some(matches.remove(idx)));
        }

        debug!("did not find any ARM binaries");
        (matches, None)
    }

    fn os_matcher(&self) -> &'static Lazy<Regex> {
        debug!("current OS = {}", self.platform.target_os);

        match self.platform.target_os {
            // The strings the regexes match are those supported by Rust
            // (based on the platforms crate) and Go (based on
            // https://gist.github.com/asukakenji/f15ba7e588ac42795f421b48b8aede63).
            //
            // There are some OS variants in the platforms package that don't
            // correspond to any target supported by rustup. Those are
            // commented out here.
            //
            //OS::Dragonfly => regex!(r"(?i:(?:\b|_)dragonfly(?:\b|_))"),
            OS::FreeBSD => freebsd_re(),
            OS::Fuchsia => fuchsia(),
            //OS::Haiku => regex!(r"(?i:(?:\b|_)haiku(?:\b|_))"),
            OS::IllumOS => illumos_re(),
            OS::Linux => linux_re(),
            OS::MacOS => macos_re(),
            OS::NetBSD => netbsd_re(),
            //OS::OpenBSD => regex!(r"(?i:(?:\b|_)openbsd(?:\b|_))"),
            OS::Solaris => solaris_re(),
            //OS::VxWorks => regex!(r"(?i:(?:\b|_)vxworks(?:\b|_))"),
            OS::Windows => windows_re(),
            _ => unreachable!(
                "Cannot determine what type of compiled binary to use for this platform"
            ),
        }
    }

    fn arch_matcher(&self) -> &'static Lazy<Regex> {
        debug!("current CPU architecture = {}", self.platform.target_arch);

        if self.running_on_macos_arm() {
            return macos_aarch64_and_x86_64_re();
        }

        match (self.platform.target_arch, self.platform.target_endian) {
            (Arch::AArch64, _) => aarch64_re(),
            (Arch::Arm, _) => arm_re(),
            (Arch::Mips, Endian::Little) => mipsle_re(),
            (Arch::Mips, Endian::Big) => mips_re(),
            (Arch::Mips64, Endian::Little) => mips64le_re(),
            (Arch::Mips64, Endian::Big) => mips64_re(),
            (Arch::PowerPc, _) => ppc32_re(),
            (Arch::PowerPc64, Endian::Big) => ppc64_re(),
            (Arch::PowerPc64, Endian::Little) => ppc64le_re(),
            //(Arch::Riscv32, _) => regex!(r"(?i:(?:\b|_)riscv(?:32)?(?:\b|_))"),
            (Arch::Riscv64, _) => riscv64_re(),
            (Arch::S390X, _) => s390x_re(),
            // Sparc is not supported by Go. 32-bit Sparc is not supported
            // by Rust, AFAICT.
            //(Arch::Sparc, _) => regex!(r"(?i:(?:\b|_)sparc(?:\b|_))"),
            (Arch::Sparc64, _) => sparc64_re(),
            (Arch::X86, _) => x86_32_re(),
            (Arch::X86_64, _) => x86_64_re(),
            _ => unreachable!(
                "Cannot determine what type of compiled binary to use for this CPU architecture"
            ),
        }
    }

    fn running_on_macos_arm(&self) -> bool {
        self.platform.target_os == OS::MacOS && self.platform.target_arch == Arch::AArch64
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use rstest::rstest;
    use url::Url;

    #[rstest]
    #[case::x86_64_unknown_linux_gnu_only_one_asset(
        "x86_64-unknown-linux-gnu",
        &["project-Linux-x86_64.tar.gz"],
        None,
        None,
        0
    )]
    #[case::x86_64_unknown_linux_gnu_pick_x86_64_asset(
        "x86_64-unknown-linux-gnu",
        &["project-Linux-i686.tar.gz", "project-Linux-x86_64.tar.gz"],
        None,
        None,
        1
    )]
    #[case::x86_64_unknown_linux_gnu_pick_first_asset_from_two_matching_32_bit_assets(
        "x86_64-unknown-linux-gnu",
        &["project-Linux-i686-gnu.tar.gz", "project-Linux-i686-musl.tar.gz"],
        None,
        None,
        0
    )]
    #[case::x86_64_unknown_linux_gnu_pick_asset_with_matching_string_when_matching_is_set(
        "x86_64-unknown-linux-gnu",
        &["project-Linux-x86_64-gnu.tar.gz", "project-Linux-x86_64-musl.tar.gz"],
        Some("musl"),
        None,
        1
    )]
    #[case::x86_64_unknown_linux_gnu_pick_asset_with_matching_string_from_two_32_bit_assets_when_matching_is_set(
        "x86_64-unknown-linux-gnu",
        &["project-Linux-i686-gnu.tar.gz", "project-Linux-i686-musl.tar.gz"],
        Some("musl"),
        None,
        1
    )]
    #[case::x86_64_unknown_linux_gnu_pick_asset_without_a_suffix_when_matching_regex_is_set(
        "x86_64-unknown-linux-gnu",
        &["project-Linux-x86_64-suffix.tar.gz", "project-Linux-x86_64.tar.gz"],
        None,
        Some(r"\d+\.tar"),
        1
    )]
    #[case::i686_unknown_linux_gnu_pick_one_asset(
        "i686-unknown-linux-gnu",
        &["project-Linux-i686.tar.gz"],
        None,
        None,
        0
    )]
    #[case::i686_unknown_linux_gnu_pick_asset_with_matching_string_when_matching_is_set(
        "i686-unknown-linux-gnu",
        &["project-Linux-i686-gnu.tar.gz", "project-Linux-i686-musl.tar.gz"],
        Some("musl"),
        None,
        1
    )]
    #[case::x86_64_unknown_linux_gnu_pick_correct_arch_when_multiple_assets_match_string(
        "x86_64-unknown-linux-gnu",
        &["project-Linux-i686-musl.tar.gz", "project-Linux-x86_64-musl.tar.gz"],
        Some("musl"),
        None,
        1
    )]
    #[case::aarch64_apple_darwin_pick_one_asset(
        "aarch64-apple-darwin",
        &["project-Macos-aarch64.tar.gz"],
        None,
        None,
        0
    )]
    #[case::aarch64_apple_darwin_pick_asset_with_mac_in_the_name(
        "aarch64-apple-darwin",
        &["project-Linux-x86-64.tar.gz", "project-Mac-x86-64.tar.gz"],
        None,
        None,
        1
    )]
    #[case::aarch64_apple_darwin_pick_asset_with_macosx_in_the_name(
        "aarch64-apple-darwin",
        &["project-Linux-x86-64.tar.gz", "project-Macosx-x86-64.tar.gz"],
        None,
        None,
        1
    )]
    #[case::aarch64_apple_darwin_pick_the_aarch64_asset_on_macOS_ARM(
        "aarch64-apple-darwin",
        &["project-Macos-x86-64.tar.gz", "project-Macos-aarch64.tar.gz"],
        None,
        None,
        1
    )]
    #[case::aarch64_apple_darwin_pick_the_arm_asset_on_macOS_ARM(
        "aarch64-apple-darwin",
        &["project-Macos-x86-64.tar.gz", "project-Macos-arm.tar.gz"],
        None,
        None,
        1
    )]
    #[case::aarch64_apple_darwin_respect_matching_filter_over_arm_preference(
        "aarch64-apple-darwin",
        &["project-a-darwin-arm64.tar.gz", "project-b-darwin-arm64.tar.gz"],
        Some("project-b"),
        None,
        1
    )]
    #[case::aarch64_apple_darwin_pick_the_x86_64_asset_on_macOS_ARM_if_no_aarch64_asset_is_available(
        "aarch64-apple-darwin",
        &["project-Macos-x86-64.tar.gz"],
        None,
        None,
        0
    )]
    #[case::aarch64_apple_darwin_pick_the_all_asset_on_macOS_ARM_if_no_aarch64_asset_is_available(
        "aarch64-apple-darwin",
        &["project-Macos-all.tar.gz"],
        None,
        None,
        0
    )]
    #[case::x86_64_unknown_linux_musl_only_one_asset(
        "x86_64-unknown-linux-musl",
        &["project-Linux-x86_64.tar.gz"],
        None,
        None,
        0
    )]
    #[case::x86_64_unknown_linux_musl_pick_the_musl_asset_over_gnu_on_a_musl_platform(
        "x86_64-unknown-linux-musl",
        &["project-Linux-x86_64-gnu.tar.gz", "project-Linux-x86_64-musl.tar.gz"],
        None,
        None,
        1
    )]
    #[case::x86_64_unknown_linux_musl_pick_the_musl_asset_over_glibc_on_a_musl_platform(
        "x86_64-unknown-linux-musl",
        &["project-Linux-x86_64-glibc.tar.gz", "project-Linux-x86_64-musl.tar.gz"],
        None,
        None,
        1
    )]
    #[case::x86_64_unknown_linux_musl_pick_the_musl_asset_over_unspecified_libc_on_a_musl_platform(
        "x86_64-unknown-linux-musl",
        &["project-Linux-x86_64.tar.gz", "project-Linux-x86_64-musl.tar.gz"],
        None,
        None,
        1
    )]
    #[case::project_aarch64_unknown_linux_pick_the_non_Android_asset_when_not_on_Android(
        "aarch64-unknown-linux-gnu",
        &["project-aarch64-linux-android.tar.gz", "project-aarch64-unknown-linux.tar.gz"],
        None,
        None,
        1
    )]
    #[allow(non_snake_case)]
    fn pick_asset(
        #[case] platform_name: &str,
        #[case] asset_names: &[&str],
        #[case] matching: Option<&str>,
        #[case] matching_regex: Option<&str>,
        #[case] expect_idx: usize,
    ) -> Result<()> {
        crate::test_log::init_logging();

        let platform = Platform::find(platform_name)
            .ok_or(anyhow!("invalid platform name - {platform_name}"))?
            .clone();
        let mut picker = AssetPicker {
            matching,
            matching_regex,
            platform,
            is_musl: platform_name.contains("musl"),
            archive_only: false,
        };

        let url = Url::parse("https://example.com")?;
        let assets = asset_names
            .iter()
            .map(|name| Asset {
                name: (*name).to_string(),
                url: url.clone(),
            })
            .collect::<Vec<_>>();

        let picked_asset = picker.pick_asset(assets)?;
        assert_eq!(picked_asset.name, asset_names[expect_idx]);

        Ok(())
    }

    #[rstest]
    #[case::picks_tarball_from_multiple_matches_tarball_first(
        "x86_64-unknown-linux-gnu",
        &["project-Linux-x86_64.tar.gz", "project-Linux-x86_64.gz"],
        None,
        None,
        0
    )]
    #[case::picks_tarball_from_multiple_matches_tarball_last(
        "x86_64-unknown-linux-gnu",
        &["project-Linux-x86_64.gz", "project-Linux-x86_64.tar.gz"],
        None,
        None,
        1
    )]
    #[case::picks_tarball_over_zip_tarball_first(
        "x86_64-unknown-linux-gnu",
        &["project-Linux-x86_64.tar.gz", "project-Linux-x86_64.zip"],
        None,
        None,
        0
    )]
    #[case::picks_tarball_over_zip_tarball_last(
        "x86_64-unknown-linux-gnu",
        &["project-Linux-x86_64.zip", "project-Linux-x86_64.tar.gz"],
        None,
        None,
        1
    )]
    fn pick_asset_archive_only(
        #[case] platform_name: &str,
        #[case] asset_names: &[&str],
        #[case] matching: Option<&str>,
        #[case] matching_regex: Option<&str>,
        #[case] expect_idx: usize,
    ) -> Result<()> {
        crate::test_log::init_logging();

        let platform = Platform::find(platform_name)
            .ok_or(anyhow!("invalid platform"))?
            .clone();
        let mut picker = AssetPicker {
            matching,
            matching_regex,
            platform,
            is_musl: platform_name.contains("musl"),
            archive_only: true,
        };

        let url = Url::parse("https://example.com")?;
        let assets = asset_names
            .iter()
            .map(|name| Asset {
                name: (*name).to_string(),
                url: url.clone(),
            })
            .collect::<Vec<_>>();

        let picked_asset = picker.pick_asset(assets)?;
        assert_eq!(picked_asset.name, asset_names[expect_idx]);

        Ok(())
    }

    #[rstest]
    #[case::project_Linux_x86_64_suffix_tar_gz_project_Linux_x86_64_tar_gz(
        "x86_64-unknown-linux-gnu",
        false,
        &["project-Linux-x86_64-suffix.tar.gz", "project-Linux-x86_64.tar.gz"],
        None,
        Some(r"\d+\.zip"),
        "could not find a release asset matching the regex \\d+\\.zip from "
    )]
    #[case::x86_64_unknown_linux_gnu_no_assets_for_this_OS(
        "x86_64-unknown-linux-gnu",
        false,
        &["project-macOS-arm64.tar.gz", "project-Windows-i686-gnu.tar.gz"],
        None,
        None,
        "could not find a release asset for this OS (linux) from"
    )]
    #[case::i686_unknown_linux_gnu_no_assets_for_this_arch(
        "i686-unknown-linux-gnu",
        false,
        &["project-Linux-x86_64-gnu.tar.gz", "project-Windows-i686-gnu.tar.gz"],
        None,
        None,
        "could not find a release asset for this OS (linux) and architecture (x86) from"
    )]
    #[case::x86_64_unknown_linux_musl_only_one_Linux_asset_and_it_is_gnu(
        "x86_64-unknown-linux-musl",
        false,
        &["project-Linux-x86_64-gnu.tar.gz", "project-Windows-i686-gnu.tar.gz"],
        None,
        None,
        "could not find a release asset for this OS (linux), architecture (x86_64), and libc (musl) from"
    )]
    #[case::x86_64_unknown_linux_musl_no_valid_extensions(
        "x86_64-unknown-linux-musl",
        false,
        &["project-Linux-x86_64-gnu.glorp", "project-Linux-x86-64-gnu.asfasf"],
        None,
        None,
        "could not find a release asset after filtering for valid extensions from"
    )]
    #[case::x86_64_unknown_linux_musl_no_archive_files(
        "x86_64-unknown-linux-musl",
        true,
        &["project-Linux-x86_64-gnu.gz", "project-Linux-x86-64-gnu.bz", "project-Linux-x86-64-gnu"],
        None,
        None,
        "could not find a release asset after filtering for archive files (tarball or zip) from"
    )]
    #[case::x86_64_unknown_linux_musl_does_not_pick_exe_files(
        "x86_64-unknown-linux-musl",
        false,
        &["project.exe"],
        None,
        None,
        "could not find a release asset after filtering for valid extensions"
    )]
    #[case::x86_64_pc_windows_msvc_does_not_pick_AppImage_files(
        "x86_64-pc-windows-msvc",
        false,
        &["project.AppImage"],
        None,
        None,
        "could not find a release asset after filtering for valid extensions"
    )]
    #[case::aarch64_apple_darwin_does_not_pick_AppImage_files(
        "aarch64-apple-darwin",
        false,
        &["project.AppImage"],
        None,
        None,
        "could not find a release asset after filtering for valid extensions"
    )]
    #[allow(non_snake_case)]
    fn pick_asset_errors(
        #[case] platform_name: &str,
        #[case] archive_only: bool,
        #[case] asset_names: &[&str],
        #[case] matching: Option<&str>,
        #[case] matching_regex: Option<&str>,
        #[case] expect_err: &str,
    ) -> Result<()> {
        crate::test_log::init_logging();

        let platform = Platform::find(platform_name)
            .ok_or(anyhow!("invalid platform"))?
            .clone();
        let mut picker = AssetPicker {
            matching,
            matching_regex,
            platform,
            is_musl: platform_name.contains("musl"),
            archive_only,
        };

        let url = Url::parse("https://example.com")?;
        let assets = asset_names
            .iter()
            .map(|name| Asset {
                name: (*name).to_string(),
                url: url.clone(),
            })
            .collect::<Vec<_>>();

        let picked_asset = picker.pick_asset(assets);
        assert!(picked_asset.is_err());
        assert!(picked_asset
            .err()
            .map(|e| e.to_string())
            .unwrap_or_default()
            .starts_with(expect_err));

        Ok(())
    }
}
