#!/usr/bin/env python3
"""Generate and validate the code-derived CEX contract inventory."""

from __future__ import annotations

import argparse
import json
from pathlib import Path

from repository_contract_facts import build_inventory, check_result


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()

    inventory = build_inventory(args.root)
    output_label: str | None = None
    if args.output is not None:
        output_path = args.output if args.output.is_absolute() else args.root / args.output
        output_path.parent.mkdir(parents=True, exist_ok=True)
        output_path.write_text(json.dumps(inventory, indent=2, ensure_ascii=False, sort_keys=True) + "\n", encoding="utf-8")
        try:
            output_label = output_path.relative_to(args.root.resolve()).as_posix()
        except ValueError:
            output_label = str(output_path)
    print(json.dumps(check_result(inventory, output_label), indent=2, ensure_ascii=False, sort_keys=True))
    return 0 if inventory.get("status") == "ok" else 1


if __name__ == "__main__":
    raise SystemExit(main())
