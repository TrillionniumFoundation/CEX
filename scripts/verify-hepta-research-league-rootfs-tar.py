#!/usr/bin/env python3
"""Verify the sole allowed empty source-directory marker in a Hepta rootfs tar."""

import argparse
import pathlib
import tarfile


ALLOWED_SOURCE_DIRECTORY = "usr/src"


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--tar", required=True, type=pathlib.Path)
    args = parser.parse_args()

    try:
        if args.tar.is_symlink() or not args.tar.is_file():
            raise ValueError("rootfs tar must be a regular non-symlink file")
        with tarfile.open(args.tar, mode="r:") as archive:
            source_entries = []
            for member in archive.getmembers():
                parts = tuple(part for part in member.name.split("/") if part)
                if "src" not in parts:
                    continue
                if member.name != ALLOWED_SOURCE_DIRECTORY or parts != ("usr", "src"):
                    raise ValueError(
                        f"rootfs tar contains a forbidden source path: {member.name}"
                    )
                source_entries.append(member)
    except (OSError, tarfile.TarError, ValueError) as error:
        parser.error(str(error))

    if len(source_entries) != 1:
        parser.error("rootfs tar must contain exactly one usr/src directory entry")
    source_entry = source_entries[0]
    if not source_entry.isdir() or source_entry.issym() or source_entry.islnk():
        parser.error("rootfs usr/src entry must be a directory, not a link")


if __name__ == "__main__":
    main()
