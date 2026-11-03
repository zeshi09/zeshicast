#!/usr/bin/env python3
"""Template JSON-RPC extension in Python (standard library only).

One request per process: read one line from stdin, write one line to stdout,
exit. Point `binaries` in extension.toml at this file to use it.
"""
import json
import sys

PROTOCOL = 1
DOCS = "https://github.com/zeshi09/zeshicast/blob/main/docs/extensions.md"


def search(params):
    query = params.get("query", "")
    return [
        {"id": "open-docs", "title": "Template: open the docs", "open_url": DOCS},
        {"id": "copy-query", "title": f'Template: copy "{query}"', "copy_text": query},
        {"id": "greet", "title": "Template: run an item"},
    ]


def execute(params):
    return {"success": True, "notify": f"Template ran {params.get('id')}"}


def main():
    request = json.loads(sys.stdin.readline())
    params = request.get("params", {})
    reply = {"jsonrpc": "2.0", "id": request.get("id", 1)}
    if params.get("protocol") != PROTOCOL:
        reply["error"] = {"code": -32600, "message": f"template speaks protocol {PROTOCOL}"}
    elif request.get("method") == "search":
        reply["result"] = search(params)
    elif request.get("method") == "execute":
        reply["result"] = execute(params)
    else:
        reply["error"] = {"code": -32601, "message": f"unknown method {request.get('method')}"}
    print(json.dumps(reply), flush=True)


if __name__ == "__main__":
    main()
