#!/usr/bin/env python3
"""Apply the exact CEX single-link build-evidence repair to a temporary clone."""

from __future__ import annotations

from pathlib import Path
import sys


def replace_once(path: Path, old: str, new: str, label: str) -> None:
    text = path.read_text(encoding="utf-8")
    if text.count(old) != 1:
        raise SystemExit(f"{label} anchor did not match exactly once in {path}")
    path.write_text(text.replace(old, new), encoding="utf-8", newline="\n")


def patch_collector(root: Path) -> None:
    path = root / "scripts/trnm_build_evidence.py"
    replace_once(
        path,
        'STEP_KEYS = ("lock", "format", "tests", "clippy", "build", "source_unchanged")\n',
        'STEP_KEYS = ("lock", "format", "tests", "clippy", "build", "stage_binary", "source_unchanged")\n',
        "stage-binary outcome key",
    )
    old_reader = '''def read_regular(path: Path, limit: int) -> bytes:
    path = plain_path(path)
    try:
        fd = os.open(path, os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0) | getattr(os, "O_NONBLOCK", 0))
        with os.fdopen(fd, "rb") as stream:
            before = os.fstat(stream.fileno())
            if not stat.S_ISREG(before.st_mode) or before.st_size > limit or before.st_nlink != 1:
                raise EvidenceError("nonregular, linked, or oversized evidence input")
            value = stream.read(limit + 1)
            after = os.fstat(stream.fileno())
        if len(value) > limit or (before.st_dev, before.st_ino, before.st_size, before.st_mtime_ns) != (
                after.st_dev, after.st_ino, after.st_size, after.st_mtime_ns):
            raise EvidenceError("evidence input changed during acquisition")
        return value
    except OSError:
        raise EvidenceError("required evidence input is unavailable") from None
'''
    new_reader = '''def _read_stable_regular(path: Path, limit: int, *, require_single_link: bool) -> bytes:
    path = plain_path(path)
    try:
        fd = os.open(path, os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0) | getattr(os, "O_NONBLOCK", 0))
        with os.fdopen(fd, "rb") as stream:
            before = os.fstat(stream.fileno())
            if (not stat.S_ISREG(before.st_mode) or before.st_size > limit
                    or (require_single_link and before.st_nlink != 1)):
                raise EvidenceError("nonregular, linked, or oversized evidence input")
            value = stream.read(limit + 1)
            after = os.fstat(stream.fileno())
        if len(value) > limit or (before.st_dev, before.st_ino, before.st_size, before.st_mtime_ns) != (
                after.st_dev, after.st_ino, after.st_size, after.st_mtime_ns):
            raise EvidenceError("evidence input changed during acquisition")
        return value
    except OSError:
        raise EvidenceError("required evidence input is unavailable") from None


def read_regular(path: Path, limit: int) -> bytes:
    return _read_stable_regular(path, limit, require_single_link=True)


def read_build_output(path: Path, limit: int) -> bytes:
    # Cargo may materialize its final executable as a hard link to a deps entry.
    # The fixed build path remains descriptor-safe, stable, regular and bounded;
    # only its link-count check is deferred to the byte-identical external stage.
    return _read_stable_regular(path, limit, require_single_link=False)


def external_staged_binary(root: Path, path: Path) -> Path:
    path = plain_path(path)
    try:
        path.relative_to(root)
    except ValueError:
        return path
    raise EvidenceError("staged candidate binary must be outside the source checkout")
'''
    replace_once(path, old_reader, new_reader, "stable reader split")

    old_collect_start = '''def collect(root: Path, output_parent: Path, environment: Mapping[str, str]) -> Path:
    root, output_parent = plain_path(root), plain_path(output_parent)
'''
    new_collect_start = '''def collect(
    root: Path,
    output_parent: Path,
    environment: Mapping[str, str],
    binary_path: Path | None = None,
) -> Path:
    root, output_parent = plain_path(root), plain_path(output_parent)
'''
    replace_once(path, old_collect_start, new_collect_start, "collector staged-binary signature")

    old_binary = '''    inputs["trnm-economy-service"] = read_regular(root / BINARY, MAX_BINARY)
    validate_binary(inputs["trnm-economy-service"])
'''
    new_binary = '''    build_output_path = root / BINARY
    if binary_path is None:
        packet_binary_path = build_output_path
        inputs["trnm-economy-service"] = read_regular(packet_binary_path, MAX_BINARY)
    else:
        packet_binary_path = external_staged_binary(root, binary_path)
        build_output = read_build_output(build_output_path, MAX_BINARY)
        inputs["trnm-economy-service"] = read_regular(packet_binary_path, MAX_BINARY)
        if inputs["trnm-economy-service"] != build_output:
            raise EvidenceError("staged candidate binary differs from the Cargo build output")
    validate_binary(inputs["trnm-economy-service"])
'''
    replace_once(path, old_binary, new_binary, "staged binary acquisition")

    old_recheck = '''        for name, relative in {**SOURCE_FILES, "trnm-economy-service": BINARY}.items():
            if read_regular(root / relative, MAX_BINARY if name == "trnm-economy-service" else MAX_TEXT) != inputs[name]:
                raise EvidenceError("build input changed while collecting packet")
'''
    new_recheck = '''        for name, relative in SOURCE_FILES.items():
            if read_regular(root / relative, MAX_TEXT) != inputs[name]:
                raise EvidenceError("build input changed while collecting packet")
        if binary_path is None:
            build_output = read_regular(build_output_path, MAX_BINARY)
        else:
            build_output = read_build_output(build_output_path, MAX_BINARY)
            if read_regular(packet_binary_path, MAX_BINARY) != inputs["trnm-economy-service"]:
                raise EvidenceError("staged candidate binary changed while collecting packet")
        if build_output != inputs["trnm-economy-service"]:
            raise EvidenceError("Cargo build output changed while collecting packet")
'''
    replace_once(path, old_recheck, new_recheck, "binary and source recheck")

    replace_once(
        path,
        '''    parser.add_argument("--output-parent", type=Path)
    parser.add_argument("--verify", type=Path)
''',
        '''    parser.add_argument("--output-parent", type=Path)
    parser.add_argument("--binary", type=Path)
    parser.add_argument("--verify", type=Path)
''',
        "binary CLI argument",
    )
    replace_once(
        path,
        '''        if args.verify is not None:
            if args.output_parent is not None:
                raise EvidenceError("verification cannot also collect a packet")
''',
        '''        if args.verify is not None:
            if args.output_parent is not None or args.binary is not None:
                raise EvidenceError("verification cannot also collect a packet")
''',
        "verify mode argument isolation",
    )
    replace_once(
        path,
        '''            print(collect(args.root, args.output_parent, os.environ))
''',
        '''            print(collect(args.root, args.output_parent, os.environ, args.binary))
''',
        "CLI staged-binary forwarding",
    )


def patch_tests(root: Path) -> None:
    path = root / "scripts/test-trnm-build-evidence.py"
    replace_once(
        path,
        '''import os
from pathlib import Path
import subprocess
''',
        '''import os
from pathlib import Path
import shutil
import subprocess
''',
        "test shutil import",
    )
    replace_once(
        path,
        '''    def collect(self, **env):
        return E.collect(self.root, self.output, {**self.env, **env})
''',
        '''    def collect(self, *, binary_path=None, **env):
        return E.collect(
            self.root,
            self.output,
            {**self.env, **env},
            binary_path=binary_path,
        )
''',
        "test collector staged-binary helper",
    )

    hardlink_test = '''    def test_hardlinked_binary_is_rejected(self):
        os.link(self.root / E.BINARY, self.parent / 'linked-binary')
        with self.assertRaises(E.EvidenceError): self.collect()
        self.assert_no_packet()
'''
    staged_tests = hardlink_test + '''
    def test_external_single_link_stage_accepts_hardlinked_cargo_output(self):
        os.link(self.root / E.BINARY, self.parent / 'linked-build-output')
        staged = self.parent / 'staged-binary'
        shutil.copyfile(self.root / E.BINARY, staged)
        staged.chmod(0o700)
        destination = self.collect(binary_path=staged)
        record = E.verify_packet(destination)
        self.assertEqual(record['sha256']['trnm-economy-service'], E.digest(staged.read_bytes()))

    def test_external_stage_must_be_outside_checkout_and_byte_identical(self):
        inside = self.write('target/staged-binary', (self.root / E.BINARY).read_bytes())
        with self.assertRaises(E.EvidenceError): self.collect(binary_path=inside)
        staged = self.parent / 'mismatched-stage'
        shutil.copyfile(self.root / E.BINARY, staged)
        with staged.open('ab') as stream:
            stream.write(b'mismatch')
        with self.assertRaises(E.EvidenceError): self.collect(binary_path=staged)
        self.assert_no_packet()

    def test_external_stage_rejects_links(self):
        staged = self.parent / 'staged-binary'
        staged.symlink_to(self.root / E.BINARY)
        with self.assertRaises(E.EvidenceError): self.collect(binary_path=staged)
        staged.unlink()
        shutil.copyfile(self.root / E.BINARY, staged)
        os.link(staged, self.parent / 'staged-hardlink')
        with self.assertRaises(E.EvidenceError): self.collect(binary_path=staged)
        self.assert_no_packet()
'''
    replace_once(path, hardlink_test, staged_tests, "external staging regressions")

    old_commands = '''                        'cargo build -p trnm-economy-service --release --locked',
                        'scripts/check-trnm-economy-settlement-contract.py',
'''
    new_commands = '''                        'cargo build -p trnm-economy-service --release --locked',
                        'install -m 0700 target/release/trnm-economy-service',
                        'cmp --silent -- target/release/trnm-economy-service',
                        'scripts/check-trnm-economy-settlement-contract.py',
'''
    replace_once(path, old_commands, new_commands, "workflow stage command contract")
    replace_once(
        path,
        '''        self.assertIn('--output-parent "$RUNNER_TEMP"', self.workflow)
        self.assertIn('path: ${{ steps.packet.outputs.directory }}/', self.workflow)
''',
        '''        self.assertIn('--output-parent "$RUNNER_TEMP"', self.workflow)
        self.assertIn('--binary "${{ steps.stage_binary.outputs.binary }}"', self.workflow)
        self.assertIn('path: ${{ steps.packet.outputs.directory }}/', self.workflow)
''',
        "workflow staged-binary CLI contract",
    )


def patch_workflow(root: Path) -> None:
    path = root / ".github/workflows/trnm-economy-settlement.yml"
    build_step = '''      - name: Build immutable candidate binary
        id: build
        run: cargo build -p trnm-economy-service --release --locked
      - name: Verify candidate sources remain unchanged
'''
    staged_step = '''      - name: Build immutable candidate binary
        id: build
        run: cargo build -p trnm-economy-service --release --locked
      - name: Stage single-link candidate binary
        id: stage_binary
        run: |
          set -euo pipefail
          stage_dir="$RUNNER_TEMP/trnm-economy-stage"
          rm -rf -- "$stage_dir"
          install -d -m 0700 "$stage_dir"
          install -m 0700 target/release/trnm-economy-service "$stage_dir/trnm-economy-service"
          test "$(stat -c %h "$stage_dir/trnm-economy-service")" = 1
          cmp --silent -- target/release/trnm-economy-service "$stage_dir/trnm-economy-service"
          printf 'binary=%s\\n' "$stage_dir/trnm-economy-service" >> "$GITHUB_OUTPUT"
      - name: Verify candidate sources remain unchanged
'''
    replace_once(path, build_step, staged_step, "workflow single-link stage")
    replace_once(
        path,
        '''          TRNM_BUILD_OUTCOME: ${{ steps.build.outcome }}
          TRNM_SOURCE_UNCHANGED_OUTCOME: ${{ steps.source_unchanged.outcome }}
''',
        '''          TRNM_BUILD_OUTCOME: ${{ steps.build.outcome }}
          TRNM_STAGE_BINARY_OUTCOME: ${{ steps.stage_binary.outcome }}
          TRNM_SOURCE_UNCHANGED_OUTCOME: ${{ steps.source_unchanged.outcome }}
''',
        "workflow stage outcome binding",
    )
    replace_once(
        path,
        '''          packet_dir="$(python3 scripts/trnm_build_evidence.py --root "$PWD" --output-parent "$RUNNER_TEMP")"
''',
        '''          packet_dir="$(python3 scripts/trnm_build_evidence.py --root "$PWD" --output-parent "$RUNNER_TEMP" --binary "${{ steps.stage_binary.outputs.binary }}")"
''',
        "workflow staged-binary collector invocation",
    )


def main() -> int:
    if len(sys.argv) != 2:
        print("usage: patch-cex-evidence-stage.py ROOT", file=sys.stderr)
        return 64
    root = Path(sys.argv[1]).resolve()
    patch_collector(root)
    patch_tests(root)
    patch_workflow(root)
    print("CEX_EVIDENCE_STAGE_PATCH=PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
