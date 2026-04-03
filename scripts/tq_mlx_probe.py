#!/usr/bin/env python3

from __future__ import annotations

import argparse
import json
import os
import pathlib
import re
import subprocess
import sys
import time


ROOT = pathlib.Path(__file__).resolve().parents[1]
PROMPTS = [
    ("smoke", ROOT / "docs/tq_prompts/smoke.txt", 8, "exact_OK"),
    ("context_fill", ROOT / "docs/tq_prompts/context_fill.txt", 64, "non_empty_summary"),
    ("retrieval", ROOT / "docs/tq_prompts/retrieval.txt", 16, "exact_TURBO-314159"),
    ("instruct", ROOT / "docs/tq_prompts/instruct.txt", 48, "exact_JSON_contract"),
]


def parse_args() -> argparse.Namespace:
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)

    run = sub.add_parser("run")
    run.add_argument("--model", required=True)
    run.add_argument("--out", required=True)
    run.add_argument("--ctx", type=int, default=8192)
    run.add_argument(
        "--python",
        default=os.environ.get("CX_TQ_MLX_PYTHON", "/tmp/cx_mlx_env/bin/python"),
    )
    return ap.parse_args()


def parse_metric(pattern: str, text: str) -> float | None:
    m = re.search(pattern, text, re.M)
    if not m:
        return None
    return float(m.group(1))


def parse_response(text: str) -> str:
    m = re.search(r"=+\n(.*?)\n=+", text, re.S)
    if not m:
        return text.strip()
    return m.group(1).strip()


def judge(prompt_name: str, response: str) -> tuple[bool, str]:
    if prompt_name == "smoke":
        return response == "OK", "exact_OK"
    if prompt_name == "retrieval":
        return response == "TURBO-314159", "exact_TURBO-314159"
    if prompt_name == "context_fill":
        return bool(response.strip()), "non_empty_summary"
    if prompt_name == "instruct":
        try:
            obj = json.loads(response)
        except Exception:
            return False, "exact_JSON_contract"
        return obj == {
            "status": "ready",
            "focus": "turboquant-baseline",
            "next_step": "phase2-v-cache",
        }, "exact_JSON_contract"
    return False, "unknown"


def run_one(py: str, model: str, prompt_file: pathlib.Path, ctx: int, max_tokens: int) -> dict:
    prompt = prompt_file.read_text()
    cmd = [
        py,
        "-m",
        "mlx_lm",
        "generate",
        "--model",
        model,
        "--prompt",
        prompt,
        "--max-tokens",
        str(max_tokens),
        "--temp",
        "0",
        "--max-kv-size",
        str(ctx),
        "--verbose",
        "true",
    ]
    start = time.monotonic()
    proc = subprocess.run(cmd, capture_output=True, text=True, check=False)
    wall_ms = round((time.monotonic() - start) * 1000)
    out = (proc.stdout or "") + (proc.stderr or "")
    response = parse_response(out)
    prompt_tps = parse_metric(r"Prompt:\s+\d+\s+tokens,\s+([0-9.]+)\s+tokens-per-sec", out)
    decode_tps = parse_metric(r"Generation:\s+\d+\s+tokens,\s+([0-9.]+)\s+tokens-per-sec", out)
    peak_mem_gb = parse_metric(r"Peak memory:\s+([0-9.]+)\s+GB", out)
    return {
        "ok": proc.returncode == 0,
        "wall_ms": wall_ms,
        "response_text": response,
        "prompt_tokens_per_sec": prompt_tps,
        "decode_tokens_per_sec": decode_tps,
        "peak_memory_gb": peak_mem_gb,
        "raw_output": out,
    }


def main() -> int:
    ns = parse_args()
    if ns.cmd != "run":
        return 1

    results = []
    for name, prompt_file, max_tokens, _rule in PROMPTS:
        one = run_one(ns.python, ns.model, prompt_file, ns.ctx, max_tokens)
        passed, quality_rule = judge(name, one["response_text"])
        results.append(
            {
                "mode": "mlx",
                "prompt_name": name,
                "prompt_file": str(prompt_file.relative_to(ROOT)),
                "context_target": ns.ctx,
                "predict_n": max_tokens,
                "quality_rule": quality_rule,
                "passed": passed and one["ok"],
                "response_text": one["response_text"],
                "prompt_tokens_per_sec": one["prompt_tokens_per_sec"],
                "decode_tokens_per_sec": one["decode_tokens_per_sec"],
                "peak_memory_gb": one["peak_memory_gb"],
                "wall_ms": one["wall_ms"],
            }
        )

    payload = {
        "contract_version": "turboquant-mlx.v1",
        "backend": "mlx",
        "python": ns.python,
        "model": ns.model,
        "context_target": ns.ctx,
        "runs": results,
        "passes": sum(1 for r in results if r["passed"]),
        "total": len(results),
        "metric_note": "peak_memory_gb is the current MLX memory proxy; raw_ratio is not yet available on this backend path",
    }
    pathlib.Path(ns.out).write_text(json.dumps(payload, indent=2) + "\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
