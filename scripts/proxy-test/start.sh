#!/bin/sh
set -eu

STATE_DIR="${PROXY_TEST_DIR:-/tmp/tokio-tungstenite-proxy-tests}"
SQUID_BIN="${SQUID_BIN:-/opt/homebrew/opt/squid/sbin/squid}"
MICROSOCKS_BIN="${MICROSOCKS_BIN:-/opt/homebrew/opt/microsocks/bin/microsocks}"

SQUID_CONF="$STATE_DIR/squid.conf"
SQUID_PID="$STATE_DIR/squid.pid"
MICROSOCKS_PID="$STATE_DIR/microsocks.pid"

mkdir -p "$STATE_DIR/squid-cache" "$STATE_DIR/squid-log"

cat > "$SQUID_CONF" <<CFG
http_port 3128
pid_filename $STATE_DIR/squid.pid
cache_log stdio:$STATE_DIR/squid-log/cache.log
access_log stdio:$STATE_DIR/squid-log/access.log
cache_dir ufs $STATE_DIR/squid-cache 50 16 256
acl localnet src 127.0.0.1/32
acl Safe_ports port 1-65535
acl SSL_ports port 1-65535
http_access deny !Safe_ports
http_access deny CONNECT !SSL_ports
http_access allow localnet
http_access deny all
CFG

"$SQUID_BIN" -z -f "$SQUID_CONF" >/dev/null 2>&1 || true
if [ -f "$SQUID_PID" ]; then
  kill "$(cat "$SQUID_PID")" 2>/dev/null || true
  rm -f "$SQUID_PID"
fi
"$SQUID_BIN" -f "$SQUID_CONF" >/dev/null 2>&1 || true

if [ -f "$MICROSOCKS_PID" ]; then
  kill "$(cat "$MICROSOCKS_PID")" 2>/dev/null || true
  rm -f "$MICROSOCKS_PID"
fi
"$MICROSOCKS_BIN" -i 127.0.0.1 -p 1080 >"$STATE_DIR/microsocks.log" 2>&1 &
echo $! > "$MICROSOCKS_PID"

echo "REAL_HTTP_PROXY=http://127.0.0.1:3128"
echo "REAL_SOCKS5_PROXY=socks5://127.0.0.1:1080"
echo "Note: set REAL_* vars to opt into real-proxy tests without clobbering existing HTTP_PROXY/ALL_PROXY."
echo "STATE_DIR=$STATE_DIR"
