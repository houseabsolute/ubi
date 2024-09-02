use crate::{
    arch::{
        aarch64_re, all_arches_re, arm_re, macos_aarch64_re, mips64_re, mips64le_re, mips_re,
        mipsle_re, ppc32_re, ppc64_re, ppc64le_re, riscv64_re, s390x_re, sparc64_re, x86_32_re,
        x86_64_re,
    },
    extension::Extension,
    release::Asset,
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
    platform: &'a Platform,
}

impl<'a> AssetPicker<'a> {
    pub(crate) fn new(matching: Option<&'a str>, platform: &'a Platform) -> Self {
        Self { matching, platform }
    }

    pub(crate) fn pick_asset(&self, assets: Vec<Asset>) -> Result<Asset> {
        debug!("filtering out assets that do not have a valid extension");
        let mut assets: Vec<_> = assets
            .into_iter()
            .filter(|a| {
                if Extension::from_path(&a.name).is_err() {
                    debug!("skipping asset with invalid extension: {}", a.name);
                    return false;
                }
                true
            })
            .collect();

        if assets.len() == 1 {
            debug!("there is only one asset to pick");
            return Ok(assets.remove(0));
        }

        let all_names = assets.iter().map(|a| &a.name).join(", ");

        let os_matches = self.os_matches(assets);
        if os_matches.is_empty() {
            return Err(anyhow!(
                "could not find a release for this OS ({}) from {all_names}",
                self.platform,
            ));
        }

        let matches = self.arch_matches(os_matches);
        if matches.is_empty() {
            return Err(anyhow!(
                "could not find a release for this OS and architecture ({}) from {all_names}",
                self.platform,
            ));
        }

        let picked = self.pick_asset_from_matches(matches)?;
        debug!("picked asset from matches named {}", picked.name);
        Ok(picked)
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
            } else if all_arches_re().is_match(&os_matches[0].name) {
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
                    if all_arches_re().is_match(&asset.name) {
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

    fn pick_asset_from_matches(&self, mut matches: Vec<Asset>) -> Result<Asset> {
        if matches.len() == 1 {
            debug!("only found one candidate asset");
            return Ok(matches.remove(0));
        }

        let filtered = self.maybe_filter_for_64_bit_arch(matches);

        let (mut filtered, asset) = self.maybe_filter_for_matching_string(filtered)?;
        if let Some(asset) = asset {
            return Ok(asset);
        }

        if filtered.len() == 1 {
            debug!("only found one candidate asset after filtering");
            return Ok(filtered.remove(0));
        }

        let (filtered, asset) = self.maybe_pick_asset_for_macos_arm(filtered);
        if let Some(asset) = asset {
            return Ok(asset);
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
        debug!(
            "found multiple candidate assets, filtering for 64-bit binaries in {:?}",
            asset_names,
        );

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

    fn maybe_filter_for_matching_string(
        &self,
        matches: Vec<Asset>,
    ) -> Result<(Vec<Asset>, Option<Asset>)> {
        if self.matching.is_none() {
            return Ok((matches, None));
        }

        let m = self.matching.unwrap();
        debug!(
            r#"looking for an asset matching the string "{}" passed in --matching"#,
            m
        );
        if let Some(asset) = matches.into_iter().find(|a| a.name.contains(m)) {
            debug!("found an asset matching the string");
            return Ok((vec![], Some(asset)));
        }

        Err(anyhow!(
            r#"could not find any assets containing our --matching string, "{}""#,
            m,
        ))
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
            "found multiple candidate assets and running on macOS ARM, filtering for arm64 binaries in {:?}",
            asset_names,
        );

        let arch_matcher = aarch64_re();

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
            OS::FreeBSD => regex!(r"(?i:(?:\b|_)freebsd(?:\b|_))"),
            OS::Fuchsia => regex!(r"(?i:(?:\b|_)fuchsia(?:\b|_))"),
            //OS::Haiku => regex!(r"(?i:(?:\b|_)haiku(?:\b|_))"),
            OS::IllumOS => regex!(r"(?i:(?:\b|_)illumos(?:\b|_))"),
            OS::Linux => regex!(r"(?i:(?:\b|_)linux(?:\b|_|32|64))"),
            OS::MacOS => regex!(r"(?i:(?:\b|_)(?:darwin|macos|osx)(?:\b|_))"),
            OS::NetBSD => regex!(r"(?i:(?:\b|_)netbsd(?:\b|_))"),
            //OS::OpenBSD => regex!(r"(?i:(?:\b|_)openbsd(?:\b|_))"),
            OS::Solaris => regex!(r"(?i:(?:\b|_)solaris(?:\b|_))"),
            //OS::VxWorks => regex!(r"(?i:(?:\b|_)vxworks(?:\b|_))"),
            OS::Windows => regex!(r"(?i:(?:\b|_)win(?:32|64|dows)?(?:\b|_))"),
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
    use super::*;
    use platforms::PlatformReq;
    use std::str::FromStr;
    use url::Url;

    #[test]
    fn pick_asset_from_matches_64_bit_platform() -> Result<()> {
        let req = PlatformReq::from_str("x86_64-unknown-linux-musl")?;
        let platform = req.matching_platforms().next().unwrap();
        let picker = AssetPicker {
            matching: None,
            platform,
        };

        {
            let assets = vec![Asset {
                name: "project-Linux-i686.tar.gz".to_string(),
                url: Url::parse("https://example.com")?,
            }];
            let picked_asset = picker.pick_asset_from_matches(assets.clone())?;
            assert_eq!(picked_asset, assets[0], "only one asset, so pick that one");
        }

        {
            let assets = vec![
                Asset {
                    name: "project-Linux-x86_64.tar.gz".to_string(),
                    url: Url::parse("https://example.com")?,
                },
                Asset {
                    name: "project-Linux-i686.tar.gz".to_string(),
                    url: Url::parse("https://example.com")?,
                },
            ];
            let picked_asset = picker.pick_asset_from_matches(assets.clone())?;
            assert_eq!(picked_asset, assets[0], "pick the x86_64 asset on x86_64");
        }

        {
            let assets = vec![
                Asset {
                    name: "project-Linux-i686-gnu.tar.gz".to_string(),
                    url: Url::parse("https://example.com")?,
                },
                Asset {
                    name: "project-Linux-i686-musl.tar.gz".to_string(),
                    url: Url::parse("https://example.com")?,
                },
            ];
            let picked_asset = picker.pick_asset_from_matches(assets.clone())?;
            assert_eq!(
                picked_asset, assets[0],
                "pick the first matching asset from two 32-bit assets"
            );
        }

        let picker = AssetPicker {
            matching: Some("musl"),
            platform,
        };

        {
            let assets = vec![
                Asset {
                    name: "project-Linux-x86_64-gnu.tar.gz".to_string(),
                    url: Url::parse("https://example.com")?,
                },
                Asset {
                    name: "project-Linux-x86_64-musl.tar.gz".to_string(),
                    url: Url::parse("https://example.com")?,
                },
            ];
            let picked_asset = picker.pick_asset_from_matches(assets.clone())?;
            assert_eq!(
                picked_asset, assets[1],
                "pick the musl asset when matching is set"
            );
        }

        {
            let assets = vec![
                Asset {
                    name: "project-Linux-i686-gnu.tar.gz".to_string(),
                    url: Url::parse("https://example.com")?,
                },
                Asset {
                    name: "project-Linux-i686-musl.tar.gz".to_string(),
                    url: Url::parse("https://example.com")?,
                },
            ];
            let picked_asset = picker.pick_asset_from_matches(assets.clone())?;
            assert_eq!(
                picked_asset, assets[1],
                "pick the musl asset from two 32-bit assets"
            );
        }

        Ok(())
    }

    #[test]
    fn pick_asset_from_matches_32_bit_platform() -> Result<()> {
        let req = PlatformReq::from_str("i686-unknown-linux-gnu")?;
        let platform = req.matching_platforms().next().unwrap();
        let picker = AssetPicker {
            matching: None,
            platform,
        };

        {
            let assets = vec![Asset {
                name: "project-Linux-i686.tar.gz".to_string(),
                url: Url::parse("https://example.com")?,
            }];
            let picked_asset = picker.pick_asset_from_matches(assets.clone())?;
            assert_eq!(picked_asset, assets[0], "only one asset, so pick that one");
        }

        let picker = AssetPicker {
            matching: Some("musl"),
            platform,
        };

        {
            let assets = vec![
                Asset {
                    name: "project-Linux-i686-gnu.tar.gz".to_string(),
                    url: Url::parse("https://example.com")?,
                },
                Asset {
                    name: "project-Linux-i686-musl.tar.gz".to_string(),
                    url: Url::parse("https://example.com")?,
                },
            ];
            let picked_asset = picker.pick_asset_from_matches(assets.clone())?;
            assert_eq!(
                picked_asset, assets[1],
                "pick the musl asset when matching is set"
            );
        }

        Ok(())
    }

    #[test]
    fn pick_asset_from_matches_macos_arm() -> Result<()> {
        //init_logger(log::LevelFilter::Debug)?;
        let req = PlatformReq::from_str("aarch64-apple-darwin")?;
        let platform = req.matching_platforms().next().unwrap();
        let picker = AssetPicker {
            matching: None,
            platform,
        };

        {
            let assets = vec![Asset {
                name: "project-Macos-aarch64.tar.gz".to_string(),
                url: Url::parse("https://example.com")?,
            }];
            let picked_asset = picker.pick_asset_from_matches(assets.clone())?;
            assert_eq!(picked_asset, assets[0], "only one asset, so pick that one");
        }

        {
            let assets = vec![
                Asset {
                    name: "project-Macos-x86-64.tar.gz".to_string(),
                    url: Url::parse("https://example.com")?,
                },
                Asset {
                    name: "project-Macos-aarch64.tar.gz".to_string(),
                    url: Url::parse("https://example.com")?,
                },
            ];
            let picked_asset = picker.pick_asset_from_matches(assets.clone())?;
            assert_eq!(
                picked_asset, assets[1],
                "pick the aarch64 asset on macOS ARM"
            );
        }

        {
            let assets = vec![Asset {
                name: "project-Macos-x86-64.tar.gz".to_string(),
                url: Url::parse("https://example.com")?,
            }];
            let picked_asset = picker.pick_asset_from_matches(assets.clone())?;
            assert_eq!(
                picked_asset, assets[0],
                "pick the x86-64 asset on macOS ARM if no aarch64 asset is available"
            );
        }

        Ok(())
    }
}
