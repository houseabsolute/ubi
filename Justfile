# Set by devcontainer.json, so recipes can tell whether they are already running inside the dev
# container. Outside it, everything is wrapped in `devcontainer exec`. Inside, there is nothing to
# wrap - the tools are right there, and wrapping would try to start a nested container. Like the
# git mount below, a container created before this variable existed needs a `just rebuild` once.
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
_github_token := `git config github.ubiTestingToken 2>/dev/null || true`
_gitlab_token := `git config gitlab.ubiTestingToken 2>/dev/null || true`
_codeberg_token := `git config codeberg.ubiTestingToken 2>/dev/null || true`

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
    {{ _dce }} bash -i

test rust-log="" *args: _up
    {{ _dce }} \
      {{ if _github_token != "" { _env + " GITHUB_TOKEN=" + _github_token } else { "" } }} \
      {{ if _gitlab_token != "" { _env + " GITLAB_TOKEN=" + _gitlab_token } else { "" } }} \
      {{ if _codeberg_token != "" { _env + " CODEBERG_TOKEN=" + _codeberg_token } else { "" } }} \
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
