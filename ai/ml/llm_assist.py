#!/usr/bin/env python3
"""Use Multi-Model AI Gateway's verified-free NVIDIA route to bootstrap ML datasets.

The gateway, not this script, owns credentials and the NVIDIA free-model
allowlist. Generated labels are marked for human review and are never deployed
automatically as routing truth.
"""
from __future__ import annotations

import argparse
import json
import time
from pathlib import Path
from urllib.request import Request, urlopen

ALLOWED = {"local", "nvidia", "apple"}


def call(gateway: str, prompt: str) -> dict:
    instruction = (
        "Classify the request for an AI gateway. Return JSON only with exactly "
        "provider (one of local, nvidia, apple) and rationale. Use local for "
        "private/simple/local-tool work, apple for Apple-native writing or "
        "Foundation Models tasks, and nvidia for difficult reasoning. Request: " + prompt
    )
    body = {"model": "auto", "messages": [{"role": "user", "content": instruction}], "max_tokens": 160, "extra_body": {"provider": "nvidia", "agent": False}}
    request = Request(gateway.rstrip("/") + "/v1/chat/completions", data=json.dumps(body).encode(), headers={"content-type": "application/json"})
    with urlopen(request, timeout=60) as response:
        payload = json.loads(response.read())
    content = payload["choices"][0]["message"]["content"]
    content = content[content.find("{") : content.rfind("}") + 1]
    label = json.loads(content)
    if label.get("provider") not in ALLOWED:
        raise ValueError(f"model returned invalid provider: {label}")
    return {"prompt": prompt, "provider": label["provider"], "rationale": label.get("rationale", ""), "source": "free-nvidia-bootstrap", "review_required": True}


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("input", type=Path, help="JSONL with a prompt field")
    parser.add_argument("output", type=Path)
    parser.add_argument("--gateway", default="http://127.0.0.1:8788")
    parser.add_argument("--delay", type=float, default=0.2)
    parser.add_argument("--limit", type=int, default=0)
    args = parser.parse_args()
    results = []
    lines = [line for line in args.input.read_text().splitlines() if line.strip()]
    if args.limit > 0:
        lines = lines[: args.limit]
    for line in lines:
        results.append(call(args.gateway, json.loads(line)["prompt"]))
        time.sleep(args.delay)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text("\n".join(json.dumps(row) for row in results) + "\n")
    print(json.dumps({"rows": len(results), "output": str(args.output), "review_required": True}))


if __name__ == "__main__":
    main()
