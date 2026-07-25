_dce := "devcontainer exec --workspace-folder ."
# When we're in a git worktree, the workspace's .git is a file pointing at a
# gitdir outside the workspace, so git doesn't work in the container unless we
# also mount the main repo's git dir at the same path. Note that `devcontainer
# up` reuses an existing container without comparing mounts, so a container
# created before this mount existed needs a `just rebuild` once.
_git_common_dir := `test -f .git && realpath "$(git rev-parse --git-common-dir)" || true`
_git_mount := if _git_common_dir != "" { "--mount 'type=bind,source=" + _git_common_dir + ",target=" + _git_common_dir + "'" } else { "" }
_github_token := `git config github.token 2>/dev/null || true`
_gitlab_token := `git config gitlab.token 2>/dev/null || true`
_codeberg_token := `git config codeberg.token 2>/dev/null || true`

_up:
    devcontainer up --workspace-folder . {{ _git_mount }}

rebuild:
    devcontainer up --workspace-folder . {{ _git_mount }} --remove-existing-container

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
