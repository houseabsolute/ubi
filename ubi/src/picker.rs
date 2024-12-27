use std::collections::hash_map::Keys;

use crate::{
    arch::{
        aarch64_re, arm_re, macos_aarch64_re, mips64_re, mips64le_re, mips_re, mipsle_re, ppc32_re,
        ppc64_re, ppc64le_re, riscv64_re, s390x_re, sparc64_re, x86_32_re, x86_64_re,
        ALL_ARCHES_RE,
    },
    extension::Extension,
    os::{freebsd_re, fuchsia, illumos_re, linux_re, macos_re, netbsd_re, solaris_re, windows_re},
};
use anyhow::{anyhow, Result};
use itertools::Itertools;
use lazy_regex::{regex, Lazy};
use log::debug;
use platforms::{Arch, Endian, Platform, OS};
use regex::Regex;
use url::Url;

#[derive(Debug)]
pub(crate) struct AssetPicker<'a> {
    matching: Option<&'a str>,
    platform: Platform,
    is_musl: bool,
}

impl<'a> AssetPicker<'a> {
    pub(crate) fn new(matching: Option<&'a str>, platform: Platform, is_musl: bool) -> Self {
        Self {
            matching,
            platform,
            is_musl,
        }
    }

    pub(crate) fn pick_asset<'b>(&mut self, names: Keys<'b, String, Url>) -> Result<&'b str> {
        debug!("filtering out asset names that do not have a valid extension");
        let mut names = names
            .filter(|name| {
                if let Err(e) = Extension::from_path(name) {
                    debug!("skipping asset name with invalid extension, `{name}`: {e}");
                    return false;
                }
                true
            })
            .map(std::convert::AsRef::as_ref)
            .collect::<Vec<_>>();

        if names.len() == 1 {
            debug!("there is only one asset  to pick");
            return Ok(names.remove(0));
        }

        let all_names = names.iter().join(", ");

        let mut matches = self.os_matches(names);
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
        debug!("picked asset from matches named {picked}");
        Ok(picked)
    }

    fn os_matches<'b>(&self, names: Vec<&'b str>) -> Vec<&'b str> {
        let os_matcher = self.os_matcher();
        debug!(
            "matching asset names against OS using {}",
            os_matcher.as_str()
        );

        let mut matches: Vec<&'b str> = vec![];

        // This could all be done much more simply with the iterator's .find()
        // method, but then there's no place to put all the debugging output.
        for name in names {
            debug!("matching OS against asset name = {name}");

            if os_matcher.is_match(name) {
                debug!("matches our OS");
                matches.push(name);
            } else {
                debug!("does not match our OS");
            }
        }

        matches
    }

    fn arch_matches<'b>(&self, mut os_matches: Vec<&'b str>) -> Vec<&'b str> {
        let arch_matcher = self.arch_matcher();
        debug!(
            "matching asset names against CPU architecture using {}",
            arch_matcher.as_str(),
        );

        let mut matches: Vec<&str> = vec![];
        if os_matches.len() == 1 {
            debug!("there is only one asset that matches our OS");
            if arch_matcher.is_match(os_matches[0]) {
                debug!("matches our CPU architecture");
                matches.push(os_matches.remove(0));
            } else if ALL_ARCHES_RE.is_match(os_matches[0]) {
                debug!("it matches a CPU architecture which is not ours");
            } else {
                debug!("does not match any CPU architecture, so we will try it");
                matches.push(os_matches.remove(0));
            }
        } else {
            for name in &os_matches {
                debug!("matching CPU architecture against asset name = {name}",);
                if arch_matcher.is_match(name) {
                    debug!("matches our CPU architecture");
                    matches.push(name);
                } else {
                    debug!("does not match our CPU architecture");
                }
            }

            if matches.is_empty() {
                debug!("no asset names matched our CPU architecture, will look for asset names without an architecture");
                for name in os_matches {
                    debug!("matching against asset name = {name}");
                    if ALL_ARCHES_RE.is_match(name) {
                        debug!("matches a CPU architecture which is not ours");
                    } else {
                        debug!("does not match any CPU architecture, so we will try it");
                        matches.push(name);
                    }
                }
            }
        }

        matches
    }

    fn libc_matches<'b>(&mut self, matches: Vec<&'b str>) -> Vec<&'b str> {
        if !self.is_musl {
            return matches;
        }

        debug!("filtering out glibc asset names since this is a musl platform");

        let mut libc_matches: Vec<&str> = vec![];
        for name in &matches {
            debug!("checking for glibc in asset name = {}", name);
            if name.contains("-gnu") || name.contains("-glibc") {
                debug!("indicates glibc and is not compatible with a musl platform");
                continue;
            } else if name.contains("-musl") {
                debug!("indicates musl");
            } else {
                debug!("name does not indicate the libc it was compiled against");
            }

            libc_matches.push(name);
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

    fn pick_asset_from_matches<'b>(&mut self, mut matches: Vec<&'b str>) -> Result<&'b str> {
        if matches.len() == 1 {
            debug!("only found one candidate asset");
            return Ok(matches.remove(0));
        }

        let filtered = self.maybe_filter_for_64_bit_arch(matches);

        let (mut filtered, name) = self.maybe_filter_for_matching_string(filtered)?;
        if let Some(name) = name {
            return Ok(name);
        }

        if filtered.len() == 1 {
            debug!("only found one candidate asset after filtering");
            return Ok(filtered.remove(0));
        }

        let (filtered, name) = self.maybe_pick_asset_for_macos_arm(filtered);
        if let Some(name) = name {
            return Ok(name);
        }

        debug!(
            "cannot disambiguate multiple asset names, picking the first one after sorting by name"
        );
        // We don't have any other criteria we could use to pick the right
        // one, and we want to pick the same one every time.
        Ok(filtered.into_iter().sorted().next().unwrap())
    }

    fn maybe_filter_for_64_bit_arch<'b>(&self, matches: Vec<&'b str>) -> Vec<&'b str> {
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

        let names = matches.clone();
        debug!(
            "found multiple candidate asset names, filtering for 64-bit binaries in {:?}",
            names,
        );

        if !matches.iter().any(|&a| a.contains("64")) {
            debug!("no 64-bit asset names found, falling back to all asset names");
            return matches;
        }

        let sixty_four_bit = matches
            .into_iter()
            .filter(|&a| a.contains("64"))
            .collect::<Vec<_>>();
        debug!(
            "found 64-bit asset names: {}",
            sixty_four_bit.iter().join(",")
        );
        sixty_four_bit
    }

    fn maybe_filter_for_matching_string<'b>(
        &self,
        matches: Vec<&'b str>,
    ) -> Result<(Vec<&'b str>, Option<&'b str>)> {
        if self.matching.is_none() {
            return Ok((matches, None));
        }

        let m = self.matching.unwrap();
        debug!(
            r#"looking for an asset matching the string "{}" passed in --matching"#,
            m
        );
        if let Some(name) = matches.into_iter().find(|&a| a.contains(m)) {
            debug!("found an asset matching the string");
            return Ok((vec![], Some(name)));
        }

        Err(anyhow!(
            r#"could not find any asset names containing our --matching string, "{}""#,
            m,
        ))
    }

    fn maybe_pick_asset_for_macos_arm<'b>(
        &self,
        mut matches: Vec<&'b str>,
    ) -> (Vec<&'b str>, Option<&'b str>) {
        if !self.running_on_macos_arm() {
            return (matches, None);
        }

        let names = matches.iter().collect::<Vec<_>>();
        debug!(
            "found multiple candidate asset names and running on macOS ARM, filtering for arm64 binaries in {:?}",
            names,
        );

        let arch_matcher = aarch64_re();
        if let Some(idx) = matches.iter().position(|&a| arch_matcher.is_match(a)) {
            debug!("found ARM binary named {}", matches[idx]);
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
            return macos_aarch64_re();
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
    use crate::assets::{Asset, Assets};

    use super::*;
    use test_case::test_case;
    use url::Url;

    #[test_case(
        "x86_64-unknown-linux-gnu",
        &["project-Linux-x86_64.tar.gz"],
        None,
        0 ;
        "x86_64-unknown-linux-gnu - only one asset"
    )]
    #[test_case(
        "x86_64-unknown-linux-gnu",
        &["project-Linux-i686.tar.gz", "project-Linux-x86_64.tar.gz"],
        None,
        1 ;
        "x86_64-unknown-linux-gnu - pick x86-64 asset"
    )]
    #[test_case(
        "x86_64-unknown-linux-gnu",
        &["project-Linux-i686-gnu.tar.gz", "project-Linux-i686-musl.tar.gz"],
        None,
        0 ;
        "x86_64-unknown-linux-gnu - pick first asset from two matching 32-bit assets"
    )]
    #[test_case(
        "x86_64-unknown-linux-gnu",
        &["project-Linux-x86_64-gnu.tar.gz", "project-Linux-x86_64-musl.tar.gz"],
        Some("musl"),
        1 ;
        "x86_64-unknown-linux-gnu - pick asset with matching string when matching is set"
    )]
    #[test_case(
        "x86_64-unknown-linux-gnu",
        &["project-Linux-i686-gnu.tar.gz", "project-Linux-i686-musl.tar.gz"],
        Some("musl"),
        1 ;
        "x86_64-unknown-linux-gnu - pick asset with matching string from two 32-bit assets when matching is set"
    )]
    #[test_case(
        "i686-unknown-linux-gnu",
        &["project-Linux-i686.tar.gz"],
        None,
        0 ;
        "i686-unknown-linux-gnu - pick one asset"
    )]
    #[test_case(
        "i686-unknown-linux-gnu",
        &["project-Linux-i686-gnu.tar.gz", "project-Linux-i686-musl.tar.gz"],
        Some("musl"),
        1 ;
        "i686-unknown-linux-gnu - pick asset with matching string when matching is set"
    )]
    #[test_case(
        "aarch64-apple-darwin",
        &["project-Macos-aarch64.tar.gz"],
        None,
        0 ;
        "aarch64-apple-darwin - pick one asset"
    )]
    #[test_case(
        "aarch64-apple-darwin",
        &["project-Linux-x86-64.tar.gz", "project-Mac-x86-64.tar.gz"],
        None,
        1 ;
        "aarch64-apple-darwin - pick asset with 'mac' in the name"
    )]
    #[test_case(
        "aarch64-apple-darwin",
        &["project-Linux-x86-64.tar.gz", "project-Macosx-x86-64.tar.gz"],
        None,
        1 ;
        "aarch64-apple-darwin - pick asset with 'macosx' in the name"
    )]
    #[test_case(
        "aarch64-apple-darwin",
        &["project-Macos-x86-64.tar.gz", "project-Macos-aarch64.tar.gz"],
        None,
        1 ;
        "aarch64-apple-darwin - pick the aarch64 asset on macOS ARM"
    )]
    #[test_case(
        "aarch64-apple-darwin",
        &["project-Macos-x86-64.tar.gz"],
        None,
        0 ;
        "aarch64-apple-darwin - pick the x86-64 asset on macOS ARM if no aarch64 asset is available"
    )]
    #[test_case(
        "x86_64-unknown-linux-musl",
        &["project-Linux-x86_64.tar.gz"],
        None,
        0 ;
        "x86_64-unknown-linux-musl - only one asset"
    )]
    #[test_case(
        "x86_64-unknown-linux-musl",
        &["project-Linux-x86_64-gnu.tar.gz", "project-Linux-x86_64-musl.tar.gz"],
        None,
        1 ;
        "x86_64-unknown-linux-musl - pick the musl asset over gnu on a musl platform"
    )]
    #[test_case(
        "x86_64-unknown-linux-musl",
        &["project-Linux-x86_64-glibc.tar.gz", "project-Linux-x86_64-musl.tar.gz"],
        None,
        1 ;
        "x86_64-unknown-linux-musl - pick the musl asset over glibc on a musl platform"
    )]
    #[test_case(
        "x86_64-unknown-linux-musl",
        &["project-Linux-x86_64.tar.gz", "project-Linux-x86_64-musl.tar.gz"],
        None,
        1 ;
        "x86_64-unknown-linux-musl - pick the musl asset over unspecified libc on a musl platform"
    )]
    fn pick_asset(
        platform_name: &str,
        asset_names: &[&str],
        matching: Option<&str>,
        expect_idx: usize,
    ) -> Result<()> {
        // It'd be nice to use `test_log` but that doesn't work with the test-case crate. See
        // https://github.com/frondeus/test-case/pull/143.
        //
        // init_logger(log::LevelFilter::Debug)?;
        let platform = Platform::find(platform_name)
            .ok_or(anyhow!("invalid platform"))?
            .clone();
        let mut picker = AssetPicker {
            matching,
            platform,
            is_musl: platform_name.contains("musl"),
        };

        let url = Url::parse("https://example.com")?;
        let assets = asset_names
            .iter()
            .map(|&name| Asset {
                name: name.to_string(),
                url: url.clone(),
            })
            .collect::<Assets>();

        let picked_asset = picker.pick_asset(assets.keys())?;
        assert_eq!(picked_asset, asset_names[expect_idx]);

        Ok(())
    }

    #[test_case(
        "x86_64-unknown-linux-gnu",
        &["project-macOS-arm64.tar.gz", "project-Windows-i686-gnu.tar.gz"],
        None,
        "could not find a release asset for this OS (linux) from" ;
        "x86_64-unknown-linux-gnu - no assets for this OS"
    )]
    #[test_case(
        "i686-unknown-linux-gnu",
        &["project-Linux-x86_64-gnu.tar.gz", "project-Windows-i686-gnu.tar.gz"],
        None,
        "could not find a release asset for this OS (linux) and architecture (x86) from" ;
        "i686-unknown-linux-gnu - no assets for this arch"
    )]
    #[test_case(
        "x86_64-unknown-linux-musl",
        &["project-Linux-x86_64-gnu.tar.gz", "project-Windows-i686-gnu.tar.gz"],
        None,
        "could not find a release asset for this OS (linux), architecture (x86_64), and libc (musl) from" ;
        "x86_64-unknown-linux-musl - only one Linux asset and it is gnu"
    )]
    fn pick_asset_errors(
        platform_name: &str,
        asset_names: &[&str],
        matching: Option<&str>,
        expect_err: &str,
    ) -> Result<()> {
        // It'd be nice to use `test_log` but that doesn't work with the test-case crate. See
        // https://github.com/frondeus/test-case/pull/143.
        //
        // init_logger(log::LevelFilter::Debug)?;
        let platform = Platform::find(platform_name)
            .ok_or(anyhow!("invalid platform"))?
            .clone();
        let mut picker = AssetPicker {
            matching,
            platform,
            is_musl: platform_name.contains("musl"),
        };

        let url = Url::parse("https://example.com")?;
        let assets = asset_names
            .iter()
            .map(|name| Asset {
                name: (*name).to_string(),
                url: url.clone(),
            })
            .collect::<Assets>();

        let picked_asset = picker.pick_asset(assets.keys());
        assert!(picked_asset.is_err());
        assert!(picked_asset
            .err()
            .map(|e| e.to_string())
            .unwrap_or_default()
            .starts_with(expect_err));

        Ok(())
    }
}
