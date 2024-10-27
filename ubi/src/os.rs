use itertools::Itertools;
use lazy_regex::{regex, Lazy};
use regex::Regex;

pub(crate) fn freebsd_re() -> &'static Lazy<Regex> {
    regex!(r"(?i:(?:\b|_)freebsd(?:\b|_))")
}

pub(crate) fn fuchsia() -> &'static Lazy<Regex> {
    regex!(r"(?i:(?:\b|_)fuchsia(?:\b|_))")
}

pub(crate) fn illumos_re() -> &'static Lazy<Regex> {
    regex!(r"(?i:(?:\b|_)illumos(?:\b|_))")
}

pub(crate) fn linux_re() -> &'static Lazy<Regex> {
    regex!(r"(?i:(?:\b|_)linux(?:\b|_|32|64))")
}

pub(crate) fn macos_re() -> &'static Lazy<Regex> {
    regex!(r"(?i:(?:\b|_)(?:darwin|macos|osx)(?:\b|_))")
}

pub(crate) fn netbsd_re() -> &'static Lazy<Regex> {
    regex!(r"(?i:(?:\b|_)netbsd(?:\b|_))")
}

pub(crate) fn solaris_re() -> &'static Lazy<Regex> {
    regex!(r"(?i:(?:\b|_)solaris(?:\b|_))")
}

pub(crate) fn windows_re() -> &'static Lazy<Regex> {
    regex!(r"(?i:(?:\b|_)win(?:32|64|dows)?(?:\b|_))")
}

pub(crate) fn all_oses_re() -> Regex {
    Regex::new(
        &[
            freebsd_re(),
            fuchsia(),
            illumos_re(),
            linux_re(),
            macos_re(),
            netbsd_re(),
            solaris_re(),
            windows_re(),
        ]
        .iter()
        .map(|r| format!("({})", r.as_str()))
        .join("|"),
    )
    .unwrap()
}
