#!/usr/bin/env python3
"""Train and use a tiny softmax router for Thomas AI.

The router predicts local, NVIDIA, or Apple from prompt features. It is a
small learned policy layer; Thomas AI still applies health checks and load
balancing after the prediction.
"""
from __future__ import annotations

import argparse
import json
import math
import random
from pathlib import Path
from typing import Iterable

FEATURES = (
    "length",
    "words",
    "questions",
    "newlines",
    "code",
    "tool_request",
    "reasoning_terms",
    "local_terms",
)
CLASSES = ("local", "nvidia", "apple")
REASONING_TERMS = ("architecture", "debug", "refactor", "prove", "derive", "compare", "optimize")
LOCAL_TERMS = ("private", "offline", "local", "mac", "file", "repository")
TOOL_TERMS = ("run", "execute", "edit", "write", "shell", "command", "terminal", "git")


def features(text: str) -> list[float]:
    lower = text.lower()
    words = lower.split()
    return [
        min(len(text), 12000) / 12000.0,
        min(len(words), 2400) / 2400.0,
        min(text.count("?"), 10) / 10.0,
        min(text.count("\n"), 100) / 100.0,
        1.0 if "```" in text or "traceback" in lower else 0.0,
        sum(lower.count(term) for term in TOOL_TERMS) / 8.0,
        sum(lower.count(term) for term in REASONING_TERMS) / 6.0,
        sum(lower.count(term) for term in LOCAL_TERMS) / 6.0,
    ]


def softmax(values: list[float]) -> list[float]:
    peak = max(values)
    exps = [math.exp(value - peak) for value in values]
    total = sum(exps)
    return [value / total for value in exps]


def predict(model: dict, text: str) -> dict:
    vector = features(text)
    normalized = [
        (value - mean) / scale if scale else value
        for value, mean, scale in zip(vector, model["means"], model["scales"])
    ]
    scores = [
        bias + sum(weight * value for weight, value in zip(weights, normalized))
        for weights, bias in zip(model["weights"], model["bias"])
    ]
    probabilities = softmax(scores)
    index = max(range(len(probabilities)), key=probabilities.__getitem__)
    return {"provider": model["classes"][index], "confidence": probabilities[index], "probabilities": dict(zip(model["classes"], probabilities))}


def read_rows(path: Path) -> list[dict]:
    rows = []
    for line_number, line in enumerate(path.read_text().splitlines(), 1):
        if not line.strip():
            continue
        row = json.loads(line)
        if row.get("provider") not in CLASSES or not row.get("prompt"):
            raise ValueError(f"invalid row {line_number}: expected prompt and provider in {CLASSES}")
        existing = next((item for item in rows if item["prompt"].strip().lower() == row["prompt"].strip().lower()), None)
        if existing and existing["provider"] != row["provider"]:
            raise ValueError(f"conflicting labels for duplicate prompt at row {line_number}")
        if existing:
            continue
        rows.append(row)
    if not rows:
        raise ValueError("dataset is empty")
    return rows


def _metrics(weights: list[list[float]], bias: list[float], vectors: list[list[float]], rows: list[dict]) -> dict:
    if not rows:
        return {"loss": 0.0, "accuracy": 0.0}
    loss = 0.0
    correct = 0
    for vector, row in zip(vectors, rows):
        probabilities = softmax([bias[c] + sum(weights[c][i] * vector[i] for i in range(len(FEATURES))) for c in range(len(CLASSES))])
        target = CLASSES.index(row["provider"])
        loss -= math.log(max(probabilities[target], 1e-12))
        correct += max(range(len(probabilities)), key=probabilities.__getitem__) == target
    return {"loss": loss / len(rows), "accuracy": correct / len(rows)}


def train(rows: Iterable[dict], epochs: int = 600, learning_rate: float = 0.35, patience: int = 60, l2: float = 0.001) -> dict:
    rows = list(rows)
    if len(rows) < 6:
        raise ValueError("at least six examples are required for a validation split")
    if any(sum(row["provider"] == provider for row in rows) < 2 for provider in CLASSES):
        raise ValueError("each provider needs at least two examples")
    random.Random(42).shuffle(rows)
    split = max(1, int(len(rows) * 0.8))
    train_rows, valid_rows = rows[:split], rows[split:]
    vectors = [features(row["prompt"]) for row in rows]
    means = [sum(vector[i] for vector in vectors) / len(vectors) for i in range(len(FEATURES))]
    scales = []
    for i, mean in enumerate(means):
        variance = sum((vector[i] - mean) ** 2 for vector in vectors) / len(vectors)
        scales.append(math.sqrt(variance) or 1.0)
    normalized = [[(value - means[i]) / scales[i] for i, value in enumerate(vector)] for vector in vectors]
    train_vectors, valid_vectors = normalized[:split], normalized[split:]
    weights = [[0.0] * len(FEATURES) for _ in CLASSES]
    bias = [0.0] * len(CLASSES)
    best = None
    best_loss = float("inf")
    stale = 0
    for _ in range(epochs):
        for vector, row in zip(train_vectors, train_rows):
            probabilities = softmax([bias[c] + sum(weights[c][i] * vector[i] for i in range(len(FEATURES))) for c in range(len(CLASSES))])
            target = CLASSES.index(row["provider"])
            for c in range(len(CLASSES)):
                error = (1.0 if c == target else 0.0) - probabilities[c]
                bias[c] += learning_rate * error / len(train_rows)
                for i, value in enumerate(vector):
                    weights[c][i] += learning_rate * (error * value - l2 * weights[c][i]) / len(train_rows)
        validation = _metrics(weights, bias, valid_vectors, valid_rows)
        if validation["loss"] < best_loss - 1e-5:
            best_loss = validation["loss"]
            best = ([row[:] for row in weights], bias[:])
            stale = 0
        else:
            stale += 1
            if stale >= patience:
                break
    if best is not None:
        weights, bias = best
    train_metrics = _metrics(weights, bias, train_vectors, train_rows)
    valid_metrics = _metrics(weights, bias, valid_vectors, valid_rows)
    return {"version": 1, "features": list(FEATURES), "classes": list(CLASSES), "means": means, "scales": scales, "weights": weights, "bias": bias, "training": {"epochs_requested": epochs, "epochs_without_validation_improvement": stale, "train": train_metrics, "validation": valid_metrics, "regularization": l2}}


def main() -> None:
    parser = argparse.ArgumentParser(description="Train or query the Thomas AI learned router")
    sub = parser.add_subparsers(dest="command", required=True)
    train_parser = sub.add_parser("train")
    train_parser.add_argument("dataset", type=Path)
    train_parser.add_argument("output", type=Path)
    train_parser.add_argument("--epochs", type=int, default=600)
    predict_parser = sub.add_parser("predict")
    predict_parser.add_argument("model", type=Path)
    predict_parser.add_argument("prompt", nargs="+")
    args = parser.parse_args()
    if args.command == "train":
        model = train(read_rows(args.dataset), epochs=args.epochs)
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(model, indent=2) + "\n")
        print(json.dumps({"ok": True, "model": str(args.output), "rows": len(read_rows(args.dataset)), "training": model["training"]}))
    else:
        print(json.dumps(predict(json.loads(args.model.read_text()), " ".join(args.prompt))))


if __name__ == "__main__":
    main()
