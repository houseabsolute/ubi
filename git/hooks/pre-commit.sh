#!/bin/bash

status=0

PRECIOUS=$(which precious)
if [[ -z $PRECIOUS ]]; then
    PRECIOUS=./bin/precious
fi

"$PRECIOUS" lint -s
if (( $? != 0 )); then
    status+=1
fi

exit $status
