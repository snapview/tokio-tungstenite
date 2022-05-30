#!/usr/bin/env bash
# Author michael <themichaeleden@gmail.com>
set -euo pipefail
set -x
SOURCE_DIR=$(readlink -f "${BASH_SOURCE[0]}")
SOURCE_DIR=$(dirname "$SOURCE_DIR")
cd "${SOURCE_DIR}/.."

CONTAINER_NAME=fuzzingserver
function cleanup() {
    docker container stop "${CONTAINER_NAME}"
}
trap cleanup TERM EXIT

function test_diff() {
    if ! diff -q \
        <(jq -S 'del(."Tungstenite" | .. | .duration?)' 'autobahn/expected-results.json') \
        <(jq -S 'del(."Tungstenite" | .. | .duration?)' 'autobahn/client/index.json')
    then
        echo 'Difference in results, either this is a regression or' \
             'one should update autobahn/expected-results.json with the new results.'
        exit 64
    fi
}

docker run -d --rm \
    -v "${PWD}/autobahn:/autobahn" \
    -p 9001:9001 \
    --init \
    --name "${CONTAINER_NAME}" \
    crossbario/autobahn-testsuite \
    wstest -m fuzzingserver -s 'autobahn/fuzzingserver.json'

sleep 5
cargo run --release --example autobahn-client
test_diff
