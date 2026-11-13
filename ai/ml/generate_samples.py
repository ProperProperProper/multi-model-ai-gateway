#!/usr/bin/env python3
"""Generate reproducible, explicitly synthetic router examples."""
from __future__ import annotations

import argparse
import json
import random
from pathlib import Path

TEMPLATES = {
    "local": [
        "Summarize this short note privately on my Mac.",
        "Read the local repository README and list the files.",
        "Run the local test suite and report the result.",
        "Write a small local script to rename these files.",
        "Inspect the Git diff and explain the changed functions.",
        "Keep this request offline and rewrite the sentence.",
        "Open the local config and check its formatting.",
        "Execute the command and show its bounded output.",
    ],
    "nvidia": [
        "Compare several model architectures and derive the trade-offs.",
        "Debug this traceback and explain the root cause step by step.",
        "Design a robust distributed training pipeline with failure recovery.",
        "Prove whether this concurrency strategy is safe under load.",
        "Refactor this large system and identify hidden performance risks.",
        "Analyze the backtest methodology and challenge its assumptions.",
        "Optimize this architecture for difficult multi-stage reasoning.",
        "Evaluate competing approaches and recommend one with evidence.",
    ],
    "apple": [
        "Rewrite this note using Apple-native writing assistance.",
        "Summarize this text privately with the Apple Foundation Model.",
        "Explain how to use Foundation Models in a native Swift app.",
        "Turn this rough sentence into a polished message on macOS.",
        "Suggest a concise Apple platform API design.",
        "Create a friendly accessibility-focused rewrite.",
        "Explain the on-device versus Private Cloud Apple AI path.",
        "Draft a short Swift example using Apple AI services.",
    ],
}


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("output", type=Path)
    parser.add_argument("--rows", type=int, default=120)
    parser.add_argument("--seed", type=int, default=42)
    args = parser.parse_args()
    rng = random.Random(args.seed)
    providers = list(TEMPLATES)
    rows = []
    for index in range(max(args.rows, 6)):
        provider = providers[index % len(providers)]
        prompt = TEMPLATES[provider][rng.randrange(len(TEMPLATES[provider]))]
        prompt = f"{prompt} Sample scenario {index + 1}: preserve safety and explain the result."
        rows.append({"prompt": prompt, "provider": provider, "source": "synthetic-router-v1", "review_required": True})
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text("\n".join(json.dumps(row) for row in rows) + "\n")
    print(json.dumps({"rows": len(rows), "output": str(args.output), "synthetic": True, "review_required": True}))


if __name__ == "__main__":
    main()
