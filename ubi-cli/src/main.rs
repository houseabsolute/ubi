use anyhow::{anyhow, Context, Result};
use clap::{builder::BoolishValueParser, Arg, ArgAction, ArgGroup, ArgMatches, Command};
use log::{debug, error};
use std::{env, path::Path, str::FromStr};
use strum::VariantNames;
use ubi::{ColorChoice, ForgeType, Ubi, UbiBuilder};

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let cmd = cmd();
    let matches = cmd.get_matches();
    let res = init_logger_from_matches(&matches);
    if let Err(e) = res {
        eprintln!("Error creating logger: {e:?}");
        std::process::exit(126);
    }

    // We use this when `--self-upgrade` is passed. We need to create this String here so that we
    // can make a Ubi<'_> instance that borrows this value. It needs to have the same lifetime as
    // `matches`. If we try to make it in `self_upgrade_ubi` we end up trying to return a reference
    // data owned by that fn.
    let ubi_exe_path = match env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            let e = anyhow!("could not find path for current executable: {e}");
            error!("{e:?}");
            std::process::exit(127);
        }
    };
    let status = match make_ubi(&matches, &ubi_exe_path) {
        Ok((mut u, post_run)) => match u.install_binary().await {
            Ok(()) => {
                if let Some(post_run) = post_run {
                    post_run();
                }
                0
            }
            Err(e) => {
                error!("{e:?}");
                1
            }
        },
        Err(e) => {
            error!("{e:?}");
            127
        }
    };
    std::process::exit(status);
}

const MAX_TERM_WIDTH: usize = 100;

#[allow(clippy::too_many_lines)]
fn cmd() -> Command {
    Command::new("ubi")
        .version(env!("CARGO_PKG_VERSION"))
        .author("Dave Rolsky <autarch@urth.org>")
        .about("The universal binary release installer")
        .arg(env_arg(
            Arg::new("project")
                .long("project")
                .short('p')
                .help(concat!(
                    "The project you want to install, like houseabsolute/precious",
                    " or https://github.com/houseabsolute/precious. You cannot pass",
                    " this when `--url` is passed.",
                )),
            "UBI_PROJECT",
        ))
        .arg(env_arg(
            Arg::new("tag")
                .long("tag")
                .short('t')
                .requires("project")
                .help(concat!(
                    "The tag to download. Defaults to the latest release.",
                    " This can only be passed with `--project`. You cannot pass this when",
                    " `--url` or `--min-age-days` are passed.",
                )),
            "UBI_TAG",
        ))
        .arg(env_arg(
            Arg::new("url")
                .long("url")
                .short('u')
                .conflicts_with_all(["tag", "project"])
                .help(concat!(
                    "The url of the file to download. This can be provided instead of a project or",
                    " tag. This will not use the forge site's API, so you will never hit its API",
                    " limits. With this parameter, you do not need to set a token env var except for",
                    " private repos. You cannot pass this when `--project`, `--tag`, or `--min-age-days`",
                    " are passed."
                )),
            "UBI_URL",
        ))
        .arg(env_arg(
            Arg::new("in")
                .long("in")
                .short('i')
                .help("The directory in which the binary should be placed. Defaults to ./bin."),
            "UBI_IN",
        ))
        .arg(env_arg(
            Arg::new("exe")
                .long("exe")
                .short('e')
                .help(concat!(
                    "The name of the file to look for in an archive file, or the name of the downloadable",
                    " file excluding its extension, e.g. `ubi.gz`. By default this is the same as the",
                    " project name, so for houseabsolute/precious we look for `precious` or",
                    " `precious.exe`. When running on Windows the `.exe` suffix will be added, as needed.",
                    " You cannot pass this when `--extract-all` is passed.",
                )),
            "UBI_EXE",
        ))
        .arg(env_arg(
            Arg::new("rename-exe-to")
                .long("rename-exe")
                .help(concat!(
                    "The name to use for the executable after it is unpacked. By default this is the same",
                    " as the name of the file passed for the `--exe` flag. If that flag isn't passed, this",
                    " is the same as the name of the project. Note that when passed, this name is used as-is,",
                    " so on Windows, `.exe` will not be appended to the name given. You cannot pass this",
                    " when `--extract-all` is passed.",
                )),
            "UBI_RENAME_EXE",
        ))
        .arg(bool_env_arg(
            Arg::new("extract-all")
                .long("extract-all")
                .action(ArgAction::SetTrue)
                // `SetTrue` defaults to a value parser that only accepts "true" and "false". That's
                // too strict for an env var, where people will reasonably write `UBI_EXTRACT_ALL=1`.
                .value_parser(BoolishValueParser::new())
                .conflicts_with_all(["exe", "rename-exe-to"])
                .help(concat!(
                    "Pass this to tell `ubi` to extract all files from the archive. By default",
                    " `ubi` will only extract an executable from an archive file. But if this is",
                    " true, it will simply unpack the archive file. If all of the contents of the",
                    " archive file share a top-level directory, that directory will be removed",
                    " during unpacking. In other words, if an archive contains",
                    " `./project/some-file` and `./project/docs.md`, it will extract them as",
                    " `some-file` and `docs.md`. You cannot pass this when `--exe` or",
                    " `--rename-exe` are passed.",
                )),
            "UBI_EXTRACT_ALL",
        ))
        .arg(env_arg(
            Arg::new("min-age-days")
                .long("min-age-days")
                .value_parser(clap::value_parser!(u32))
                .requires("project")
                .conflicts_with_all(["tag", "url"])
                .help(concat!(
                    "Minimum age in days for releases. Only releases at least this many days old",
                    " will be installed. This is useful for mitigating supply chain attacks. It's",
                    " especially useful for projects that use GitHub's immutable releases",
                    " feature. You cannot pass this with --tag or --url.",
                )),
            "UBI_MIN_AGE_DAYS",
        ))
        .arg(env_arg(
            Arg::new("matching")
                .long("matching")
                .short('m')
                .help(concat!(
                    "A string that will be matched against the release filename when there are",
                    " multiple matching files for your OS/arch. For example, there may be",
                    " multiple releases for an OS/arch that differ by compiler (MSVC vs. gcc)",
                    " or linked libc (glibc vs. musl). Note that this will be ignored if there",
                    " is only one matching release filename for your OS/arch.",
                )),
            "UBI_MATCHING",
        ))
        .arg(env_arg(
            Arg::new("matching-regex")
                .long("matching-regex")
                .short('r')
                .help(concat!(
                    "A regular expression string that will be matched against release filenames before",
                    " matching against your OS/arch. If the pattern yields a single match, that release",
                    " will be selected. If no matches are found, this will result in an error.",
                )),
            "UBI_MATCHING_REGEX",
        ))
        .arg(env_arg(
            Arg::new("forge")
                .long("forge")
                .value_parser(clap::builder::PossibleValuesParser::new(
                    ForgeType::VARIANTS,
                ))
                .help(concat!(
                    "The forge to use. If this isn't passed, then the value of `--project` or `--url`",
                    " will be checked for gitlab.com. If this contains any other domain _or_ if it",
                    " does not have a domain at all, then the default is GitHub.",
                )),
            "UBI_FORGE",
        ))
        .arg(env_arg(
            Arg::new("api-base-url")
                .long("api-base-url")
                .help(concat!(
                    "The base URL for the forge site's API. This is useful for testing or if you want",
                    " to operate against an Enterprise version of GitHub or GitLab. This should be",
                    " something like `https://github.my-corp.example.com/api/v4`.",
                )),
            "UBI_API_BASE_URL",
        ))
        .arg(env_arg(
            Arg::new("color")
                .long("color")
                .value_parser(clap::builder::PossibleValuesParser::new(
                    ColorChoice::VARIANTS,
                ))
                .default_value("auto")
                .help(concat!(
                    "When to colorize output. The default, `auto`, colorizes output when stderr is",
                    " a terminal and the `NO_COLOR` environment variable is not set. Setting",
                    " `NO_COLOR` to the empty string or to `0` is treated as not setting it at all.",
                )),
            "UBI_COLOR",
        ))
        .arg(
            Arg::new("self-upgrade")
                .long("self-upgrade")
                .conflicts_with_all(["exe", "extract-all", "forge", "in", "project", "tag", "url"])
                .action(ArgAction::SetTrue)
                .help("Use ubi to upgrade to the latest version of ubi."),
        )
        .arg(
            Arg::new("verbose")
                .short('v')
                .long("verbose")
                .action(ArgAction::SetTrue)
                .help("Enable verbose output."),
        )
        .arg(
            Arg::new("debug")
                .short('d')
                .long("debug")
                .action(ArgAction::SetTrue)
                .help("Enable debugging output."),
        )
        .arg(
            Arg::new("quiet")
                .short('q')
                .long("quiet")
                .action(ArgAction::SetTrue)
                .help("Suppresses most output."),
        )
        .group(
            ArgGroup::new("require one of")
                .args(["project", "url", "self-upgrade"])
                .required(true),
        )
        .group(
            ArgGroup::new("log-level")
                .args(["verbose", "debug", "quiet"]),
        )
        .max_term_width(MAX_TERM_WIDTH)
}

/// `Arg::env` reads the env var when it's called and treats a var set to the empty string as a
/// present-but-empty value. We want an empty var to mean "not set at all", so that a script can do
/// `UBI_TAG=$SOME_VAR ubi ...` without caring whether `$SOME_VAR` is empty. Since `Arg::env` reads
/// the var here and now, we get that behavior by simply not attaching the env var when it's empty.
fn env_arg(arg: Arg, name: &'static str) -> Arg {
    if env::var_os(name).is_some_and(|v| v.is_empty()) {
        arg
    } else {
        arg.env(name)
    }
}

/// clap treats an arg as _present_ whenever its env var is attached and set, and a present arg
/// participates in `conflicts_with_all` checks regardless of its parsed boolean value. So for a
/// boolean flag like `--extract-all`, attaching the env var for a falsy value (e.g.
/// `UBI_EXTRACT_ALL=0`) would still make clap treat `--extract-all` as present and trigger
/// conflicts with `--exe`/`--rename-exe-to`, even though the flag's parsed value is `false`. To
/// avoid that, we don't attach the env var at all when its value is empty or falsy, which mirrors
/// what `env_arg` already does for the empty case.
///
/// The falsy literals mirror clap's own `FALSE_LITERALS` in
/// `clap_builder::util::str_to_bool`, which are `pub(crate)` and so can't be imported directly.
/// This list must be kept in sync with that one.
fn bool_env_arg(arg: Arg, name: &'static str) -> Arg {
    const FALSE_LITERALS: &[&str] = &["n", "no", "f", "false", "off", "0"];
    if env::var_os(name).is_some_and(|v| {
        let v = v.to_string_lossy();
        v.is_empty() || FALSE_LITERALS.contains(&v.to_lowercase().as_str())
    }) {
        arg
    } else {
        arg.env(name)
    }
}

pub(crate) fn init_logger_from_matches(matches: &ArgMatches) -> Result<()> {
    let level = if matches.get_flag("debug") {
        log::LevelFilter::Debug
    } else if matches.get_flag("verbose") {
        log::LevelFilter::Info
    } else if matches.get_flag("quiet") {
        log::LevelFilter::Error
    } else {
        log::LevelFilter::Warn
    };

    let color = match matches.get_one::<String>("color") {
        Some(c) => ColorChoice::from_str(c)
            .with_context(|| format!("failed to parse color choice: {c}"))?,
        None => ColorChoice::default(),
    };

    ubi::init_logger(level, color).context("failed to initialize the logger")
}

fn make_ubi<'a>(
    matches: &'a ArgMatches,
    ubi_exe_path: &'a Path,
) -> Result<(Ubi<'a>, Option<impl FnOnce()>)> {
    if matches.get_flag("self-upgrade") {
        return self_upgrade_ubi(ubi_exe_path);
    }

    let mut builder = UbiBuilder::new();
    if let Some(p) = matches.get_one::<String>("project") {
        builder = builder.project(p);
    }
    if let Some(t) = matches.get_one::<String>("tag") {
        builder = builder.tag(t);
    }
    if let Some(u) = matches.get_one::<String>("url") {
        builder = builder.url(u);
    }
    if let Some(dir) = matches.get_one::<String>("in") {
        builder = builder.install_dir(dir);
    }
    if let Some(m) = matches.get_one::<String>("matching") {
        builder = builder.matching(m);
    }
    if let Some(r) = matches.get_one::<String>("matching-regex") {
        builder = builder.matching_regex(r);
    }
    if let Some(e) = matches.get_one::<String>("exe") {
        builder = builder.exe(e);
    }
    if let Some(e) = matches.get_one::<String>("rename-exe-to") {
        builder = builder.rename_exe_to(e);
    }
    if matches.get_flag("extract-all") {
        builder = builder.extract_all();
    }
    if let Some(ft) = matches.get_one::<String>("forge") {
        builder = builder.forge(
            ForgeType::from_str(ft).with_context(|| format!("failed to parse forge type: {ft}"))?,
        );
    }
    if let Some(url) = matches.get_one::<String>("api-base-url") {
        builder = builder.api_base_url(url);
    }
    if let Some(days) = matches.get_one::<u32>("min-age-days") {
        builder = builder.min_age_days(*days);
    }

    Ok((builder.build()?, None))
}

fn self_upgrade_ubi(ubi_exe_path: &Path) -> Result<(Ubi<'_>, Option<impl FnOnce()>)> {
    let ubi =
        UbiBuilder::new()
            .project("houseabsolute/ubi")
            .install_dir(ubi_exe_path.parent().ok_or_else(|| {
                anyhow!("executable path `{}` has no parent", ubi_exe_path.display())
            })?)
            .build()?;

    let post_run = if cfg!(target_os = "windows") {
        let mut old_exe = ubi_exe_path.to_path_buf();
        old_exe.set_file_name("ubi-old.exe");
        debug!(
            "renaming {} to {}",
            ubi_exe_path.display(),
            old_exe.display()
        );
        std::fs::rename(ubi_exe_path, &old_exe).with_context(|| {
            format!(
                "failed to rename {} to {}",
                ubi_exe_path.display(),
                old_exe.display()
            )
        })?;
        Some(move || {
            println!(
                "The self-upgrade operation left an old binary behind that must be deleted manually: {}",
                old_exe.display(),
            );
        })
    } else {
        None
    };

    Ok((ubi, post_run))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use std::ffi::OsString;

    /// Every env var the CLI knows about. The test guard clears all of these before each test so
    /// that a variable set in the developer's own shell cannot influence the results.
    const ALL_ENV_VARS: &[&str] = &[
        "UBI_API_BASE_URL",
        "UBI_COLOR",
        "UBI_EXE",
        "UBI_EXTRACT_ALL",
        "UBI_FORGE",
        "UBI_IN",
        "UBI_MATCHING",
        "UBI_MATCHING_REGEX",
        "UBI_MIN_AGE_DAYS",
        "UBI_PROJECT",
        "UBI_RENAME_EXE",
        "UBI_TAG",
        "UBI_URL",
    ];

    /// Clears all `UBI_*` vars, sets the given ones, and restores the original environment when
    /// dropped. Tests using this must be `#[serial]`, since the environment is process-global.
    struct EnvGuard {
        saved: Vec<(&'static str, Option<OsString>)>,
    }

    impl EnvGuard {
        fn new(vars: &[(&str, &str)]) -> Self {
            let saved = ALL_ENV_VARS
                .iter()
                .map(|name| (*name, env::var_os(name)))
                .collect();
            for name in ALL_ENV_VARS {
                env::remove_var(name);
            }
            for (name, value) in vars {
                debug_assert!(
                    ALL_ENV_VARS.contains(name),
                    "{name} is not listed in ALL_ENV_VARS; add it there so EnvGuard can restore it",
                );
                env::set_var(name, value);
            }
            Self { saved }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            for (name, value) in &self.saved {
                match value {
                    Some(v) => env::set_var(name, v),
                    None => env::remove_var(name),
                }
            }
        }
    }

    #[test]
    #[serial]
    fn tag_from_env() {
        let _guard = EnvGuard::new(&[("UBI_TAG", "v1.2.3")]);
        let matches = cmd()
            .try_get_matches_from(["ubi", "--project", "houseabsolute/precious"])
            .unwrap();
        assert_eq!(
            matches.get_one::<String>("tag").map(String::as_str),
            Some("v1.2.3"),
        );
    }

    #[test]
    #[serial]
    fn tag_flag_overrides_env() {
        let _guard = EnvGuard::new(&[("UBI_TAG", "v1.2.3")]);
        let matches = cmd()
            .try_get_matches_from([
                "ubi",
                "--project",
                "houseabsolute/precious",
                "--tag",
                "v4.5.6",
            ])
            .unwrap();
        assert_eq!(
            matches.get_one::<String>("tag").map(String::as_str),
            Some("v4.5.6"),
        );
    }

    #[test]
    #[serial]
    fn empty_tag_env_is_unset() {
        let _guard = EnvGuard::new(&[("UBI_TAG", "")]);
        let matches = cmd()
            .try_get_matches_from(["ubi", "--project", "houseabsolute/precious"])
            .unwrap();
        assert_eq!(matches.get_one::<String>("tag"), None);
    }

    #[test]
    #[serial]
    fn string_values_from_env() {
        let _guard = EnvGuard::new(&[
            ("UBI_PROJECT", "houseabsolute/precious"),
            ("UBI_IN", "/usr/local/bin"),
            ("UBI_EXE", "precious"),
            ("UBI_RENAME_EXE", "prec"),
            ("UBI_MATCHING", "musl"),
            ("UBI_MATCHING_REGEX", "musl$"),
            ("UBI_FORGE", "gitlab"),
            ("UBI_API_BASE_URL", "https://example.com/api/v4"),
        ]);
        let matches = cmd().try_get_matches_from(["ubi"]).unwrap();

        let expect = [
            ("project", "houseabsolute/precious"),
            ("in", "/usr/local/bin"),
            ("exe", "precious"),
            ("rename-exe-to", "prec"),
            ("matching", "musl"),
            ("matching-regex", "musl$"),
            ("forge", "gitlab"),
            ("api-base-url", "https://example.com/api/v4"),
        ];
        for (id, value) in expect {
            assert_eq!(
                matches.get_one::<String>(id).map(String::as_str),
                Some(value),
                "{id} from env",
            );
        }
    }

    #[test]
    #[serial]
    fn url_from_env() {
        let _guard = EnvGuard::new(&[("UBI_URL", "https://example.com/some/file.tar.gz")]);
        let matches = cmd().try_get_matches_from(["ubi"]).unwrap();
        assert_eq!(
            matches.get_one::<String>("url").map(String::as_str),
            Some("https://example.com/some/file.tar.gz"),
        );
    }

    #[test]
    #[serial]
    fn min_age_days_from_env() {
        let _guard = EnvGuard::new(&[
            ("UBI_PROJECT", "houseabsolute/precious"),
            ("UBI_MIN_AGE_DAYS", "7"),
        ]);
        let matches = cmd().try_get_matches_from(["ubi"]).unwrap();
        assert_eq!(matches.get_one::<u32>("min-age-days"), Some(&7));
    }

    #[test]
    #[serial]
    fn invalid_min_age_days_from_env_is_an_error() {
        let _guard = EnvGuard::new(&[
            ("UBI_PROJECT", "houseabsolute/precious"),
            ("UBI_MIN_AGE_DAYS", "not-a-number"),
        ]);
        let err = cmd().try_get_matches_from(["ubi"]).unwrap_err();
        assert_eq!(err.kind(), clap::error::ErrorKind::ValueValidation);
    }

    #[test]
    #[serial]
    fn empty_env_vars_are_all_unset() {
        let vars = ALL_ENV_VARS.iter().map(|n| (*n, "")).collect::<Vec<_>>();
        let _guard = EnvGuard::new(&vars);
        let matches = cmd()
            .try_get_matches_from(["ubi", "--project", "houseabsolute/precious"])
            .unwrap();

        for id in [
            "tag",
            "url",
            "in",
            "exe",
            "rename-exe-to",
            "matching",
            "matching-regex",
            "forge",
            "api-base-url",
        ] {
            assert_eq!(matches.get_one::<String>(id), None, "{id} is unset");
        }
        assert_eq!(matches.get_one::<u32>("min-age-days"), None);
    }

    #[test]
    #[serial]
    fn extract_all_from_env() {
        for value in ["1", "true", "yes", "on"] {
            let _guard = EnvGuard::new(&[("UBI_EXTRACT_ALL", value)]);
            let matches = cmd()
                .try_get_matches_from(["ubi", "--project", "houseabsolute/precious"])
                .unwrap();
            assert!(matches.get_flag("extract-all"), "UBI_EXTRACT_ALL={value}");
        }
    }

    #[test]
    #[serial]
    fn falsy_extract_all_from_env() {
        for value in ["0", "false", "no", "off", "n", "f", "FALSE", "Off", ""] {
            let _guard = EnvGuard::new(&[("UBI_EXTRACT_ALL", value)]);
            let matches = cmd()
                .try_get_matches_from(["ubi", "--project", "houseabsolute/precious"])
                .unwrap();
            assert!(!matches.get_flag("extract-all"), "UBI_EXTRACT_ALL={value}");
        }
    }

    #[test]
    #[serial]
    fn falsy_extract_all_from_env_does_not_conflict_with_exe() {
        for value in ["0", "false", "no", "off", "n", "f", "FALSE", "Off", ""] {
            let _guard = EnvGuard::new(&[("UBI_EXTRACT_ALL", value)]);
            let matches = cmd()
                .try_get_matches_from(["ubi", "--project", "houseabsolute/precious", "--exe", "x"])
                .unwrap_or_else(|e| panic!("UBI_EXTRACT_ALL={value} should not error: {e}"));
            assert!(!matches.get_flag("extract-all"), "UBI_EXTRACT_ALL={value}");
        }
    }

    #[test]
    #[serial]
    fn truthy_extract_all_from_env_still_conflicts_with_exe() {
        for value in ["1", "true", "yes", "on"] {
            let _guard = EnvGuard::new(&[("UBI_EXTRACT_ALL", value)]);
            let err = cmd()
                .try_get_matches_from(["ubi", "--project", "houseabsolute/precious", "--exe", "x"])
                .unwrap_err();
            assert_eq!(
                err.kind(),
                clap::error::ErrorKind::ArgumentConflict,
                "UBI_EXTRACT_ALL={value}",
            );
        }
    }

    #[test]
    #[serial]
    fn invalid_extract_all_from_env_is_an_error() {
        let _guard = EnvGuard::new(&[("UBI_EXTRACT_ALL", "banana")]);
        let err = cmd()
            .try_get_matches_from(["ubi", "--project", "houseabsolute/precious"])
            .unwrap_err();
        assert_eq!(err.kind(), clap::error::ErrorKind::ValueValidation);
    }

    #[test]
    #[serial]
    fn extract_all_flag_works_without_env() {
        let _guard = EnvGuard::new(&[]);
        let matches = cmd()
            .try_get_matches_from([
                "ubi",
                "--project",
                "houseabsolute/precious",
                "--extract-all",
            ])
            .unwrap();
        assert!(matches.get_flag("extract-all"));
    }

    #[test]
    #[serial]
    fn project_from_env_satisfies_required_group() {
        let _guard = EnvGuard::new(&[("UBI_PROJECT", "houseabsolute/precious")]);
        let matches = cmd().try_get_matches_from(["ubi"]).unwrap();
        assert_eq!(
            matches.get_one::<String>("project").map(String::as_str),
            Some("houseabsolute/precious"),
        );
    }

    #[test]
    #[serial]
    fn no_project_or_url_anywhere_is_an_error() {
        let _guard = EnvGuard::new(&[]);
        let err = cmd().try_get_matches_from(["ubi"]).unwrap_err();
        assert_eq!(err.kind(), clap::error::ErrorKind::MissingRequiredArgument);
    }

    #[test]
    #[serial]
    fn url_from_env_conflicts_with_project_flag() {
        let _guard = EnvGuard::new(&[("UBI_URL", "https://example.com/some/file.tar.gz")]);
        let err = cmd()
            .try_get_matches_from(["ubi", "--project", "houseabsolute/precious"])
            .unwrap_err();
        assert_eq!(err.kind(), clap::error::ErrorKind::ArgumentConflict);
    }

    #[test]
    #[serial]
    fn env_vars_are_attached_to_exactly_the_expected_args() {
        let _guard = EnvGuard::new(&[]);
        let mut got = cmd()
            .get_arguments()
            .filter_map(|a| a.get_env().map(|e| e.to_string_lossy().into_owned()))
            .collect::<Vec<_>>();
        got.sort();
        assert_eq!(got, ALL_ENV_VARS);
    }

    #[test]
    #[serial]
    fn color_defaults_to_auto() {
        let _guard = EnvGuard::new(&[]);
        let matches = cmd()
            .try_get_matches_from(["ubi", "--project", "houseabsolute/precious"])
            .unwrap();
        assert_eq!(
            matches.get_one::<String>("color").map(String::as_str),
            Some("auto"),
        );
    }

    #[test]
    #[serial]
    fn color_from_env_and_flag() {
        for value in ColorChoice::VARIANTS {
            let _guard = EnvGuard::new(&[("UBI_COLOR", *value)]);
            let matches = cmd()
                .try_get_matches_from(["ubi", "--project", "houseabsolute/precious"])
                .unwrap();
            assert_eq!(
                matches.get_one::<String>("color").map(String::as_str),
                Some(*value),
                "UBI_COLOR={value}",
            );
        }

        // The flag wins over the environment variable.
        let _guard = EnvGuard::new(&[("UBI_COLOR", "never")]);
        let matches = cmd()
            .try_get_matches_from([
                "ubi",
                "--project",
                "houseabsolute/precious",
                "--color",
                "always",
            ])
            .unwrap();
        assert_eq!(
            matches.get_one::<String>("color").map(String::as_str),
            Some("always"),
        );
    }

    #[test]
    #[serial]
    fn invalid_color_is_an_error() {
        let _guard = EnvGuard::new(&[]);
        let err = cmd()
            .try_get_matches_from([
                "ubi",
                "--project",
                "houseabsolute/precious",
                "--color",
                "sometimes",
            ])
            .unwrap_err();
        assert_eq!(err.kind(), clap::error::ErrorKind::InvalidValue);
    }
}
