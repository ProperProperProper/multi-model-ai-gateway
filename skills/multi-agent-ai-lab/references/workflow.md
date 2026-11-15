# Lab workflow and artifacts

Each run produces:

```text
run.json                 run metadata and configuration
plan.json                planner hypotheses and acceptance criteria
workers/worker-*.json    independent worker outputs and timings
review.json              agreement, risks, and contamination checks
report.md                human-readable result
```

The canonical job shape is in `job_schema.json`. The orchestrator uses the gateway's OpenAI-compatible API and never reads an NVIDIA key. The gateway owns provider selection, free-model enforcement, health checks, and local access policy.

## Worker contract

Every worker must return:

- hypothesis;
- evidence or commands that would verify it;
- proposed change, if any;
- confidence from 0 to 1;
- risks and rollback;
- explicit `needs_review` when it cannot verify a claim.

## Patch policy

The lab generates candidate recommendations and artifacts. Applying a patch is a separate operator action. A future patch adapter should create a branch, run tests, inspect the diff, and require a review gate before merge.

## Backend policy

`nvidia` calls Multi-Model AI Gateway's verified-free NVIDIA provider. `local` calls the MLX backend. `auto` lets the gateway choose. Apple can be selected by calling the gateway directly, but the lab defaults to NVIDIA or MLX for comparable worker results.
