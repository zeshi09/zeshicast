#!/bin/sh
# Template JSON-RPC extension in POSIX sh + jq. One request per process:
# read one line from stdin, write one line to stdout, exit.
set -eu

request=$(head -n 1)
method=$(printf '%s' "$request" | jq -r '.method')
protocol=$(printf '%s' "$request" | jq -r '.params.protocol')

reply() { jq -cn --argjson result "$1" '{jsonrpc: "2.0", id: 1, result: $result}'; }
fail() { jq -cn --arg message "$1" '{jsonrpc: "2.0", id: 1, error: {code: -32600, message: $message}}'; }

if [ "$protocol" != 1 ]; then
  fail "template speaks protocol 1, host sent $protocol"
  exit 0
fi

case "$method" in
  search)
    query=$(printf '%s' "$request" | jq -r '.params.query')
    reply "$(jq -cn --arg q "$query" '[
      {id: "open-docs", title: "Template: open the docs", subtitle: "open_url",
       open_url: "https://github.com/zeshi09/zeshicast/blob/main/docs/extensions.md"},
      {id: "copy-query", title: ("Template: copy \"" + $q + "\""), copy_text: $q},
      {id: "greet", title: "Template: run an item", subtitle: "asks the extension via execute"}
    ]')"
    ;;
  execute)
    id=$(printf '%s' "$request" | jq -r '.params.id')
    reply "$(jq -cn --arg id "$id" '{success: true, notify: ("Template ran " + $id)}')"
    ;;
  *)
    jq -cn --arg m "$method" '{jsonrpc: "2.0", id: 1, error: {code: -32601, message: ("unknown method " + $m)}}'
    ;;
esac
