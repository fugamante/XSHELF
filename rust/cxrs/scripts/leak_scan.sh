#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$repo_root"

mode="${1:---repo}"

if [[ "$mode" != "--repo" && "$mode" != "--staged" ]]; then
  echo "usage: $0 [--repo|--staged]" >&2
  exit 2
fi

declare -a checks=(
  "local_unix_path|/Users/[A-Za-z0-9_.-]+/"
  "local_home_path|/home/[A-Za-z0-9_.-]+/"
  "windows_user_path|C:\\\\Users\\\\[A-Za-z0-9_.-]+\\\\"
  "email_address|\\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\\.[A-Za-z]{2,}\\b"
  "github_pat|\\bgh[pousr]_[A-Za-z0-9]{20,}\\b"
  "aws_access_key|\\bAKIA[0-9A-Z]{16}\\b"
  "openai_secret|\\bsk-[A-Za-z0-9]{20,}\\b"
  "slack_token|\\bxox[baprs]-[A-Za-z0-9-]{10,}\\b"
  "private_key|-----BEGIN (RSA |EC |OPENSSH )?PRIVATE KEY-----"
)

is_allowed_match() {
  local check_name="$1"
  local matched_line="$2"

  case "$check_name" in
    email_address)
      [[ "$matched_line" =~ users\.noreply\.github\.com|example\.com ]] && return 0
      ;;
  esac

  return 1
}

find_matches() {
  local pattern="$1"
  local physical_path="$2"

  CX_LEAK_PATTERN="$pattern" perl -e '
    use strict;
    use warnings;
    use Fcntl qw(O_RDONLY O_NONBLOCK O_NOFOLLOW S_ISREG);
    my $re = qr/$ENV{CX_LEAK_PATTERN}/;
    my $path = shift @ARGV;
    sysopen(my $fh, $path, O_RDONLY | O_NONBLOCK | O_NOFOLLOW)
      or die "leak-scan: cannot open input: $!\n";
    S_ISREG((stat($fh))[2]) or die "leak-scan: input is not a regular file\n";
    my $line = 0;
    while (1) {
      $! = 0;
      my $text = <$fh>;
      die "leak-scan: input read failed: $!\n" if $!;
      last if !defined($text);
      ++$line;
      print "$line:$text" if $text =~ $re;
    }
    eof($fh) or die "leak-scan: input read failed: $!\n";
    close($fh) or die "leak-scan: input close failed: $!\n";
  ' -- "$physical_path"
}

scan_file_path() {
  local logical_file="$1"
  local physical_path="$2"
  local had_match=0

  for spec in "${checks[@]}"; do
    local check_name="${spec%%|*}"
    local pattern="${spec#*|}"
    local matches
    if ! matches="$(find_matches "$pattern" "$physical_path")"; then
      echo "leak-scan: failed to inspect $logical_file" >&2
      return 1
    fi
    [[ -z "$matches" ]] && continue

    while IFS= read -r hit; do
      [[ -z "$hit" ]] && continue
      local line_no="${hit%%:*}"
      local hit_line="${hit#*:}"
      hit_line="${hit_line%$'\r'}"
      if is_allowed_match "$check_name" "$hit_line"; then
        continue
      fi
      echo "${logical_file}:${line_no}:${hit_line}" >&2
      had_match=1
    done <<< "$matches"
  done

  return "$had_match"
}

# Materialize inventories before reading them so Git failures cannot become PASS.
scan_tmp="$(mktemp -d)"
trap 'rm -rf -- "$scan_tmp"' EXIT

scan_staged() {
  local failed=0 record meta file old_file oid mode
  git diff --cached --raw -z --no-abbrev --diff-filter=ACMR -- > "$scan_tmp/inventory" || return 1
  while IFS= read -r -d '' meta <&3; do
    IFS= read -r -d '' file <&3 || return 1
    # Rename/copy raw records contain both old and new paths.
    case "${meta##* }" in
      R*|C*) old_file="$file"; IFS= read -r -d '' file <&3 || return 1 ;;
    esac
    record="${meta#:}"
    mode="${record#* }"; mode="${mode%% *}"
    oid="${record#* * * }"; oid="${oid%% *}"
    [[ "$mode" == 160000 ]] && continue
    git cat-file blob "$oid" > "$scan_tmp/blob" || return 1
    scan_file_path "$file" "$scan_tmp/blob" || failed=1
  done 3< "$scan_tmp/inventory"
  return "$failed"
}

scan_repo() {
  local failed=0 record meta file mode oid stage
  git ls-files --stage -z -- > "$scan_tmp/inventory" || return 1
  while IFS= read -r -d '' record; do
    meta="${record%%$'\t'*}"
    file="${record#*$'\t'}"
    mode="${meta%% *}"
    oid="${meta#* }"; oid="${oid%% *}"
    stage="${meta##* }"
    [[ "$stage" == 0 ]] || { echo "leak-scan: unmerged index" >&2; return 1; }
    case "$mode" in
      160000) continue ;;
      120000)
        # Scan the tracked link text without following it outside the repository.
        git cat-file blob "$oid" > "$scan_tmp/blob" || return 1
        scan_file_path "$file" "$scan_tmp/blob" || failed=1
        ;;
      100644|100755) scan_file_path "$file" "./$file" || failed=1 ;;
      *) echo "leak-scan: unsupported index mode" >&2; return 1 ;;
    esac
  done < "$scan_tmp/inventory"
  return "$failed"
}

if [[ "$mode" == "--staged" ]]; then
  if ! scan_staged; then
    echo "leak-scan: blocked sensitive/identifiable content in staged changes" >&2
    exit 1
  fi
  echo "leak-scan: staged scan PASS" >&2
  exit 0
fi

if ! scan_repo; then
  echo "leak-scan: blocked sensitive/identifiable content in tracked files" >&2
  exit 1
fi
echo "leak-scan: repo scan PASS" >&2
