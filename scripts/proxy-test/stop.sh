#!/bin/sh
set -eu

STATE_DIR="${PROXY_TEST_DIR:-/tmp/tokio-tungstenite-proxy-tests}"
SQUID_BIN="${SQUID_BIN:-/opt/homebrew/opt/squid/sbin/squid}"
MICROSOCKS_BIN="${MICROSOCKS_BIN:-/opt/homebrew/opt/microsocks/bin/microsocks}"
SQUID_CONF="$STATE_DIR/squid.conf"
SQUID_PID="$STATE_DIR/squid.pid"
MICROSOCKS_PID="$STATE_DIR/microsocks.pid"

if [ -x "$SQUID_BIN" ] && [ -f "$SQUID_CONF" ]; then
  "$SQUID_BIN" -k shutdown -f "$SQUID_CONF" >/dev/null 2>&1 || true
fi

if [ -f "$SQUID_PID" ]; then
  kill "$(cat "$SQUID_PID")" 2>/dev/null || true
  rm -f "$SQUID_PID"
fi

SQUID_PIDS="$(pgrep -f "$SQUID_BIN" || true)"
if [ -n "$SQUID_PIDS" ]; then
  kill $SQUID_PIDS 2>/dev/null || true
fi

if [ -f "$MICROSOCKS_PID" ]; then
  kill "$(cat "$MICROSOCKS_PID")" 2>/dev/null || true
  rm -f "$MICROSOCKS_PID"
fi

MICROSOCKS_PIDS="$(pgrep -f "$MICROSOCKS_BIN" || true)"
if [ -n "$MICROSOCKS_PIDS" ]; then
  kill $MICROSOCKS_PIDS 2>/dev/null || true
fi
