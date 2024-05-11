#!/bin/bash

declare -i status
status=0

PRECIOUS=$(which precious)
if [[ -z $PRECIOUS ]]; then
    PRECIOUS=./bin/precious
fi

if ! "$PRECIOUS" lint -s; then
    status+=1
fi

exit $status
