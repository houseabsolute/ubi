# Env Var Support for CLI Flags

Design for [issue #153](https://github.com/houseabsolute/ubi/issues/153).

## Problem

Scripts that conditionally pass a flag to `ubi` must build up a command line by hand:

```sh
if [[ -n "${MY_TAG}" ]]; then
    UBI_TAG="--tag ${MY_TAG}"
fi
ubi <...> $UBI_TAG
```

If each flag had a corresponding environment variable, this would be:

```sh
UBI_TAG=$MY_TAG ubi ...
```

## Scope

Twelve args in `ubi-cli/src/main.rs` gain an env var fallback. Names are `UBI_` plus the long flag
uppercased with `-` replaced by `_`, so every name is derivable from `--help` without memorization.

| Flag               | Env var              |
| ------------------ | -------------------- |
| `--project`        | `UBI_PROJECT`        |
| `--tag`            | `UBI_TAG`            |
| `--url`            | `UBI_URL`            |
| `--in`             | `UBI_IN`             |
| `--exe`            | `UBI_EXE`            |
| `--rename-exe`     | `UBI_RENAME_EXE`     |
| `--extract-all`    | `UBI_EXTRACT_ALL`    |
| `--min-age-days`   | `UBI_MIN_AGE_DAYS`   |
| `--matching`       | `UBI_MATCHING`       |
| `--matching-regex` | `UBI_MATCHING_REGEX` |
| `--forge`          | `UBI_FORGE`          |
| `--api-base-url`   | `UBI_API_BASE_URL`   |

Two flags are deliberately excluded:

- `--self-upgrade`. An env var that silently turns any invocation into a self-upgrade is a footgun.
- `-v` / `-d` / `-q`. Log level describes the current terminal session, not the install config, and
  inheriting a stray `UBI_DEBUG` from a parent shell would be surprising.

Excluding `--self-upgrade` does not exempt it from the conflict rule below. It conflicts with
`--exe`, `--extract-all`, `--forge`, `--in`, `--project`, `--tag`, and `--url`, so exporting any of
those seven variables makes `ubi --self-upgrade` fail with a conflict error. We accept that and
document the `env -u` workaround in the README rather than special-casing it, since suppressing env
vars only for this flag would require pre-scanning `argv`, which is the approach rejected above.

## Behavior

**Precedence.** An explicit CLI flag wins over its env var. This is clap's native behavior: the
parser skips any arg already present in the matcher before applying env values.

**Empty means unset.** An env var set to the empty string is treated as if it were not set at all.
This is what makes the issue's use case work — `UBI_TAG=$MY_TAG ubi ...` installs the latest release
when `MY_TAG` is empty, rather than requesting a release literally tagged `""`. The consequence is
that there is no way to pass a genuinely empty value via env, which is fine because no flag has a
meaningful empty value.

**Conflicts apply.** Env-provided values participate in the existing `conflicts_with_all` and
`requires` rules exactly as CLI flags do. So with `UBI_URL` exported in a shell profile,
`ubi --project foo/bar` is a hard error rather than silently preferring one or the other. The
conflicting sets here — `--url` vs. `--project`/`--tag`, `--min-age-days` vs. `--tag`/`--url`,
`--extract-all` vs. `--exe`/`--rename-exe` — are ones where guessing wrong means installing
something the user did not ask for, so a loud error is the right outcome. The alternative, ignoring
env vars for flags that conflict with something on the command line, would require pre-scanning
`argv` before building the `Command`, which is fragile in the presence of short flags, `=`-joined
values, and `--`.

**Required group.** An env-set `UBI_PROJECT` or `UBI_URL` satisfies the required `ArgGroup`, so a
bare `ubi` with one of those exported works.

## Implementation

### clap configuration

`ubi-cli/Cargo.toml` currently enables clap features `["default", "wrap_help"]`. The `env` feature
is not part of `default` and must be added.

### Empty-means-unset

clap has no built-in support for this. `Arg::env` snapshots the value with `env::var_os` at the time
it is called, yielding `Some("")` for an empty var, and the parser feeds any `Some(val)` in as a
value. There is no `env_if_non_empty` and no hook; `value_parser` cannot help either, since it can
reject an empty value but not turn it back into "absent".

Because the snapshot happens when we call `.env()`, we do not need to read env vars by hand or
mutate the process environment. We simply do not attach the env var when it is empty:

```rust
fn env_arg(arg: Arg, name: &'static str) -> Arg {
    // clap's `Arg::env` snapshots the value here and treats an empty string as a
    // present-but-empty value. We want an empty var to mean "not set", so we
    // simply don't attach the env var in that case.
    if env::var_os(name).is_some_and(|v| v.is_empty()) {
        arg
    } else {
        arg.env(name)
    }
}
```

Each supported arg is wrapped at construction in `cmd()`:

```rust
.arg(env_arg(
    Arg::new("tag").long("tag").short('t')./* ... */,
    "UBI_TAG",
))
```

Nothing downstream changes. `make_ubi` still reads from `matches`, so the builder wiring, the
conflict and requirement rules, and the required `ArgGroup` all keep working as-is.

### Boolean parsing

`ArgAction::SetTrue` defaults to a strict `bool` value parser, which accepts only `true` and
`false`. `--extract-all` gets `.value_parser(BoolishValueParser::new())` so that `UBI_EXTRACT_ALL=1`
does not fail with "invalid value '1'". That parser accepts `y`/`n`, `yes`/`no`, `t`/`f`,
`true`/`false`, `on`/`off`, and `1`/`0`, case-insensitively.

Empty-means-unset is not enough for a boolean arg. clap records an arg as _present_ whenever its env
var is attached and set, and a present arg participates in `conflicts_with_all` regardless of the
value it parsed to. So `UBI_EXTRACT_ALL=0` would make `ubi --project foo/bar --exe x` fail with a
conflict error, even though the flag is off. `--extract-all` therefore uses a separate
`bool_env_arg` helper that skips attaching the env var when the value is empty **or falsy**, so that
every way of saying "off" is genuinely indistinguishable from not setting the variable at all. The
falsy list mirrors clap's private `FALSE_LITERALS` and must be kept in sync with it.

`--min-age-days` and `--forge` keep their existing parsers, which now also validate env input.

### Help output

clap automatically appends `[env: UBI_TAG=]` to the help for each affected flag. The rendered
`--help` should be checked against the existing `MAX_TERM_WIDTH` of 100 for bad wrapping, but no
change is expected.

## Testing

Unit tests in `ubi-cli/src/main.rs` exercising `cmd()` via `try_get_matches_from`. Env is
process-global, so each test is marked `#[serial]` (`serial_test` is already a dev-dependency) and
saves and restores the vars it touches, following the pattern in `ubi/src/forge.rs`.

Cases:

- An env var supplies the value when the flag is absent.
- An explicit CLI flag overrides the env var.
- An empty env var is treated as absent.
- `UBI_EXTRACT_ALL` accepts `1`, `0`, `true`, `false`, and empty, and a falsy value neither sets the
  flag nor triggers the `--exe` conflict.
- `UBI_PROJECT` alone satisfies the required arg group.
- `UBI_URL` combined with `--project` on the command line is a conflict error.
- An invalid `UBI_MIN_AGE_DAYS` is a parse error.

## Documentation

The CLI flags table under "How to Use It" in `README.md` gains an **Env Var** column
(`Key | Env Var | Type | Required? | Description`), filled in for the twelve supported flags and
left empty for `--self-upgrade`, `-v`/`-d`/`-q`, `-h`, and `-V`.

A short paragraph below that table covers the three rules that a column of names does not convey:
CLI flags win over env vars; an empty env var is unset, with the `UBI_TAG="$MY_TAG" ubi ...` example
from the issue; and env values are subject to the same conflict rules as flags.

The environment variable table in the "Installing the CLI Tool" section documents the bootstrap
script's parameters, not `ubi`'s, and is left alone.

A `Changes.md` entry is added.
