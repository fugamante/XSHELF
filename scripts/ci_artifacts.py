#!/usr/bin/env python3
"""Stage only generated, regular CI failure files for artifact upload."""

import errno
import os
import re
import stat
import sys
from pathlib import Path


def expected_files(os_name: str) -> tuple[str, ...]:
    if re.fullmatch(r"[A-Za-z0-9-]+", os_name) is None:
        raise ValueError("invalid runner OS label")
    return (
        f"rust_check_{os_name}.log",
        f"compat_check_{os_name}.log",
        f"shell_regression_{os_name}.log",
        f"cargo_outdated_{os_name}.log",
        f"summary_{os_name}.txt",
        f"reliability_{os_name}.txt",
    )


def copy_regular(source_dir: int, target_dir: int, name: str) -> bool:
    flags = os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK
    try:
        source = os.open(name, flags, dir_fd=source_dir)
    except FileNotFoundError:
        return False
    except OSError as error:
        if error.errno in (errno.ELOOP, errno.ENXIO, errno.ENODEV):
            return False
        raise

    try:
        info = os.fstat(source)
        if not stat.S_ISREG(info.st_mode):
            return False
        target = os.open(
            name,
            os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW,
            0o600,
            dir_fd=target_dir,
        )
        try:
            remaining = info.st_size
            while remaining:
                chunk = os.read(source, min(remaining, 65536))
                if not chunk:
                    raise OSError("artifact changed while staging")
                view = memoryview(chunk)
                while view:
                    written = os.write(target, view)
                    if written <= 0:
                        raise OSError("short artifact staging write")
                    view = view[written:]
                remaining -= len(chunk)
        except BaseException:
            os.unlink(name, dir_fd=target_dir)
            raise
        finally:
            os.close(target)
        return True
    finally:
        os.close(source)


def stage(source: Path, target: Path, os_name: str) -> list[str]:
    names = expected_files(os_name)
    directory_flags = os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW
    source_dir = os.open(source, directory_flags)
    try:
        os.mkdir(target, 0o700)
        target_dir = os.open(target, directory_flags)
        try:
            return [name for name in names if copy_regular(source_dir, target_dir, name)]
        finally:
            os.close(target_dir)
    finally:
        os.close(source_dir)


def main() -> int:
    if len(sys.argv) != 4:
        print("usage: ci_artifacts.py SOURCE_DIR TARGET_DIR RUNNER_OS", file=sys.stderr)
        return 2
    try:
        copied = stage(Path(sys.argv[1]), Path(sys.argv[2]), sys.argv[3])
    except (OSError, ValueError) as error:
        print(f"failed to stage CI artifacts: {error}", file=sys.stderr)
        return 1
    print(f"staged {len(copied)} generated CI artifact files")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
