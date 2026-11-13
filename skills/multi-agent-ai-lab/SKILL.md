---
name: multi-agent-ai-lab
description: Run reproducible planner, researcher, parallel-worker, reviewer, and reporter workflows through the Multi-Model AI Gateway's local MLX or verified-free hosted gateway.
metadata:
  short-description: Orchestrate safe multi-agent AI experiments
---

# Multi-agent AI lab

Use this skill when a task benefits from multiple independent hypotheses, code/research workers, model comparison, evaluation, or a report assembled from parallel work.

## Operating model

```text
main branch / operator
          |
       planner
          |
  independent worker jobs
          |
 researcher / reviewer
          |
       reporter
          |
 candidate patch + metrics + report
```

- The planner turns the request into bounded hypotheses and acceptance criteria.
- Workers run independently and must state assumptions, evidence, and failure modes.
- The researcher/reviewer compares outputs and flags contamination, unsupported claims, and overfitting.
- The reporter produces a concise result plus artifacts and next actions.
- The default backend is the local Multi-Model AI Gateway using verified-free hosted routing; use `--provider local` for MLX.
- Runs write to an isolated artifact directory. Main-branch mutations, external GPU jobs, and paid services are never implicit.

## Run

From the repository root:

```sh
python3 skills/multi-agent-ai-lab/scripts/run_lab.py \
  --prompt "Evaluate three approaches to improving the router" \
  --workers 3 \
  --provider nvidia
```

Use `--provider local` for MLX, `--run-dir` to select an artifact location, and `--dry-run` to inspect the job graph without calling a model. Read [references/workflow.md](references/workflow.md) for schemas and review rules.

## Safety and quality

- Keep workers independent; do not let one worker's answer become another worker's evidence without labeling it.
- Preserve raw outputs and prompt hashes for reproducibility, but do not store credentials or private source by default.
- Require reviewer agreement or explicit uncertainty before recommending a patch.
- Reject contaminated train/validation data and report validation metrics, latency, and failures.
- Treat free hosted endpoints as rate-limited development infrastructure. Private data stays on MLX unless the user explicitly authorizes cloud processing.
