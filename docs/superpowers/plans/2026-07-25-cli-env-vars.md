# CLI Env Var Support Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development
> (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let each of `ubi`'s twelve install-configuration CLI flags be set through a corresponding
`UBI_*` environment variable, where an empty variable means "not set".

**Architecture:** All changes are in the CLI crate. A small `env_arg` helper wraps each `Arg` in
`cmd()` and attaches clap's native `Arg::env` — but only when the variable is not set to the empty
string, which is how we get "empty means unset" out of a library that has no such option. Nothing
downstream of `get_matches` changes.

**Tech Stack:** Rust 2021, clap 4.5 (builder API, `env` feature), `serial_test` for env-mutating
tests.

**Spec:** `docs/superpowers/specs/2026-07-25-cli-env-vars-design.md`

---

## Background for the implementer

`ubi` is a workspace with two crates: `ubi/` (the library) and `ubi-cli/` (the `ubi` binary). All
work here is in `ubi-cli/src/main.rs`, `ubi-cli/Cargo.toml`, `README.md`, and `Changes.md`.

`ubi-cli/src/main.rs` has one function, `cmd()`, that builds the whole clap `Command` as a single
chain of `.arg(...)` calls. A separate function, `make_ubi`, reads the resulting `ArgMatches` and
feeds a `UbiBuilder`. **You will not need to touch `make_ubi` at all** — env vars are resolved by
clap during parsing, so by the time `make_ubi` runs, an env-provided value is indistinguishable from
a flag-provided one.

### Why the `env_arg` helper exists

clap's `Arg::env(name)` reads the variable _at the time you call it_ and stores `Option<OsString>`.
A variable set to `""` yields `Some("")`, and the parser then supplies `""` as the argument's value.
That is wrong for us: `UBI_TAG=""` must mean "no `--tag`", not "the release tagged empty string".
clap offers no option for this, and a `value_parser` can't help because it can reject an empty value
but cannot turn it back into "argument absent".

Since `.env()` snapshots at call time, the fix is to just not call it when the variable is empty.

### Running things

Tests and lint run inside a devcontainer via `just`:

```bash
just test "" -p ubi-cli --bin ubi
```

The first argument to `just test` is a `RUST_LOG` value; pass `""` for none. Everything after it is
forwarded to `cargo test`.

If the devcontainer is unavailable, `cargo test -p ubi-cli --bin ubi` works directly — these are
pure unit tests with no network access.

Lint with `just lint --all` and format with `just tidy --all`. The git pre-commit hook runs
`precious lint` automatically, so commit normally — do not pass `--no-verify`.

---

## File Structure

- **Modify `ubi-cli/Cargo.toml`** — add clap's `env` feature.
- **Modify `ubi-cli/src/main.rs`** — add the `env_arg` helper, wrap twelve args in `cmd()`, add a
  `value_parser` to `--extract-all`, and add a `#[cfg(test)] mod tests` at the end of the file.
- **Modify `README.md`** — add an `Env Var` column to the CLI flags table plus explanatory prose.
- **Modify `Changes.md`** — add an entry.

The tests live in `main.rs` rather than `ubi-cli/tests/` because they exercise `cmd()`, a private
function, and because the existing integration tests in `ubi-cli/tests/ubi.rs` are slow,
network-dependent end-to-end tests — a bad fit for parser-level assertions.

---

## Task 1: Enable the clap `env` feature and add the `env_arg` helper, wired to `--tag`

This task builds the whole mechanism and proves it on one flag. Tasks 2 and 3 then apply it to the
rest.

**Files:**

- Modify: `ubi-cli/Cargo.toml:15`
- Modify: `ubi-cli/src/main.rs` (imports, `cmd()`, new helper, new test module)

- [ ] **Step 1: Enable the clap `env` feature**

`ubi-cli/Cargo.toml` line 15 currently reads:

```toml
clap = { version = "4.5.54", features = ["default", "wrap_help"] }
```

Change it to:

```toml
clap = { version = "4.5.54", features = ["default", "env", "wrap_help"] }
```

The `env` feature is not part of `default`. Without it, `Arg::env` does not exist and the code will
not compile.

- [ ] **Step 2: Write the failing tests**

Add this at the very end of `ubi-cli/src/main.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use std::ffi::OsString;

    /// Every env var the CLI knows about. The test guard clears all of these before each test so
    /// that a variable set in the developer's own shell cannot influence the results.
    const ALL_ENV_VARS: &[&str] = &[
        "UBI_API_BASE_URL",
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
}
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `just test "" -p ubi-cli --bin ubi`

Expected: `tag_from_env` and `tag_flag_overrides_env` FAIL. `tag_from_env` fails with
`assertion \`left == right\` failed: left: None, right:
Some("v1.2.3")` because nothing reads the env var yet. (`empty_tag_env_is_unset` passes trivially at
this point — it will keep passing, and it is what guards the behavior added in step 4.)

- [ ] **Step 4: Add the `env_arg` helper**

Add this function to `ubi-cli/src/main.rs`, immediately after `cmd()` ends (after the closing brace
on the line following `.max_term_width(MAX_TERM_WIDTH)`):

```rust
// `Arg::env` reads the env var when it's called and treats a var set to the empty string as a
// present-but-empty value. We want an empty var to mean "not set at all", so that a script can do
// `UBI_TAG=$SOME_VAR ubi ...` without caring whether `$SOME_VAR` is empty. Since `Arg::env` reads
// the var here and now, we get that behavior by simply not attaching the env var when it's empty.
fn env_arg(arg: Arg, name: &'static str) -> Arg {
    if env::var_os(name).is_some_and(|v| v.is_empty()) {
        arg
    } else {
        arg.env(name)
    }
}
```

`env` is already imported at the top of the file via `use std::{env, path::Path, str::FromStr};`, so
no import change is needed.

- [ ] **Step 5: Wire `--tag` through the helper**

In `cmd()`, replace this block (currently at `ubi-cli/src/main.rs:68-78`):

```rust
        .arg(
            Arg::new("tag")
                .long("tag")
                .short('t')
                .requires("project")
                .help(concat!(
                    "The tag to download. Defaults to the latest release.",
                    " This can only be passed with `--project`. You cannot pass this when",
                    " `--url` or `--min-age-days` are passed.",
                )),
        )
```

with:

```rust
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
```

- [ ] **Step 6: Run the tests to verify they pass**

Run: `just test "" -p ubi-cli --bin ubi`

Expected: PASS — `tag_from_env`, `tag_flag_overrides_env`, and `empty_tag_env_is_unset` all green.

- [ ] **Step 7: Commit**

```bash
git add ubi-cli/Cargo.toml ubi-cli/src/main.rs
git commit -m "Add env var support for the --tag flag"
```

---

## Task 2: Wire the remaining ten value-taking flags

**Files:**

- Modify: `ubi-cli/src/main.rs` (`cmd()` and the test module)

- [ ] **Step 1: Write the failing tests**

Add these to the `tests` module in `ubi-cli/src/main.rs`, after `empty_tag_env_is_unset`:

```rust
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
```

Note that the clap arg ID for `--rename-exe` is `rename-exe-to`, not `rename-exe` — the flag's long
name and its ID differ. The env var name follows the long flag name, so it is `UBI_RENAME_EXE`.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `just test "" -p ubi-cli --bin ubi`

Expected: `string_values_from_env`, `url_from_env`, `min_age_days_from_env`, and
`invalid_min_age_days_from_env_is_an_error` FAIL. The first three fail on `assert_eq!` comparisons
against `None`; the fourth fails because with no `--project`, `--url`, or `--self-upgrade` the error
kind is `MissingRequiredArgument` rather than `ValueValidation`. (`empty_env_vars_are_all_unset`
passes already and stays passing.)

- [ ] **Step 3: Wrap the remaining args**

In `cmd()`, wrap each of the following args in `env_arg(...)` exactly as `--tag` was wrapped in Task
1: change `.arg(` to `.arg(env_arg(`, and change the arg's trailing `,\n        )` to
`,\n            "UBI_NAME",\n        ))`, indenting the arg's body one level deeper.

| Arg ID           | Env var name         |
| ---------------- | -------------------- |
| `project`        | `UBI_PROJECT`        |
| `url`            | `UBI_URL`            |
| `in`             | `UBI_IN`             |
| `exe`            | `UBI_EXE`            |
| `rename-exe-to`  | `UBI_RENAME_EXE`     |
| `min-age-days`   | `UBI_MIN_AGE_DAYS`   |
| `matching`       | `UBI_MATCHING`       |
| `matching-regex` | `UBI_MATCHING_REGEX` |
| `forge`          | `UBI_FORGE`          |
| `api-base-url`   | `UBI_API_BASE_URL`   |

Do **not** wrap `extract-all` (that is Task 3), and do **not** wrap `self-upgrade`, `verbose`,
`debug`, or `quiet` at all — those are deliberately excluded.

As a worked example, the `project` arg at `ubi-cli/src/main.rs:59-67` becomes:

```rust
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
```

Note that the original `project` block ends with a stray `)),)` — replace that whole trailing
sequence with the form shown above.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `just test "" -p ubi-cli --bin ubi`

Expected: PASS, all tests green.

- [ ] **Step 5: Commit**

```bash
git add ubi-cli/src/main.rs
git commit -m "Add env var support for the remaining value-taking flags"
```

---

## Task 3: Wire `--extract-all` with a boolish value parser

`ArgAction::SetTrue` uses a strict `bool` value parser that accepts only the exact strings `true`
and `false`. That is fine for a command-line flag, where the value is synthesized by clap, but it
means `UBI_EXTRACT_ALL=1` would fail with "invalid value '1'". `BoolishValueParser` accepts
`true`/`false`, `yes`/`no`, `on`/`off`, and `1`/`0`.

**Files:**

- Modify: `ubi-cli/src/main.rs` (`cmd()`, imports, and the test module)

- [ ] **Step 1: Write the failing tests**

Add these to the `tests` module:

```rust
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
        for value in ["0", "false", "no", "off", ""] {
            let _guard = EnvGuard::new(&[("UBI_EXTRACT_ALL", value)]);
            let matches = cmd()
                .try_get_matches_from(["ubi", "--project", "houseabsolute/precious"])
                .unwrap();
            assert!(!matches.get_flag("extract-all"), "UBI_EXTRACT_ALL={value}");
        }
    }

    #[test]
    #[serial]
    fn extract_all_flag_works_without_env() {
        let _guard = EnvGuard::new(&[]);
        let matches = cmd()
            .try_get_matches_from(["ubi", "--project", "houseabsolute/precious", "--extract-all"])
            .unwrap();
        assert!(matches.get_flag("extract-all"));
    }
```

`extract_all_flag_works_without_env` guards against breaking the plain command-line use of the flag
while changing its value parser.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `just test "" -p ubi-cli --bin ubi`

Expected: `extract_all_from_env` FAILS on the first iteration (the env var is not read yet, so the
flag is false). `falsy_extract_all_from_env` and `extract_all_flag_works_without_env` pass already
and must keep passing.

- [ ] **Step 3: Add the value parser and wrap the arg**

Change the import on `ubi-cli/src/main.rs:2` from:

```rust
use clap::{Arg, ArgAction, ArgGroup, ArgMatches, Command};
```

to:

```rust
use clap::{builder::BoolishValueParser, Arg, ArgAction, ArgGroup, ArgMatches, Command};
```

Then replace the `extract-all` block (currently at `ubi-cli/src/main.rs:121-136`) with:

```rust
        .arg(env_arg(
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
                    "  `--rename-exe-to` are passed.",
                )),
            "UBI_EXTRACT_ALL",
        ))
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `just test "" -p ubi-cli --bin ubi`

Expected: PASS, all tests green.

- [ ] **Step 5: Commit**

```bash
git add ubi-cli/src/main.rs
git commit -m "Add env var support for the --extract-all flag"
```

---

## Task 4: Test the interaction with arg groups, conflicts, and the excluded flags

No production code changes here — this task pins down behavior the previous tasks produced for free
via clap, so that a future refactor cannot silently change it.

**Files:**

- Modify: `ubi-cli/src/main.rs` (test module only)

- [ ] **Step 1: Write the tests**

Add these to the `tests` module:

```rust
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
```

`env_vars_are_attached_to_exactly_the_expected_args` does double duty: it fails if someone adds a
new env-backed flag without adding it to `ALL_ENV_VARS` (which would let the developer's own
environment leak into every other test), and it fails if someone attaches an env var to
`--self-upgrade` or a log-level flag, which the design deliberately excludes. It relies on the guard
having cleared all the vars, so that every env-backed arg actually gets its env var attached.

- [ ] **Step 2: Run the tests to verify they pass**

Run: `just test "" -p ubi-cli --bin ubi`

Expected: PASS. These describe behavior that already works — if any of them fail, one of the
previous tasks was implemented incorrectly. In particular, if
`env_vars_are_attached_to_exactly_the_expected_args` fails, compare its output against the table in
Task 2 to find the arg that was missed or misnamed.

- [ ] **Step 3: Commit**

```bash
git add ubi-cli/src/main.rs
git commit -m "Add tests for env var interaction with arg groups and conflicts"
```

---

## Task 5: Check the rendered help output

**Files:**

- None expected. This is a verification step.

- [ ] **Step 1: Render the help**

```bash
cargo run -p ubi-cli --bin ubi -- --help
```

clap appends `[env: UBI_TAG=]` to the help text of each env-backed flag automatically.

- [ ] **Step 2: Check the wrapping**

`cmd()` sets `max_term_width(MAX_TERM_WIDTH)` where `MAX_TERM_WIDTH` is 100, and the help strings
are long. Confirm that the `[env: ...]` suffixes appear and that no line runs past 100 columns or
wraps in a way that mangles the layout. Pipe through `awk '{ if (length($0) > 100) print }'` to
check.

If the output looks fine, there is nothing to commit and you move on to Task 6. If it does not, stop
and report what you see rather than guessing at a fix — the design explicitly expects no change
here.

---

## Task 6: Documentation

**Files:**

- Modify: `README.md` (the CLI flags table under "How to Use It", around lines 76-95)
- Modify: `Changes.md` (top of file)

- [ ] **Step 1: Add the `Env Var` column to the CLI flags table**

The table under `## How to Use It` currently has the columns `Key | Type | Required? | Description`.
Insert a new `Env Var` column between `Key` and `Type`, so the header becomes:

```
| Key | Env Var | Type | Required? | Description |
```

Fill it in per this mapping, using backticks around each name:

| Key                                       | Env Var              |
| ----------------------------------------- | -------------------- |
| `-p`, `--project <project>`               | `UBI_PROJECT`        |
| `-t`, `--tag <tag>`                       | `UBI_TAG`            |
| `-u`, `--url <url>`                       | `UBI_URL`            |
| `-i`, `--in <in>`                         | `UBI_IN`             |
| `-e`, `--exe <exe>`                       | `UBI_EXE`            |
| `-m`, `--matching <matching>`             | `UBI_MATCHING`       |
| `-r`, `--matching-regex <matching-regex>` | `UBI_MATCHING_REGEX` |
| `--min-age-days`                          | `UBI_MIN_AGE_DAYS`   |
| `--rename-exe <rename-exe-to>`            | `UBI_RENAME_EXE`     |
| `--extract-all`                           | `UBI_EXTRACT_ALL`    |
| `--forge <forge>`                         | `UBI_FORGE`          |
| `--api-base-url <api-base-url>`           | `UBI_API_BASE_URL`   |

Leave the cell empty for `--self-upgrade`, `-v`/`--verbose`, `-d`/`--debug`, `-q`/`--quiet`,
`-h`/`--help`, and `-V`/`--version`.

Do **not** touch the environment variable table further up under "Installing the CLI Tool" — that
one documents the bootstrap shell script's parameters, which are unrelated.

- [ ] **Step 2: Add the explanatory prose**

Immediately after that table, before the `## Using a Forge Token` heading, add:

````markdown
### Setting Flags with Environment Variables

Every flag with an entry in the `Env Var` column above can be set through that environment variable
instead of on the command line. There are three rules worth knowing:

- A flag passed on the command line always wins over the corresponding environment variable.
- An environment variable set to the empty string is treated as if it were not set at all. This
  makes it easy to conditionally override a default in a script:

  ```sh
  UBI_TAG=$MY_TAG ubi --project houseabsolute/precious
  ```

  If `$MY_TAG` is empty, this installs the latest release, exactly as if `--tag` had not been
  passed.

- Values from environment variables are subject to the same conflict rules as the flags themselves.
  If you export `UBI_URL` in your shell profile, then running `ubi --project houseabsolute/precious`
  is an error, because `--url` and `--project` cannot be combined.

Note that `--self-upgrade` and the `--verbose`, `--debug`, and `--quiet` flags cannot be set through
environment variables.
````

- [ ] **Step 3: Add a `Changes.md` entry**

`Changes.md` starts with `## 0.9.0 2026-01-11`, and the workspace version is currently `0.9.0`. This
is a new feature, and the previous feature addition (`--min-age-days`) got a minor bump, so add a
new unreleased section above it:

```markdown
## 0.10.0

- Most CLI flags can now be set with a corresponding `UBI_*` environment variable, for example
  `UBI_TAG` for `--tag` or `UBI_PROJECT` for `--project`. A variable set to the empty string is
  treated as unset, so scripts can pass a possibly-empty value without building up a command line by
  hand. Flags passed on the command line take precedence over environment variables. Requested by
  @BatmanAoD (Kyle J Strand). GH #153.
```

If an unreleased section already exists at the top of `Changes.md`, add the bullet to it instead of
creating a new section.

- [ ] **Step 4: Verify the Markdown formatting**

Run: `just tidy --all`

Expected: `README.md` and `Changes.md` reformatted (prettier reflows the tables) with no errors.

- [ ] **Step 5: Commit**

```bash
git add README.md Changes.md
git commit -m "Document env var support for CLI flags"
```

---

## Task 7: Final verification

**Files:**

- None. Verification only.

- [ ] **Step 1: Run the full CLI crate unit test suite**

Run: `just test "" -p ubi-cli --bin ubi`

Expected: all tests pass, zero failures.

- [ ] **Step 2: Run clippy**

Run: `cargo clippy -p ubi-cli --all-targets`

Expected: no warnings. The workspace denies `fallible_impl_from`, `wildcard_enum_match_arm`,
`unneeded_field_pattern`, and `fn_params_excessive_bools`; none of this work should trip them.

- [ ] **Step 3: Confirm the whole workspace still builds and tests**

Run: `cargo build --workspace`

Expected: success. The library crate is untouched, so nothing there should change.

- [ ] **Step 4: Smoke test the actual behavior end to end**

```bash
UBI_PROJECT=houseabsolute/precious UBI_IN=/tmp/ubi-smoke cargo run -p ubi-cli --bin ubi -- --debug
```

Expected: `precious` is downloaded into `/tmp/ubi-smoke` without any flags being passed for the
project or install directory. This requires network access; skip it if the environment has none and
say so in your report.
