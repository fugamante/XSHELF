#!/usr/bin/env python3
"""Write compatibility reports without following repository-selected default paths."""

import argparse
import errno
import os
import secrets
import stat
import sys


_DIR_FLAGS = os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW
_FILE_FLAGS = os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW


def _report_dir(root):
    root_fd = os.open(root, _DIR_FLAGS)
    opened = [root_fd]
    try:
        for name in (".cx", "compat"):
            try:
                os.mkdir(name, mode=0o700, dir_fd=opened[-1])
            except FileExistsError:
                pass
            opened.append(os.open(name, _DIR_FLAGS, dir_fd=opened[-1]))
        return opened
    except BaseException:
        for fd in reversed(opened):
            os.close(fd)
        raise


def _check_leaf(dir_fd, leaf):
    try:
        info = os.stat(leaf, dir_fd=dir_fd, follow_symlinks=False)
    except FileNotFoundError:
        return
    if not stat.S_ISREG(info.st_mode) or info.st_nlink != 1:
        raise OSError(errno.EINVAL, "default report target must be a single regular file", leaf)
    # Atomic replacement must not bypass a read-only existing report.
    if not os.access(leaf, os.W_OK, dir_fd=dir_fd, follow_symlinks=False):
        raise OSError(errno.EACCES, "default report target is not writable", leaf)


def write_default(root, leaf, payload):
    """Atomically replace one default report beneath root/.cx/compat."""
    if not leaf or leaf in (".", "..") or os.path.basename(leaf) != leaf:
        raise ValueError("default report leaf must be a single filename")

    opened = _report_dir(root)
    dir_fd = opened[-1]
    temp = None
    try:
        _check_leaf(dir_fd, leaf)
        for _ in range(8):
            candidate = f".{leaf}.{secrets.token_hex(8)}.tmp"
            try:
                file_fd = os.open(candidate, _FILE_FLAGS, 0o600, dir_fd=dir_fd)
            except FileExistsError:
                continue
            temp = candidate
            break
        else:
            raise OSError(errno.EEXIST, "unable to reserve temporary report file")

        with os.fdopen(file_fd, "w", encoding="utf-8") as fh:
            fh.write(payload)
            fh.flush()
            os.fsync(fh.fileno())
        # Recheck for a leaf swapped while the report was being serialized.
        _check_leaf(dir_fd, leaf)
        os.replace(temp, leaf, src_dir_fd=dir_fd, dst_dir_fd=dir_fd)
        temp = None
    finally:
        try:
            if temp is not None:
                os.unlink(temp, dir_fd=dir_fd)
        finally:
            for fd in reversed(opened):
                os.close(fd)


def write_explicit(path, payload):
    """Keep --out's existing operator-selected path behavior."""
    os.makedirs(os.path.dirname(path) or ".", exist_ok=True)
    with open(path, "w", encoding="utf-8") as fh:
        fh.write(payload)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument("--default-root")
    group.add_argument("--out")
    parser.add_argument("--leaf")
    args = parser.parse_args(argv)
    if args.default_root is not None and args.leaf is None:
        parser.error("--leaf is required with --default-root")
    if args.out is not None and args.leaf is not None:
        parser.error("--leaf requires --default-root")
    payload = sys.stdin.read()
    try:
        if args.default_root is not None:
            write_default(args.default_root, args.leaf, payload)
        else:
            write_explicit(args.out, payload)
    except (OSError, ValueError) as exc:
        print(f"compat-report: {exc}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
