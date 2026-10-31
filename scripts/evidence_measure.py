#!/usr/bin/env python3
"""Replay synthetic logs through legacy capture and evidence; never run builds."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile


def invoke(binary, project, log, args):
    env = os.environ.copy()
    env.update(CX_LOG_FILE=str(log), CX_CONTEXT_BUDGET_CHARS="12000",
               CX_CONTEXT_BUDGET_LINES="200", CX_CONTEXT_CLIP_MODE="auto",
               CX_CONTEXT_CLIP_FOOTER="1", CX_NATIVE_REDUCE="1",
               CX_CAPTURE_PROFILE="balanced", CX_CAPTURE_PROMPT_PROFILE="legacy")
    out = subprocess.run([str(binary), *args], cwd=project, env=env,
                         capture_output=True, text=True, timeout=60)
    if out.returncode:
        raise RuntimeError(f"replay failed ({out.returncode}): {out.stderr}")
    return out.stdout


def measure(binary, root, name, raw, required):
    project = root / name
    project.mkdir()
    source = project / "synthetic.txt"
    source.write_text(raw)
    store = root / f"{name}-store"
    store.mkdir(mode=0o700)
    legacy = invoke(binary, project, root / "runs.jsonl", ["capture", "cat", str(source)])
    summary = invoke(binary, project, root / "runs.jsonl", ["evidence", "import",
                     "--store", str(store), "--input", str(source),
                     "--profile", name, "--status", "0"])
    result = json.loads(summary)
    detail = invoke(binary, project, root / "runs.jsonl", ["evidence", "show",
                    "--store", str(store), "--id", result["id"], "--start", "1", "--lines", "3"])
    kept = all(s in result["summary"]["text"] for s in required)
    if not kept:
        raise RuntimeError(f"required synthetic evidence lost for {name}")
    return dict(profile=name, raw_chars=len(raw), legacy_response_chars=len(legacy),
                summary_response_chars=len(summary), detail_response_chars=len(detail),
                summary_plus_detail_chars=len(summary) + len(detail),
                required_spans_kept=kept, measured_tokens=None)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    cargo = "".join(f"   Compiling synthetic_{i} v0.1.0\n" for i in range(700))
    cargo += "warning: unused synthetic import\nFinished test profile in 0.1s\ntest result: ok. 24 passed; 0 failed; 0 ignored\n"
    xcode = "".join(f"CompileSwift synthetic_{i}.swift normal arm64\n" for i in range(700))
    xcode += "ld: warning: duplicate libraries: '-lc++'\nTest Suite 'Synthetic' passed\nExecuted 24 tests, with 0 failures in 0.2 seconds\n** TEST SUCCEEDED **\n"
    with tempfile.TemporaryDirectory(prefix="xshelf-evidence-measure-") as temp:
        root = Path(temp).resolve()
        rows = [measure(binary, root, "cargo", cargo, ["24 passed", "unused synthetic import"]),
                measure(binary, root, "xcode", xcode, ["Executed 24 tests", "duplicate libraries"]),
                measure(binary, root, "generic", "validation passed\n", ["validation passed"])]
    print(json.dumps(dict(schema_version=1, basis="synthetic UTF-8 decoded character counts; full JSON responses including one detail page", results=rows), indent=2))


if __name__ == "__main__":
    main()
