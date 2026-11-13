#!/usr/bin/env python3
"""Run a safe, parallel Multi-Model AI Gateway experiment lab."""
from __future__ import annotations

import argparse
import concurrent.futures
import hashlib
import json
import os
import time
import uuid
from pathlib import Path
from urllib.request import Request, urlopen


def call_gateway(gateway: str, prompt: str, provider: str, max_tokens: int = 900) -> dict:
    body = {"model": "auto", "messages": [{"role": "user", "content": prompt}], "max_tokens": max_tokens, "extra_body": {"provider": provider, "agent": False}}
    started = time.perf_counter()
    request = Request(gateway.rstrip("/") + "/v1/chat/completions", data=json.dumps(body).encode(), headers={"content-type": "application/json"})
    try:
        with urlopen(request, timeout=120) as response:
            payload = json.loads(response.read())
        message = payload.get("choices", [{}])[0].get("message", {})
        return {"ok": True, "text": message.get("content") or message.get("reasoning_content") or "", "routing": payload.get("_multi_model_gateway", {}), "latency_ms": round((time.perf_counter() - started) * 1000, 1)}
    except Exception as error:
        return {"ok": False, "error": str(error), "latency_ms": round((time.perf_counter() - started) * 1000, 1)}


def gateway_get(gateway: str, path: str) -> dict:
    try:
        with urlopen(gateway.rstrip("/") + path, timeout=10) as response:
            return json.loads(response.read())
    except Exception as error:
        return {"ok": False, "error": str(error)}


def write_json(path: Path, value: dict) -> None:
    path.write_text(json.dumps(value, indent=2) + "\n")


def main() -> None:
    parser = argparse.ArgumentParser(description="Multi-Model AI Gateway planner/worker/reviewer/reporter lab")
    parser.add_argument("--prompt", required=True)
    parser.add_argument("--workers", type=int, default=3)
    parser.add_argument("--provider", choices=["nvidia", "local", "auto"], default="nvidia")
    parser.add_argument("--gateway", default=os.environ.get("MULTI_MODEL_GATEWAY_URL", "http://127.0.0.1:8788"))
    parser.add_argument("--run-dir", type=Path, default=Path("lab_runs"))
    parser.add_argument("--dry-run", action="store_true")
    args = parser.parse_args()
    if not 1 <= args.workers <= 12:
        parser.error("--workers must be between 1 and 12")
    run_id = time.strftime("%Y%m%d-%H%M%S") + "-" + uuid.uuid4().hex[:8]
    root = args.run_dir / run_id
    (root / "workers").mkdir(parents=True, exist_ok=True)
    metadata = {"run_id": run_id, "prompt": args.prompt, "provider": args.provider, "workers": args.workers, "dry_run": args.dry_run, "prompt_sha256": hashlib.sha256(args.prompt.encode()).hexdigest()}
    write_json(root / "run.json", metadata)
    write_json(root / "security.json", gateway_get(args.gateway, "/api/security"))
    plan = {"objective": args.prompt, "hypotheses": [{"id": f"H{i + 1}", "question": f"Develop and test an independent approach to: {args.prompt}", "worker": i + 1} for i in range(args.workers)], "acceptance": ["state evidence", "identify risks", "give a reversible recommendation"]}
    write_json(root / "plan.json", plan)
    if args.dry_run:
        print(json.dumps({"run_id": run_id, "artifact_dir": str(root), "status": "dry_run", "workers": args.workers}))
        return
    worker_prompt = "You are an independent lab worker.\nTask: {task}\nReturn concise JSON with keys hypothesis, evidence, recommendation, confidence (0..1), risks, needs_review. Do not claim to have run commands you did not run."
    jobs = [(item["id"], worker_prompt.format(task=item["question"])) for item in plan["hypotheses"]]
    def run(item: tuple[str, str]) -> tuple[str, dict]:
        return item[0], call_gateway(args.gateway, item[1], args.provider)
    with concurrent.futures.ThreadPoolExecutor(max_workers=args.workers) as pool:
        results = dict(pool.map(run, jobs))
    for worker_id, value in results.items():
        write_json(root / "workers" / f"worker-{worker_id}.json", value)
    evidence = "\n\n".join(f"{worker_id}: {value.get('text', value.get('error', ''))}" for worker_id, value in results.items())
    review_prompt = f"You are the lab reviewer. Compare these independent outputs for the task {args.prompt}. Identify agreement, contradictions, unsupported claims, contamination risks, and the safest next step.\n\n{evidence}"
    review = call_gateway(args.gateway, review_prompt, args.provider)
    write_json(root / "review.json", review)
    report_prompt = f"You are the lab reporter. Write a concise Markdown report for {args.prompt}. Include outcome, evidence, uncertainty, risks, and next action. Do not invent results.\n\nWorker outputs:\n{evidence}\n\nReview:\n{review.get('text', review.get('error', ''))}"
    report = call_gateway(args.gateway, report_prompt, args.provider)
    (root / "report.md").write_text(report.get("text", report.get("error", "No report")) + "\n")
    print(json.dumps({"run_id": run_id, "artifact_dir": str(root), "status": "completed", "successful_workers": sum(value.get("ok", False) for value in results.values()), "review_ok": review.get("ok", False), "report_ok": report.get("ok", False)}))


if __name__ == "__main__":
    main()
