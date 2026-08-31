_dce := "devcontainer exec --workspace-folder ."
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

_up:
    .devcontainer/up.sh --workspace-folder . {{ _git_mount }}

rebuild:
    .devcontainer/up.sh --workspace-folder . {{ _git_mount }} --remove-existing-container

shell: _up
    {{ _dce }} bash -i

test rust-log="" *args: _up
    {{ _dce }} \
      {{ if _github_token != "" { "--remote-env GITHUB_TOKEN=" + _github_token } else { "" } }} \
      {{ if _gitlab_token != "" { "--remote-env GITLAB_TOKEN=" + _gitlab_token } else { "" } }} \
      {{ if _codeberg_token != "" { "--remote-env CODEBERG_TOKEN=" + _codeberg_token } else { "" } }} \
      {{ if rust-log != "" { "--remote-env RUST_LOG=" + rust-log } else { "" } }} \
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
release level: (test "" "--workspace --locked") (lint "-a")
    mise exec -- cargo-release release {{ level }} --workspace --execute
