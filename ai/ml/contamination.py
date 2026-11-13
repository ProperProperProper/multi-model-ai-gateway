#!/usr/bin/env python3
"""Audit train/validation/test JSONL files for data leakage."""
from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from pathlib import Path


def normalize(text: str) -> str:
    return re.sub(r"\s+", " ", text.lower()).strip()


def load(path: Path) -> set[str]:
    values = set()
    for line in path.read_text().splitlines():
        if line.strip():
            row = json.loads(line)
            text = row.get("text") or row.get("prompt") or ""
            if text:
                values.add(hashlib.sha256(normalize(text).encode()).hexdigest())
    return values


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("files", nargs="+", type=Path)
    args = parser.parse_args()
    sets = [load(path) for path in args.files]
    overlaps = {}
    contaminated = False
    for i, left in enumerate(sets):
        for j, right in enumerate(sets[i + 1 :], i + 1):
            count = len(left & right)
            overlaps[f"{args.files[i].name}:{args.files[j].name}"] = count
            contaminated |= count > 0
    result = {"contaminated": contaminated, "files": [str(path) for path in args.files], "exact_normalized_overlaps": overlaps}
    print(json.dumps(result))
    if contaminated:
        sys.exit(2)


if __name__ == "__main__":
    main()
