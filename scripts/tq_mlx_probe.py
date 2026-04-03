#!/usr/bin/env python3

from __future__ import annotations

import argparse
import json
import os
import pathlib
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
        help="Kept for artifact provenance; the script itself should already be running under the desired interpreter.",
    )
    return ap.parse_args()


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


def main() -> int:
    ns = parse_args()
    if ns.cmd != "run":
        return 1

    import mlx.core as mx
    from mlx_lm import load, stream_generate
    from mlx_lm.models import cache
    from mlx_lm.sample_utils import make_sampler

    model, tokenizer = load(ns.model)
    sampler = make_sampler(
        0.0,
        1.0,
        0.0,
        1,
        top_k=0,
        xtc_probability=0.0,
        xtc_threshold=0.0,
        xtc_special_tokens=tokenizer.encode("\n") + list(tokenizer.eos_token_ids),
    )

    results = []
    for name, prompt_file, max_tokens, _rule in PROMPTS:
        prompt = prompt_file.read_text()
        prompt_text = tokenizer.apply_chat_template(
            [{"role": "user", "content": prompt}],
            tokenize=False,
            add_generation_prompt=True,
        )
        prompt_cache = cache.make_prompt_cache(model, max_kv_size=ns.ctx)
        mx.reset_peak_memory()
        start = time.monotonic()
        text = ""
        last = None
        for resp in stream_generate(
            model,
            tokenizer,
            prompt_text,
            max_tokens=max_tokens,
            sampler=sampler,
            max_kv_size=ns.ctx,
            prompt_cache=prompt_cache,
        ):
            text += resp.text
            last = resp
        mx.synchronize()
        wall_ms = round((time.monotonic() - start) * 1000)
        response_text = text.strip()
        passed, quality_rule = judge(name, response_text)
        cache_nbytes = sum(c.nbytes for c in prompt_cache)
        cache_tokens = max((getattr(c, "size", lambda: 0)() for c in prompt_cache), default=0)
        results.append(
            {
                "mode": "mlx",
                "prompt_name": name,
                "prompt_file": str(prompt_file.relative_to(ROOT)),
                "context_target": ns.ctx,
                "predict_n": max_tokens,
                "quality_rule": quality_rule,
                "passed": passed,
                "response_text": response_text,
                "prompt_tokens_per_sec": None if last is None else last.prompt_tps,
                "decode_tokens_per_sec": None if last is None else last.generation_tps,
                "peak_memory_gb": None if last is None else last.peak_memory,
                "cache_nbytes": cache_nbytes,
                "cache_tokens": cache_tokens,
                "wall_ms": wall_ms,
            }
        )

    payload = {
        "contract_version": "turboquant-mlx.v2",
        "backend": "mlx",
        "python": ns.python,
        "model": ns.model,
        "context_target": ns.ctx,
        "runs": results,
        "passes": sum(1 for r in results if r["passed"]),
        "total": len(results),
        "metric_note": "MLX now records both peak_memory_gb and live cache_nbytes. cache_nbytes is the preferred cache-footprint metric on this backend path.",
    }
    pathlib.Path(ns.out).write_text(json.dumps(payload, indent=2) + "\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
