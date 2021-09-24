#!/bin/bash

set -eo pipefail

function run () {
    echo $1
    eval $1
}

function install_tools () {
    curl --silent --location \
         https://raw.githubusercontent.com/houseabsolute/ubi/master/bootstrap/bootstrap-ubi.sh |
        sh
    run "rustup component add clippy"
    run "ubi --project houseabsolute/precious --in ~/bin"
    run "ubi --project houseabsolute/omegasort --in ~/bin"
}

if [ "$1" == "-v" ]; then
    set -x
fi

mkdir -p $HOME/bin

set +e
echo ":$PATH:" | grep --extended-regexp ":$HOME/bin:" >& /dev/null
if [ "$?" -eq "0" ]; then
    path_has_home_bin=1
fi
set -e

if [ -z "$path_has_home_bin" ]; then
    PATH=$HOME/bin:$PATH
fi

install_tools

echo "Tools were installed into $HOME/bin."
if [ -z "$path_has_home_bin" ]; then
     echo "You should add $HOME/bin to your PATH."
fi

exit 0
