#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 3 ]]; then
  echo "usage: $0 <repo-root> <base-sha> <head-sha>" >&2
  exit 2
fi

repo_root="$1"
base_sha="$2"
head_sha="$3"

cd "$repo_root"

changed="$(git diff --name-only "$base_sha" "$head_sha")"
cmd_changed="$(echo "$changed" | grep -E '^(bin/(cx|xs|xshelf)(-(install|uninstall))?|lib/cx\.sh|cx\.sh|rust/cxrs/src/app/mod\.rs|rust/cxrs/src/modules/(.*cmd.*\.rs|help_data\.rs|native_dispatch\.rs|compat_dispatch\.rs|command_names\.rs|routing\.rs))$' || true)"

if [[ -z "$cmd_changed" ]]; then
  echo "command surface: unchanged"
  exit 0
fi

required_docs=(
  "CHANGELOG.md"
  "README.md"
  "docs/project/XSHELF_RENAME_MIGRATION.md"
)

missing=()
for path in "${required_docs[@]}"; do
  if ! echo "$changed" | grep -qx "$path"; then
    missing+=("$path")
  fi
done

if [[ ${#missing[@]} -gt 0 ]]; then
  echo "Command surface changed without required compatibility docs updates." >&2
  echo "Changed command files:" >&2
  echo "$cmd_changed" >&2
  echo "Missing required doc updates:" >&2
  printf '%s\n' "${missing[@]}" >&2
  exit 1
fi

echo "command surface doc gate: PASS"
