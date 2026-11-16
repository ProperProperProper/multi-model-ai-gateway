#!/usr/bin/env python3
"""Minimal stdio MCP server for Multi-Model AI Gateway.

The server never handles provider credentials. It calls the local Multi-Model AI Gateway
gateway, which enforces the verified-free NVIDIA allowlist, local MLX routing,
health checks, and full-access policy.
"""
from __future__ import annotations

import json
import os
import sys
from urllib.error import HTTPError
from urllib.request import Request, urlopen

GATEWAY = os.environ.get("MULTI_MODEL_GATEWAY_URL", "http://127.0.0.1:8788").rstrip("/")
PROVIDERS = {"auto", "local", "nvidia", "apple"}


def http_json(path: str, payload: dict | None = None) -> dict:
    if payload is None:
        request = Request(GATEWAY + path)
    else:
        request = Request(GATEWAY + path, data=json.dumps(payload).encode(), headers={"content-type": "application/json"})
    try:
        with urlopen(request, timeout=90) as response:
            return json.loads(response.read())
    except HTTPError as exc:
        retry_after = exc.headers.get("Retry-After") if exc.headers else None
        suffix = f" retry_after={retry_after}s" if retry_after else ""
        raise RuntimeError(f"gateway_http_status={exc.code}{suffix}") from exc


def result(text: str, structured: dict | None = None) -> dict:
    response = {"content": [{"type": "text", "text": text}]}
    if structured is not None:
        response["structuredContent"] = structured
    return response


def call_tool(name: str, arguments: dict) -> dict:
    if name == "gateway_health":
        status = http_json("/api/status")
        return result(json.dumps(status), status)
    if name == "gateway_chat":
        prompt = str(arguments.get("prompt", "")).strip()
        provider = str(arguments.get("provider", "nvidia"))
        model = str(arguments.get("model", "auto"))
        if not prompt:
            raise ValueError("prompt is required")
        if provider not in PROVIDERS:
            raise ValueError(f"provider must be one of {sorted(PROVIDERS)}")
        requested_max_tokens = min(max(int(arguments.get("max_tokens", 512)), 1), 4096)
        # The bundled MLX server accepts at most 512 output tokens. Keep the
        # MCP contract aligned with that backend; the Rust gateway also clamps
        # at its final upstream boundary for non-MCP clients.
        max_tokens = min(requested_max_tokens, 512) if provider == "local" else requested_max_tokens
        payload = {
            "model": model,
            "messages": [{"role": "user", "content": prompt}],
            "max_tokens": max_tokens,
            "extra_body": {
                "provider": provider,
                "agent": False,
                "bounded_controller": provider == "local",
            },
        }
        # Mutation workers need the final JSON object, not a long reasoning
        # trace that consumes the output budget. NVIDIA-compatible endpoints
        # support this template hint; providers that ignore it remain safe
        # because the client still parses defensively and can fall back to MLX.
        payload["chat_template_kwargs"] = {"enable_thinking": False}
        response = http_json("/v1/chat/completions", payload)
        message = response.get("choices", [{}])[0].get("message", {})
        text = message.get("content") or message.get("reasoning_content") or json.dumps(response)
        return result(text, {"text": text, "routing": response.get("_multi_model_gateway", {}), "model": response.get("model")})
    if name == "gateway_router_sample":
        return result(json.dumps({"prompt": arguments.get("prompt", ""), "provider": arguments.get("provider", "nvidia"), "source": "mcp-sample"}))
    raise ValueError(f"unknown tool: {name}")


def handle(request: dict) -> dict | None:
    request_id = request.get("id")
    method = request.get("method")
    if method == "initialize":
        return {"jsonrpc": "2.0", "id": request_id, "result": {"protocolVersion": "2024-11-05", "capabilities": {"tools": {}}, "serverInfo": {"name": "multi-model-ai-gateway-mcp", "version": "1.0.0"}}}
    if method == "notifications/initialized":
        return None
    if method == "tools/list":
        tools = [
            {"name": "gateway_health", "description": "Inspect the Multi-Model AI Gateway, MLX, Apple, and NVIDIA status.", "inputSchema": {"type": "object", "properties": {}}},
            {"name": "gateway_chat", "description": "Chat through Multi-Model AI Gateway. NVIDIA is constrained to the verified free model allowlist; explicit local MLX requests are capped at 512 output tokens.", "inputSchema": {"type": "object", "properties": {"prompt": {"type": "string"}, "provider": {"type": "string", "enum": sorted(PROVIDERS), "default": "nvidia"}, "model": {"type": "string", "description": "Optional verified model id; use auto for gateway routing."}, "max_tokens": {"type": "integer", "minimum": 1, "maximum": 4096}}, "required": ["prompt"]}},
            {"name": "gateway_router_sample", "description": "Create a reviewable routing-example record; it does not train or deploy anything.", "inputSchema": {"type": "object", "properties": {"prompt": {"type": "string"}, "provider": {"type": "string", "enum": sorted(PROVIDERS)}}}},
        ]
        return {"jsonrpc": "2.0", "id": request_id, "result": {"tools": tools}}
    if method == "tools/call":
        try:
            params = request.get("params", {})
            return {"jsonrpc": "2.0", "id": request_id, "result": call_tool(params.get("name", ""), params.get("arguments", {}))}
        except Exception as error:
            return {"jsonrpc": "2.0", "id": request_id, "error": {"code": -32000, "message": str(error)}}
    if request_id is not None:
        return {"jsonrpc": "2.0", "id": request_id, "error": {"code": -32601, "message": f"method not found: {method}"}}
    return None


for line in sys.stdin:
    if line.strip():
        response = handle(json.loads(line))
        if response is not None:
            print(json.dumps(response), flush=True)
