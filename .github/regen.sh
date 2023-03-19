#!/bin/sh

yq --input-format yaml --output-format yaml 'explode(.)' ./.github/ci.yml.src > ./.github/workflows/ci.yml
