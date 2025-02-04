use itertools::Itertools;
use lazy_regex::{regex, Lazy};
use regex::Regex;

// This is a special case to account for the fact that MacOS ARM systems can
// also run x86-64 binaries.
pub(crate) fn macos_aarch64_re() -> &'static Lazy<Regex> {
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
            x86[_-]64
            |
            x64
            |
            amd64
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
        riscv(_?64)?
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
            x86[_-]64
            |
            x64
            |
            amd64
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

pub(crate) static ALL_ARCHES_RE: Lazy<Regex> = Lazy::new(|| {
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
