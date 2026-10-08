# Set by devcontainer.json, so recipes can tell whether they are already running inside the dev
# container. Outside it, everything is wrapped in `devcontainer exec`. Inside, there is nothing to
# wrap - the tools are right there, and wrapping would try to start a nested container. Unlike the
# git mount below, this lives in devcontainer.json, so `devcontainer up` recreates a stale
# container on its own and no rebuild is needed.
_in_container := env("UBI_DEVCONTAINER", "")
# The devcontainer CLI is pinned in mise.toml, so go through mise rather than assuming the caller
# has mise activated in their shell. Git hooks in particular run with a bare PATH.
_dce := if _in_container != "" { "" } else { "mise exec -- devcontainer exec --workspace-folder ." }
# `devcontainer exec` takes env vars as flags. A plain shell needs `env` instead.
_env := if _in_container != "" { "env" } else { "--remote-env" }
# When we're in a git worktree, the workspace's .git is a file pointing at a
# gitdir outside the workspace, so git doesn't work in the container unless we
# also mount the main repo's git dir at the same path. Note that `devcontainer
# up` reuses an existing container without comparing mounts, so a container
# created before this mount existed needs a `just rebuild` once.
_git_common_dir := `test -f .git && realpath "$(git rev-parse --git-common-dir)" || true`
_git_mount := if _git_common_dir != "" { "--mount 'type=bind,source=" + _git_common_dir + ",target=" + _git_common_dir + "'" } else { "" }
# Puts the integration test tokens in the environment of the command it runs. Only the recipes
# that need them use it, and it is what lets the devcontainer CLI resolve the ${localEnv:...}
# entries in devcontainer.json's remoteEnv.
_with_tokens := ".devcontainer/with-tokens.sh"

_host_only recipe:
    #!/usr/bin/env bash
    set -euo pipefail
    if [ -n "{{ _in_container }}" ]; then
        echo "just {{ recipe }} has to run on the host, not inside the dev container" >&2
        exit 1
    fi

_up:
    #!/usr/bin/env bash
    set -euo pipefail
    if [ -n "{{ _in_container }}" ]; then
        exit 0
    fi
    .devcontainer/up.sh --workspace-folder . {{ _git_mount }}

rebuild: (_host_only "rebuild")
    .devcontainer/up.sh --workspace-folder . {{ _git_mount }} --remove-existing-container

shell: _up
    {{ _with_tokens }} {{ _dce }} bash -i

test rust-log="" *args: _up
    {{ _with_tokens }} {{ _dce }} \
      {{ if rust-log != "" { _env + " RUST_LOG=" + rust-log } else { "" } }} \
      cargo test {{ args }}

lint *args: _up
    {{ _dce }} mise exec -- precious lint {{ args }}

tidy *args: _up
    {{ _dce }} mise exec -- precious tidy {{ args }}

# Cut a release. The level is anything cargo-release accepts, so "patch", "minor", "major", or an
# explicit version like "0.11.0". This bumps the version everywhere, stamps the "NEXT" section in
# Changes.md with the version and date, commits, tags, and pushes. Pushing the tag is what makes
# CI build the binaries, publish the crates to crates.io, and draft the GitHub release.
#
# Unlike the other recipes, cargo-release runs on the host rather than in the dev container,
# because the commit and tag are signed and the signing key lives outside the container.
release level: (_host_only "release") (test "" "--workspace --locked") (lint "-a")
    mise exec -- cargo-release release {{ level }} --workspace --execute

# Upgrade the Rust dependencies and the tools in mise.toml, skipping any release that is less than
# three days old. Brand new releases are where a compromised package is most likely to show up, and
# a short wait gives the ecosystem time to notice and yank it.
#
# For mise, the cutoff comes from `minimum_release_age` in mise.toml. Keep the two ages in sync.
#
# Cargo's version of this is still unstable. RUSTC_BOOTSTRAP is what lets a stable cargo accept
# the -Z flag, so this does not need a nightly toolchain, but it does need cargo 1.99 or newer.
#
# `cargo upgrade` knows nothing about release ages and always picks the newest version. So rather
# than letting it choose, we update Cargo.lock with the age limit first, and then tell `cargo
# upgrade` to set each requirement in Cargo.toml to the version that ended up in the lockfile.
# Requirements that are deliberately loose, like "0.4", are left alone.
#
# Like `release`, this runs on the host, because it needs cargo-edit and jq, which are not in the
# dev container.
upgrade-deps: (_host_only "upgrade-deps")
    #!/usr/bin/env bash
    set -euo pipefail
    export RUSTC_BOOTSTRAP=1
    mise exec -- cargo update -Z min-publish-age \
        --config 'registry.global-min-publish-age="3 days"'
    packages=$(mise exec -- cargo metadata --format-version 1 | mise exec -- jq -r '
        . as $md
        | ([$md.resolve.nodes[] | select(.id | IN($md.workspace_members[])) | .deps[].pkg]
            | unique) as $ids
        | ([$md.packages[] | select(.id | IN($ids[])) | {key: .name, value: .version}]
            | from_entries) as $locked
        | [$md.packages[]
            | select(.id | IN($md.workspace_members[]))
            | .dependencies[]
            | select(.source != null and (.req | test("^\\^?[0-9]+\\.[0-9]+\\.[0-9]+$")))
            | "--package=\(.name)@\($locked[.name])"]
        | unique
        | .[]
    ')
    # `cargo upgrade` re-resolves the lockfile without the age limit, even with `--recursive
    # false`. The lockfile we already have still satisfies the new requirements, so put it back.
    lock=$(mktemp)
    trap 'rm -f "$lock"' EXIT
    cp Cargo.lock "$lock"
    # shellcheck disable=SC2086 # one argument per line of $packages
    mise exec -- cargo upgrade --incompatible --recursive false $packages
    cp "$lock" Cargo.lock
    mise exec -- cargo metadata --locked --format-version 1 >/dev/null
    mise upgrade --bump
