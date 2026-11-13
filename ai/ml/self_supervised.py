#!/usr/bin/env python3
"""Prepare local, self-supervised MLX data without sending source off-device.

The corpus mode creates unlabeled text chunks. A causal language model learns
next-token structure from those chunks; there are no human labels. The router
mode turns local outcome telemetry into provisional preferences, which should
be treated as bootstrapping data rather than ground truth.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import random
from pathlib import Path

ALLOWED = {".md", ".txt", ".rs", ".py", ".swift", ".json", ".toml"}
IGNORED = {".git", ".build", "target", ".venv", "node_modules", "__pycache__"}


def chunks(text: str, size: int = 1800) -> list[str]:
    clean = " ".join(text.split())
    return [clean[index : index + size] for index in range(0, len(clean), size) if len(clean[index : index + size]) >= 160]


def corpus(roots: list[Path], output: Path, validation_fraction: float = 0.1) -> dict:
    seen: set[str] = set()
    grouped: list[list[dict[str, str]]] = []
    for root in roots:
        for path in root.rglob("*"):
            if not path.is_file() or path.suffix.lower() not in ALLOWED or any(part in IGNORED for part in path.parts):
                continue
            try:
                text = path.read_text(errors="ignore")
            except OSError:
                continue
            file_rows = []
            for chunk in chunks(text):
                digest = hashlib.sha256(chunk.encode()).hexdigest()
                if digest not in seen:
                    seen.add(digest)
                    file_rows.append({"text": chunk})
            if file_rows:
                grouped.append(file_rows)
    random.Random(42).shuffle(grouped)
    split_group = max(1, int(len(grouped) * (1.0 - validation_fraction)))
    train_rows = [row for group in grouped[:split_group] for row in group]
    valid_rows = [row for group in grouped[split_group:] for row in group]
    output.mkdir(parents=True, exist_ok=True)
    (output / "train.jsonl").write_text("\n".join(json.dumps(row) for row in train_rows) + "\n")
    (output / "valid.jsonl").write_text("\n".join(json.dumps(row) for row in valid_rows) + "\n")
    return {"rows": len(train_rows) + len(valid_rows), "train": len(train_rows), "valid": len(valid_rows), "source_groups": len(grouped), "output": str(output)}


def bootstrap_router(telemetry: Path, output: Path) -> dict:
    """Create provisional labels from successful local outcomes.

    This is deliberately opt-in and does not read gateway logs automatically.
    The input must contain prompt, provider, success, and latency_ms fields.
    """
    rows = []
    for line in telemetry.read_text().splitlines():
        if not line.strip():
            continue
        row = json.loads(line)
        if row.get("prompt") and row.get("provider") in {"local", "nvidia", "apple"} and row.get("success") is not False:
            latency = max(float(row.get("latency_ms", 1_000_000)), 1.0)
            rows.append({"prompt": row["prompt"], "provider": row["provider"], "_utility": 1.0 / latency})
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text("\n".join(json.dumps({"prompt": row["prompt"], "provider": row["provider"]}) for row in rows) + "\n")
    return {"rows": len(rows), "output": str(output), "warning": "outcome-derived labels require evaluation before deployment"}


def main() -> None:
    parser = argparse.ArgumentParser(description="Prepare self-supervised local MLX data")
    sub = parser.add_subparsers(dest="command", required=True)
    corpus_parser = sub.add_parser("corpus")
    corpus_parser.add_argument("output", type=Path)
    corpus_parser.add_argument("roots", nargs="+", type=Path)
    router_parser = sub.add_parser("bootstrap-router")
    router_parser.add_argument("telemetry", type=Path)
    router_parser.add_argument("output", type=Path)
    args = parser.parse_args()
    result = corpus(args.roots, args.output) if args.command == "corpus" else bootstrap_router(args.telemetry, args.output)
    print(json.dumps(result))


if __name__ == "__main__":
    main()
