# AI and machine-learning adapters

Thomas AI combines three ML layers:

1. **Deep-learning inference:** MLX runs local quantized language models, Apple Foundation Models provide Apple-native inference, and verified free NVIDIA endpoints provide stronger hosted reasoning.
2. **Learned routing:** `ai/ml/router.py` trains a small softmax classifier from JSONL request examples. The Rust gateway can load its output with `THOMAS_ROUTER_MODEL`; health checks and load balancing still remain authoritative.
3. **Application ML:** the same adapter boundary is intended for trading features, risk classifiers, anomaly detection, backtest diagnostics, and model evaluation.

## Train the learned router

The trainer uses only the Python standard library, so it works without adding a paid service or a heavyweight dependency:

```sh
python3 ai/ml/router.py train ai/datasets/router.jsonl ai/models/router-v1.json
THOMAS_ROUTER_MODEL="$PWD/ai/models/router-v1.json" \
  ./target/release/thomas-ai serve
python3 ai/ml/router.py predict ai/models/router-v1.json \
  "Compare architectures and derive the failure mode step by step"
```

The generated model is intentionally optional and should be evaluated against real request/latency/failure data before becoming the default. Training uses a held-out validation split, L2 regularization, early stopping, and best-checkpoint restoration to limit overfitting. The current heuristic router remains the safe fallback.

## MLX deep-learning path

Use `ai/ml/self_supervised.py corpus` to create unlabeled local training text from code and documentation. This is self-supervised causal-language-model data: the model learns to predict the next token without human labels. Feed the resulting `train.jsonl` and `valid.jsonl` into the MLX-LM training workflow appropriate to the installed MLX-LM version, then keep adapter metadata in `ai/model_registry.json`.

```sh
python3 ai/ml/self_supervised.py corpus ai/data/self-supervised .
# Review the generated data locally, then train with your installed mlx_lm tool.
python3 ai/ml/contamination.py ai/data/self-supervised/train.jsonl ai/data/self-supervised/valid.jsonl
```

The corpus builder deduplicates chunks and splits by source file, preventing adjacent chunks from the same file leaking into both train and validation. The contamination audit rejects exact normalized overlap before training. Store datasets outside Git when they contain private code, customer data, financial data, or credentials. NVIDIA hosted models are inference-only in this project; do not send private training data to them.

For optional outcome learning, export local-only telemetry as JSONL and run `bootstrap-router`. This creates provisional provider preferences from successful latency outcomes; it is not a substitute for a held-out evaluation set.

## Free NVIDIA assistance

`ai/ml/llm_assist.py` can use the running Thomas AI gateway's NVIDIA provider to bootstrap labels, generate review candidates, or critique dataset examples. The gateway chooses only from the verified free NVIDIA allowlist; this script never reads or stores an NVIDIA key. Generated labels are marked `review_required: true`, and contamination/validation checks must run before training.

```sh
python3 ai/ml/llm_assist.py ai/datasets/router.jsonl /tmp/router-llm-labels.jsonl
# Audit the final train/validation files after review, not the source and its labels.
python3 ai/ml/contamination.py /tmp/train.jsonl /tmp/valid.jsonl
```

For MLX training, retain train/validation loss per epoch and run `python3 ai/ml/monitor.py metrics.jsonl`. If validation loss worsens while training loss improves, stop and restore the best validation checkpoint. Increase data diversity before increasing model size or training steps.

## Data contract

Training examples are JSON Lines with a prompt and a desired provider:

```json
{"prompt":"Run the local test suite and edit the repository","provider":"local"}
```

Allowed provider labels are `local`, `nvidia`, and `apple`. Evaluation should measure provider accuracy, latency, failure rate, fallback rate, and cost/quota exposure—not accuracy alone.

