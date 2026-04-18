_dce := "devcontainer exec --workspace-folder ."

shell:
    {{ _dce }} bash

test *args:
    {{ _dce }} cargo test {{ args }}

lint:
    {{ _dce }} precious lint --all

tidy:
    {{ _dce }} precious tidy --all
