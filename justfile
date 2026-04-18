_dce := "devcontainer exec --workspace-folder ."

shell:
    {{ _dce }} bash

test *args:
    {{ _dce }} cargo test {{ args }}

lint *args:
    {{ _dce }} mise exec -- precious lint {{ args }}

tidy *args:
    {{ _dce }} mise exec -- precious tidy {{ args }}
