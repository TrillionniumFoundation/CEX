#!/usr/bin/env python3
"""Digest-sealed loader for audited Paper Raid Python evaluators.

The Node Bridge verifies and copies these exact loader bytes before invoking
``/usr/bin/python3 -I -S -B``.  This loader then re-verifies every executable
byte, removes non-stdlib import roots, and injects the sole optional support
module from its sealed bytes.  It is intentionally not a general Python
runner.
"""

from __future__ import annotations

import hashlib
from importlib.machinery import BuiltinImporter, FrozenImporter, PathFinder
import os
import stat
import sys
import types


_DIGEST_PREFIX = "sha256:"
_FORBIDDEN_MODULES = frozenset(
    {
        "pip",
        "setuptools",
        "site",
        "sitecustomize",
        "usercustomize",
    }
)
_SUPPORT_MODULE = "baseline"
_CONTEXT_MODULE = "hepta_review_context"
_MAIN_PATH = "evaluator/main.py"
_SUPPORT_PATH = "evaluator/baseline.py"
_CANDIDATE_PATH = "inputs/candidate.json"
_DATASET_PATHS = frozenset({"inputs/dataset.csv", "inputs/dataset.json"})


class _SealedLoaderError(Exception):
    """An authority or import boundary failed closed."""


def _valid_digest(value: str) -> bool:
    return (
        len(value) == len(_DIGEST_PREFIX) + 64
        and value.startswith(_DIGEST_PREFIX)
        and all(character in "0123456789abcdef" for character in value[7:])
    )


def _under(path: str, roots: tuple[str, ...]) -> bool:
    resolved = os.path.realpath(path)
    for root in roots:
        try:
            if os.path.commonpath((resolved, root)) == root:
                return True
        except ValueError:
            continue
    return False


def _read_sealed(root: str, logical_path: str, expected_digest: str) -> bytes:
    if not _valid_digest(expected_digest):
        raise _SealedLoaderError("object digest is not canonical sha256")
    destination = os.path.join(root, *logical_path.split("/"))
    if os.path.normpath(destination) != destination or not _under(destination, (root,)):
        raise _SealedLoaderError("object path escapes the private execution root")
    flags = os.O_RDONLY | getattr(os, "O_CLOEXEC", 0) | getattr(os, "O_NOFOLLOW", 0)
    try:
        descriptor = os.open(destination, flags)
    except OSError as error:
        raise _SealedLoaderError("sealed object could not be opened") from error
    try:
        before = os.fstat(descriptor)
        if (
            not stat.S_ISREG(before.st_mode)
            or before.st_uid != os.geteuid()
            or before.st_nlink != 1
            or before.st_mode & 0o277
        ):
            raise _SealedLoaderError("sealed object ownership or mode is invalid")
        chunks: list[bytes] = []
        while True:
            chunk = os.read(descriptor, 1024 * 1024)
            if not chunk:
                break
            chunks.append(chunk)
        after = os.fstat(descriptor)
        if (
            before.st_dev != after.st_dev
            or before.st_ino != after.st_ino
            or before.st_size != after.st_size
            or before.st_mtime_ns != after.st_mtime_ns
        ):
            raise _SealedLoaderError("sealed object changed while being read")
    finally:
        os.close(descriptor)
    content = b"".join(chunks)
    actual_digest = _DIGEST_PREFIX + hashlib.sha256(content).hexdigest()
    if actual_digest != expected_digest:
        raise _SealedLoaderError("sealed object digest does not match its execution pin")
    return content


def _trusted_stdlib_paths() -> tuple[str, ...]:
    roots = tuple(
        dict.fromkeys(
            os.path.realpath(value)
            for value in (sys.base_prefix, sys.base_exec_prefix)
            if value
        )
    )
    trusted: list[str] = []
    for value in sys.path:
        if not value:
            continue
        resolved = os.path.realpath(value)
        components = set(resolved.split(os.sep))
        if (
            {"site-packages", "dist-packages"} & components
            or not _under(resolved, roots)
        ):
            continue
        trusted.append(resolved)
    if not trusted:
        raise _SealedLoaderError("isolated Python exposed no trusted stdlib paths")
    return tuple(dict.fromkeys(trusted))


def _trusted_import_origin(origin: str | None, roots: tuple[str, ...]) -> bool:
    return origin in {"built-in", "frozen"} or (
        isinstance(origin, str) and _under(origin, roots)
    )


class _StdlibOnlyFinder:
    """Resolve imports only from the interpreter's isolated stdlib roots."""

    trusted_paths: tuple[str, ...] = ()
    stdlib_modules = frozenset(sys.stdlib_module_names)

    @classmethod
    def find_spec(cls, fullname: str, path=None, target=None):  # noqa: ANN001
        root_name = fullname.partition(".")[0]
        if root_name in _FORBIDDEN_MODULES or root_name not in cls.stdlib_modules:
            raise ModuleNotFoundError(
                f"module {root_name!r} is outside the sealed stdlib/support allowlist"
            )
        search_path = cls.trusted_paths if path is None else tuple(path)
        if any(not _under(value, cls.trusted_paths) for value in search_path):
            raise ModuleNotFoundError("package import path escaped trusted stdlib roots")
        spec = PathFinder.find_spec(fullname, search_path, target)
        if spec is None:
            return None
        if not _trusted_import_origin(spec.origin, cls.trusted_paths):
            raise ModuleNotFoundError("module origin escaped trusted stdlib roots")
        locations = spec.submodule_search_locations
        if locations is not None and any(
            not _under(value, cls.trusted_paths) for value in locations
        ):
            raise ModuleNotFoundError("package search path escaped trusted stdlib roots")
        return spec


def _lock_imports() -> None:
    trusted = _trusted_stdlib_paths()
    _StdlibOnlyFinder.trusted_paths = trusted
    sys.path[:] = trusted
    sys.path_importer_cache.clear()
    sys.meta_path[:] = [BuiltinImporter, FrozenImporter, _StdlibOnlyFinder]
    for name in _FORBIDDEN_MODULES:
        sys.modules.pop(name, None)


def _install_support(content: bytes) -> None:
    if _SUPPORT_MODULE in sys.modules:
        raise _SealedLoaderError("support module was loaded before sealed injection")
    module = types.ModuleType(_SUPPORT_MODULE)
    module.__file__ = f"sealed://{_SUPPORT_PATH}"
    module.__loader__ = None
    module.__package__ = ""
    module.__spec__ = None
    sys.modules[_SUPPORT_MODULE] = module
    try:
        code = compile(content, module.__file__, "exec", dont_inherit=True)
        exec(code, module.__dict__)
    except BaseException:
        sys.modules.pop(_SUPPORT_MODULE, None)
        raise


def _install_context(kind: str) -> None:
    if _CONTEXT_MODULE in sys.modules:
        raise _SealedLoaderError("review context module was loaded before sealed injection")
    module = types.ModuleType(_CONTEXT_MODULE)
    module.__file__ = "sealed://runtime/hepta_review_context"
    module.__loader__ = None
    module.__package__ = ""
    module.__spec__ = None
    module.KIND = kind
    sys.modules[_CONTEXT_MODULE] = module


def _run() -> int:
    if len(sys.argv) != 7:
        raise _SealedLoaderError("fixed loader argv contract is invalid")
    (
        evaluator_digest,
        dataset_path,
        dataset_digest,
        candidate_digest,
        support_digest,
        review_kind,
    ) = sys.argv[1:]
    if dataset_path not in _DATASET_PATHS:
        raise _SealedLoaderError("dataset logical path is unsupported")
    if review_kind not in {"evaluate", "reproduce"}:
        raise _SealedLoaderError("review execution kind is unsupported")
    root = os.path.realpath(os.path.join(os.path.dirname(__file__), os.pardir))
    evaluator = _read_sealed(root, _MAIN_PATH, evaluator_digest)
    _read_sealed(root, dataset_path, dataset_digest)
    _read_sealed(root, _CANDIDATE_PATH, candidate_digest)
    support_path = os.path.join(root, *_SUPPORT_PATH.split("/"))
    support: bytes | None = None
    if support_digest == "-":
        if os.path.lexists(support_path):
            raise _SealedLoaderError("unexpected evaluator support object is present")
    else:
        support = _read_sealed(root, _SUPPORT_PATH, support_digest)

    _lock_imports()
    _install_context(review_kind)
    if support is not None:
        _install_support(support)
    sys.argv[:] = [
        f"sealed://{_MAIN_PATH}",
        os.path.join(root, *dataset_path.split("/")),
        os.path.join(root, *_CANDIDATE_PATH.split("/")),
    ]
    globals_for_main = {
        "__builtins__": __builtins__,
        "__file__": f"sealed://{_MAIN_PATH}",
        "__loader__": None,
        "__name__": "__main__",
        "__package__": None,
        "__spec__": None,
    }
    code = compile(evaluator, globals_for_main["__file__"], "exec", dont_inherit=True)
    exec(code, globals_for_main)
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(_run())
    except _SealedLoaderError as error:
        print(f"sealed evaluator loader rejected execution: {error}", file=sys.stderr)
        raise SystemExit(2) from None
