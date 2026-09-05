#!/usr/bin/env python3
"""Read a consistent, bounded semantic input set from a complete local checkout.

The checks reject source subsets/sparse missing inputs; they do not replace
Cargo metadata, compilation or exact-commit hosted qualification.
"""
from __future__ import annotations

import fnmatch
import os
from pathlib import Path, PurePosixPath
import stat
import subprocess
import tomllib

MAX_FILE_BYTES = 16 * 1024 * 1024
MAX_TOTAL_BYTES = 256 * 1024 * 1024
MAX_FILES = 20_000
CODE_SUFFIXES = {'.rs', '.js', '.mjs', '.ts', '.tsx'}


def regular_bytes(root: Path, path: Path) -> bytes:
    root = root.resolve()
    if not path.is_absolute():
        path = root / path
    try:
        relative = path.relative_to(root)
    except ValueError:
        raise AssertionError('semantic input is outside the repository') from None
    if '..' in relative.parts:
        raise AssertionError('semantic input contains a parent path')
    for parent in [path, *path.parents]:
        if parent == root:
            break
        if parent.is_symlink():
            raise AssertionError(f'semantic input traverses a symbolic link: {relative.as_posix()}')
    try:
        flags = os.O_RDONLY | getattr(os, 'O_NOFOLLOW', 0) | getattr(os, 'O_NONBLOCK', 0)
        descriptor = os.open(path, flags)
        with os.fdopen(descriptor, 'rb') as stream:
            before = os.fstat(stream.fileno())
            if not stat.S_ISREG(before.st_mode) or before.st_size > MAX_FILE_BYTES:
                raise AssertionError(f'nonregular or oversized semantic input: {relative.as_posix()}')
            data = stream.read(MAX_FILE_BYTES + 1)
            after = os.fstat(stream.fileno())
        if len(data) > MAX_FILE_BYTES or (before.st_ino, before.st_size, before.st_mtime_ns) != (after.st_ino, after.st_size, after.st_mtime_ns):
            raise AssertionError(f'semantic input changed while reading: {relative.as_posix()}')
        return data
    except OSError:
        raise AssertionError(f'required semantic input unavailable: {relative.as_posix()}') from None


class InputSnapshot:
    def __init__(self, root: Path):
        self.root = root.resolve()
        self.inputs: dict[Path, bytes] = {}
        self.total_bytes = 0

    def read(self, path: Path) -> bytes:
        path = path if path.is_absolute() else self.root / path
        if path not in self.inputs:
            data = regular_bytes(self.root, path)
            self.total_bytes += len(data)
            if self.total_bytes > MAX_TOTAL_BYTES or len(self.inputs) >= MAX_FILES:
                raise AssertionError('semantic snapshot budget exceeded')
            self.inputs[path] = data
        return self.inputs[path]

    def verify(self) -> None:
        for path, expected in self.inputs.items():
            if regular_bytes(self.root, path) != expected:
                raise AssertionError(f'semantic input changed during generation: {path.relative_to(self.root).as_posix()}')


def _safe_path(value: str) -> str:
    if not isinstance(value, str):
        raise AssertionError('workspace and source paths must be strings')
    path = PurePosixPath(value)
    if not value or path.is_absolute() or '..' in path.parts or '\\' in value or str(path) != value:
        raise AssertionError('invalid workspace or source path')
    return value


def tracked_paths(root: Path) -> tuple[str, ...]:
    def git(*args: str) -> bytes:
        try:
            result = subprocess.run(['git', '-C', str(root), *args], capture_output=True, timeout=30, check=False)
        except (OSError, subprocess.TimeoutExpired):
            raise AssertionError('Git checkout inventory could not be read') from None
        if result.returncode:
            raise AssertionError('a complete Git checkout is required for semantic generation')
        return result.stdout
    if Path(os.fsdecode(git('rev-parse', '--show-toplevel')).strip()).resolve() != root.resolve():
        raise AssertionError('semantic root must equal the Git checkout root')
    try:
        paths = tuple(value.decode('utf-8') for value in git('ls-files', '--cached', '-z').split(b'\0') if value)
    except UnicodeDecodeError:
        raise AssertionError('Git source paths must use UTF-8') from None
    if not paths or len(paths) > MAX_FILES or len(paths) != len(set(paths)):
        raise AssertionError('Git inventory is empty, conflicted or exceeds its budget')
    for path in paths:
        _safe_path(path)
    return tuple(sorted(paths))


def require_complete_workspace(snapshot: InputSnapshot, modules: list[dict]) -> tuple[str, ...]:
    try:
        manifest = tomllib.loads(snapshot.read(Path('Cargo.toml')).decode('utf-8'))
    except (ValueError, UnicodeError):
        raise AssertionError('invalid workspace manifest') from None
    members = manifest.get('workspace', {}).get('members')
    if not isinstance(members, list) or not members or not all(isinstance(m, str) for m in members):
        raise AssertionError('explicit workspace members are required')
    # CEX currently uses an explicit list. Future glob-based membership requires
    # an intentional checker change, rather than accepting incomplete expansion.
    for member in members:
        _safe_path(member)
        if any(char in member for char in '*?['):
            raise AssertionError('workspace glob membership requires explicit resolution')
    roots = [m['workspace_member'] for m in modules]
    packages = [m['package'] for m in modules]
    if len(packages) != len(set(packages)):
        raise AssertionError('duplicate semantic package identity')
    if len(roots) != len(set(roots)) or len(members) != len(set(members)) or set(roots) != set(members):
        raise AssertionError('semantic catalog does not equal workspace membership')
    for module in modules:
        member = _safe_path(module['workspace_member'])
        try:
            package = tomllib.loads(snapshot.read(Path(member) / 'Cargo.toml').decode('utf-8'))
        except (ValueError, UnicodeError):
            raise AssertionError(f'invalid member manifest: {member}') from None
        if package.get('package', {}).get('name') != module['package']:
            raise AssertionError(f'package/catalog mismatch: {member}')
        entries = module.get('source_entrypoints')
        if not isinstance(entries, list) or not entries:
            raise AssertionError(f'module has no declared source entry points: {member}')
        for entry in entries:
            if not _safe_path(entry).startswith(member + '/'):
                raise AssertionError(f'entry point escapes its module: {member}')
            snapshot.read(Path(entry))
    paths = tracked_paths(snapshot.root)
    for relative in paths:
        path = PurePosixPath(relative)
        in_source = path.parts[0] in {'apps', 'services', 'crates'} and path.suffix in CODE_SUFFIXES
        in_sql = path.suffix == '.sql' and (relative.startswith(('migrations/', 'deploy/sql/')) or fnmatch.fnmatch(relative, 'services/*/migrations/*'))
        if (in_source or in_sql) and not {'target', 'vendor', 'node_modules'}.intersection(path.parts):
            snapshot.read(Path(relative))
    return paths
