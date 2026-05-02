_dce := "devcontainer exec --workspace-folder ."
_github_token := `git config github.token 2>/dev/null || true`
_gitlab_token := `git config gitlab.token 2>/dev/null || true`
_codeberg_token := `git config codeberg.token 2>/dev/null || true`

_up:
    devcontainer up --workspace-folder .

rebuild:
    devcontainer up --workspace-folder . --remove-existing-container

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
