#!/usr/bin/env python3
"""Detect overfitting in JSONL training metrics.

Each row should contain epoch, train_loss, and validation_loss. The command
returns a non-zero exit code when validation loss worsens while training loss
continues improving for the configured patience window.
"""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path


def inspect(path: Path, patience: int = 5, gap: float = 0.05) -> dict:
    rows = [json.loads(line) for line in path.read_text().splitlines() if line.strip()]
    if not rows:
        raise ValueError("metrics file is empty")
    best = min(float(row["validation_loss"]) for row in rows)
    stale = 0
    overfit = False
    for row in rows:
        if float(row["validation_loss"]) > best:
            stale += 1
        else:
            stale = 0
        if stale >= patience and float(row["validation_loss"]) - float(row["train_loss"]) >= gap:
            overfit = True
            break
    return {"overfit": overfit, "best_validation_loss": best, "epochs": len(rows), "recommendation": "restore_best_checkpoint_and_stop" if overfit else "continue_or_evaluate"}


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("metrics", type=Path)
    parser.add_argument("--patience", type=int, default=5)
    parser.add_argument("--gap", type=float, default=0.05)
    args = parser.parse_args()
    result = inspect(args.metrics, args.patience, args.gap)
    print(json.dumps(result))
    if result["overfit"]:
        sys.exit(2)


if __name__ == "__main__":
    main()
