use itertools::Itertools;
use lazy_regex::{regex, Lazy};
use regex::Regex;
use std::sync::LazyLock;

// This is a special case to account for the fact that MacOS ARM systems can
// also run x86-64 binaries.
pub(crate) fn macos_aarch64_and_x86_64_re() -> &'static Lazy<Regex> {
    regex!(
        r"(?ix)
        (?:
            \b
            |
            _
        )
        (?:
            aarch_?64
            |
            arm_?64
            |
            arm
            |
            x86[_-]64
            |
            x64
            |
            amd64
            |
            all
        )
        (?:
            \b
            |
            _
        )
        "
    )
}

pub(crate) fn macos_aarch64_only_re() -> &'static Lazy<Regex> {
    regex!(
        r"(?ix)
        (?:
            \b
            |
            _
        )
        (?:
            aarch_?64
            |
            arm_?64
            |
            arm
        )
        (?:
            \b
            |
            _
        )
        "
    )
}

pub(crate) fn aarch64_re() -> &'static Lazy<Regex> {
    regex!(
        r"(?ix)
        (?:
            \b
            |
            _
        )
        (?:
            aarch_?64
            |
            arm_?64
        )
        (?:
            \b
            |
            _
        )
        "
    )
}

pub(crate) fn arm_re() -> &'static Lazy<Regex> {
    regex!(
        r"(?ix)
        (?:
            \b
            |
            _
        )
        arm(?:v[0-7])?
        (?:
            \b
            |
            _
        )
        "
    )
}

pub(crate) fn mipsle_re() -> &'static Lazy<Regex> {
    regex!(
        r"(?ix)
        (?:
            \b
            |
            _
        )
        mips(?:el|le)
        (?:
            \b
            |
            _
        )
        "
    )
}

pub(crate) fn mips_re() -> &'static Lazy<Regex> {
    regex!(
        r"(?ix)
        (?:
            \b
            |
            _
        )
        mips
        (?:
            \b
            |
            _
        )
        "
    )
}

pub(crate) fn mips64le_re() -> &'static Lazy<Regex> {
    regex!(
        r"(?ix)
        (?:
            \b
            |
            _
        )
        mips_?64(?:el|le)
        (?:
            \b
            |
            _
        )
        "
    )
}

pub(crate) fn mips64_re() -> &'static Lazy<Regex> {
    regex!(
        r"(?ix)
        (?:
            \b
            |
            _
        )
        mips_?64
        (?:
            \b
            |
            _
        )
        "
    )
}

pub(crate) fn ppc32_re() -> &'static Lazy<Regex> {
    regex!(
        r"(?ix)
        (?:
            \b
            |
            _
        )
        (?:
            powerpc
            |
            ppc
        )
        (?:
            \b
            |
            _
        )
        "
    )
}

pub(crate) fn ppc64_re() -> &'static Lazy<Regex> {
    regex!(
        r"(?ix)
        (?:
            \b
            |
            _
        )
        (?:
             (?:
                 powerpc64
                 |
                 ppc64
             )
             (?:be)?
             |
             (?:
                 powerpc
                 |
                 ppc
             )
             (?:be)?
             _?64
        )
        (?:
            \b
            |
            _
        )
        "
    )
}

pub(crate) fn ppc64le_re() -> &'static Lazy<Regex> {
    regex!(
        r"(?ix)
        (?:
            \b
            |
            _
        )
        (?:
             (?:
                 powerpc64
                 |
                 ppc64
             )
             le
             |
             (?:
                 powerpc
                 |
                 ppc
             )
             le
             _?64
        )
        (?:
            \b
            |
            _
        )
        "
    )
}

pub(crate) fn riscv64_re() -> &'static Lazy<Regex> {
    regex!(
        r"(?ix)
        (?:
            \b
            |
            _
        )
        # `gc` names the standard set of ISA extensions, which is what Rust's `riscv64gc` targets
        # build for.
        riscv(_?64(?:gc)?)?
        (?:
            \b
            |
            _
        )
        "
    )
}

pub(crate) fn s390x_re() -> &'static Lazy<Regex> {
    regex!(
        r"(?ix)
        (?:
            \b
            |
            _
        )
        s390x?(?:_?64)?
        (?:
            \b
            |
            _
        )
        "
    )
}

pub(crate) fn sparc64_re() -> &'static Lazy<Regex> {
    regex!(
        r"(?ix)
        (?:
            \b
            |
            _
        )
        sparc(?:_?64)?
        (?:
            \b
            |
            _
        )
        "
    )
}

pub(crate) fn x86_32_re() -> &'static Lazy<Regex> {
    regex!(
        r"(?ix)
        (?:
            \b
            |
            _
        )
        (?:
            386 | i586 | i686
            |
            x86[_-]32
            |
            # This is gross but the OS matcher will reject this on non-Windows
            # platforms.
            win32
        )
        (?:
            \b
            |
            _
        )
        "
    )
}

pub(crate) fn x86_64_re() -> &'static Lazy<Regex> {
    regex!(
        r"(?ix)
        (?:
            \b
            |
            _
        )
        (?:
            386 | i586 | i686
            |
            x86[_-]32
            |
            # The optional `v` level is a microarch level like `amd64v3`, which needs newer CPU
            # features. The picker prefers a plain build when it has a choice.
            (?:
                x86[_-]64
                |
                x64
                |
                amd64
            )
            (?:v\d+)?
            |
            linux64
            |
            # This is gross but the OS matcher will reject this on non-Windows
            # platforms.
            win64
        )
        (?:
            \b
            |
            _
        )
        "
    )
}

// This is a regex to match all the different 64-bit CPU architecture strings we might
// encounter. This is used when filtering multiple matching assets for 64-bit architectures.
pub(crate) fn cpu_64_bit_re() -> &'static Lazy<Regex> {
    regex!(
        r"(?ix)
        (?:
            \b
            |
            _
        )
        (?:
            x86[_-]?64
            |
            x64
            |
            amd64
            |
            linux64
            |
            win64
            |
            aarch[_-]?64
            |
            arm[_-]?64
            |
            mips[_-]?64
            |
            powerpc(?:le)?[_-]?64
            |
            ppc(?:le)?[_-]?64
            |
            riscv[_-]?64
            |
            s390x?[_-]?64
            |
            sparc[_-]?64

        )
        # Some names add an endianness (`ppc64le`, `mips64el`), an ISA extension (`riscv64gc`), or a
        # microarch level (`amd64v3`) right after the `64`, with no separator. We leave out Apple's
        # `arm64e`, because third-party arm64e binaries don't run on macOS without a special boot
        # setting.
        (?:le|el|gc|v\d+)?
        (?:
            \b
            |
            _
        )
        "
    )
}

// Matches an x86-64 microarch level like `amd64v3` or `x86_64_v2`. A build for a higher level
// needs newer CPU features, like AVX2 for v3, so it can crash on an older CPU.
pub(crate) fn x86_64_microarch_level_re() -> &'static Lazy<Regex> {
    regex!(
        r"(?ix)
        (?:
            \b
            |
            _
        )
        (?:
            x86[_-]?64
            |
            x64
            |
            amd64
        )
        [_-]?
        v\d+
        # Many names put a version right after the arch, like `amd64-v2.1.6`, so a `.` only ends a
        # level when an extension follows it. The regex crate has no lookahead, so we spell out
        # what may come next.
        (?:
            [_-]
            |
            \.[a-z]
            |
            $
        )
        "
    )
}

pub(crate) static ALL_ARCHES_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        &[
            aarch64_re(),
            arm_re(),
            mipsle_re(),
            mips_re(),
            mips64le_re(),
            mips64_re(),
            ppc32_re(),
            ppc64_re(),
            ppc64le_re(),
            riscv64_re(),
            s390x_re(),
            sparc64_re(),
            x86_32_re(),
            x86_64_re(),
        ]
        .iter()
        .map(|r| format!("(?:{})", r.as_str()))
        .join("|"),
    )
    .unwrap()
});

#[cfg(test)]
mod test {

    use test_case::test_case;

    #[test_case("project-linux-amd64.tar.gz", true ; "amd64")]
    #[test_case("project-linux-x86_64.tar.gz", true ; "x86_64")]
    #[test_case("project-linux-arm64.tar.gz", true ; "arm64")]
    #[test_case("project-linux-ppc64.tar.gz", true ; "ppc64")]
    #[test_case("project-linux-ppc64le.tar.gz", true ; "ppc64le")]
    #[test_case("project-linux-powerpc64le.tar.gz", true ; "powerpc64le")]
    #[test_case("project-linux-mips64le.tar.gz", true ; "mips64le")]
    #[test_case("project-linux-mips64el.tar.gz", true ; "mips64el")]
    #[test_case("project-linux-riscv64gc.tar.gz", true ; "riscv64gc")]
    #[test_case("project-linux-amd64v3.tar.gz", true ; "amd64v3")]
    #[test_case("project-linux-x86_64_v2.tar.gz", true ; "x86_64_v2")]
    #[test_case("project-linux-x86_64v2.tar.gz", true ; "x86_64v2")]
    #[test_case("project-linux-amd64v10.tar.gz", true ; "multi-digit microarch level")]
    // Third-party arm64e binaries don't run on macOS without a special boot setting, so this must
    // not count as a normal 64-bit ARM build.
    #[test_case("project-darwin-arm64e.tar.gz", false ; "arm64e")]
    #[test_case("project-1.64.5-linux-386.tar.gz", false ; "64 in version")]
    #[test_case("project-linux-i686.tar.gz", false ; "i686")]
    #[test_case("project-linux-amd64x.tar.gz", false ; "unknown suffix")]
    fn cpu_64_bit_re(name: &str, expect: bool) {
        assert_eq!(super::cpu_64_bit_re().is_match(name), expect, "{name}");
    }

    #[test_case("project-linux-x86_64.tar.gz", true ; "x86_64")]
    #[test_case("project-linux-amd64v3.tar.gz", true ; "amd64v3")]
    #[test_case("project-linux-x86_64v2.tar.gz", true ; "x86_64v2")]
    #[test_case("project-linux-x64v4.tar.gz", true ; "x64v4")]
    #[test_case("project-linux-amd64x.tar.gz", false ; "unknown suffix")]
    fn x86_64_re(name: &str, expect: bool) {
        assert_eq!(super::x86_64_re().is_match(name), expect, "{name}");
    }

    #[test_case("project-linux-riscv64.tar.gz", true ; "riscv64")]
    #[test_case("project-linux-riscv64gc.tar.gz", true ; "riscv64gc")]
    #[test_case("project-linux-riscv64x.tar.gz", false ; "unknown suffix")]
    fn riscv64_re(name: &str, expect: bool) {
        assert_eq!(super::riscv64_re().is_match(name), expect, "{name}");
    }

    #[test_case("project-linux-amd64v3.tar.gz", true ; "no separator")]
    #[test_case("project-linux-amd64_v3.tar.gz", true ; "underscore")]
    #[test_case("project-linux-x86_64-v2.tar.gz", true ; "x86_64 with hyphen")]
    #[test_case("mihomo-linux-amd64-v3-v1.19.32.gz", true ; "level then version")]
    #[test_case("mihomo-linux-amd64-v3.gz", true ; "level then extension")]
    #[test_case("project-linux-amd64-v3", true ; "level at end")]
    #[test_case("project-linux-amd64.tar.gz", false ; "plain")]
    #[test_case("syncthing-linux-amd64-v2.1.6.tar.gz", false ; "version after arch")]
    #[test_case("mihomo-linux-amd64-v1.19.32.gz", false ; "longer version after arch")]
    fn x86_64_microarch_level_re(name: &str, expect: bool) {
        assert_eq!(
            super::x86_64_microarch_level_re().is_match(name),
            expect,
            "{name}"
        );
    }

    #[test_case("project-linux-riscv64gc.tar.gz", true ; "riscv64gc")]
    #[test_case("project-linux-amd64v3.tar.gz", true ; "amd64v3")]
    fn all_arches_re(name: &str, expect: bool) {
        assert_eq!(super::ALL_ARCHES_RE.is_match(name), expect, "{name}");
    }
}
