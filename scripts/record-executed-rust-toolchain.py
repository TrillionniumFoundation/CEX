#!/usr/bin/env python3
"""Record a verified compiler identity from the actual nested build directory."""
from __future__ import annotations

import json
import os
from pathlib import Path
import subprocess
import sys


def observe() -> dict[str, str]:
    expected = os.environ['RUST_TOOLCHAIN']
    release_commit = os.environ['RUST_RELEASE_COMMIT']
    if os.environ.get('RUSTUP_TOOLCHAIN') != expected:
        raise ValueError('RUSTUP_TOOLCHAIN must explicitly override nested toolchain files')
    active = subprocess.check_output(['rustup', 'show', 'active-toolchain'], text=True).strip()
    rustc = subprocess.check_output(['rustc', '--version', '--verbose'], text=True)
    cargo = subprocess.check_output(['cargo', '--version', '--verbose'], text=True)
    fields = dict(line.split(': ', 1) for line in rustc.splitlines() if ': ' in line)
    if fields.get('release') != expected or fields.get('commit-hash') != release_commit:
        raise ValueError('executed compiler release or commit does not match the policy')
    if not active.startswith(expected + '-'):
        raise ValueError('active toolchain does not match the policy')
    return {
        'rust_toolchain': fields['release'],
        'rust_release_commit': fields['commit-hash'],
        'active_toolchain': active,
        'rustc_version_verbose': rustc,
        'cargo_version_verbose': cargo,
    }


def main() -> int:
    try:
        identity = observe()
        Path(sys.argv[1]).write_text(json.dumps(identity, indent=2, sort_keys=True) + '\n')
    except (KeyError, IndexError, ValueError, OSError, subprocess.CalledProcessError) as error:
        print(f'compiler identity verification failed: {error}', file=sys.stderr)
        return 1
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
