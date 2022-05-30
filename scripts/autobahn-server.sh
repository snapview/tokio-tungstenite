#!/usr/bin/env bash
# Author michael <themichaeleden@gmail.com>
set -euo pipefail
set -x
SOURCE_DIR=$(readlink -f "${BASH_SOURCE[0]}")
SOURCE_DIR=$(dirname "$SOURCE_DIR")
cd "${SOURCE_DIR}/.."

function cleanup() {
    kill -9 ${WSSERVER_PID}
}
trap cleanup TERM EXIT

function test_diff() {
    if ! diff -q \
        <(jq -S 'del(."Tungstenite" | .. | .duration?)' 'autobahn/expected-results.json') \
        <(jq -S 'del(."Tungstenite" | .. | .duration?)' 'autobahn/server/index.json')
    then
        echo 'Difference in results, either this is a regression or' \
             'one should update autobahn/expected-results.json with the new results.'
        exit 64
    fi
}

cargo run --release --example autobahn-server & WSSERVER_PID=$!
sleep 5

docker run --rm \
    -v "${PWD}/autobahn:/autobahn" \
    --network host \
    crossbario/autobahn-testsuite \
    wstest -m fuzzingclient -s 'autobahn/fuzzingclient.json'

test_diff
