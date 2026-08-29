#!/usr/bin/env python3
"""Fail closed when consumer term settlement bridges Ledger value through floating point.

The consumer entry API keeps a few legacy ``f64`` display fields in its game state. Those fields
are allowed to remain readable, but a term-exchange request must carry only the exact whole-credit
``amount_credits`` authority. A rejected compatibility resolution is carried as an error marker
so the adapter can preserve room/account precondition statuses before failing closed.
"""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
problems: list[str] = []


def read(relative: str) -> str:
    path = ROOT / relative
    if not path.is_file():
        problems.append(f"missing required file: {relative}")
        return ""
    return path.read_text(encoding="utf-8")


term = read("services/consumer-entry-api/src/term_exchange_backend.rs")
consumer_lib = read("services/consumer-entry-api/src/lib.rs")
league = read("services/consumer-entry-api/src/league_routes.rs")
world = read("services/consumer-entry-api/src/world_commerce_routes.rs")
world_runtime = read("services/consumer-entry-api/src/world_routes.rs")
matrix = read("docs/protocol/version-compatibility-matrix-v1.md")


def require(source: str, marker: str, label: str) -> None:
    if marker not in source:
        problems.append(f"{label} lacks required marker: {marker}")


require(term, "pub(super) amount_credits: i64", "term exchange request")
require(term, "amount_validation_error: Option<String>", "term exchange request")
require(term, "whole_credits_to_minor_units", "term exchange adapter")
require(term, ".checked_mul(factor)", "term exchange integer conversion")
require(term, '"amount_minor": amount_minor.to_string()', "term exchange Ledger body")
require(term, '"amount_authority": "amount_credits"', "term exchange failure evidence")
require(term, "validate_exact_ledger_response", "exact Ledger response validator")
require(term, "malformed_exact_response_receipt", "malformed Ledger response fail-closed path")
require(term, "ambiguous_ledger_status", "ambiguous Ledger status recovery")
require(term, "read_bounded_ledger_body", "bounded Ledger response body")
require(term, "MAX_LEDGER_RESPONSE_BYTES", "Ledger response body cap")
require(consumer_lib, "ledger_http: Client", "dedicated Ledger HTTP client")
require(consumer_lib, "reqwest::redirect::Policy::none()", "Ledger redirect fail-closed policy")
require(consumer_lib, ".timeout(Duration::from_secs(20))", "Ledger HTTP timeout")
require(term, "10_f64.powi", "bounded exact-to-display conversion")
require(term, "exact ledger response effect account_id", "exact response identity binding")
require(
    term,
    "fractional compatibility settlement amount requires an exact minor-unit contract",
    "fractional fail-closed guard",
)
require(
    matrix,
    "whole credits convert only by checked scale multiplication; fractional exact state fails closed",
    "protocol compatibility matrix",
)

# The adapter request itself must not carry a floating-point value field. f64 display values may
# exist in the domain state, but they are resolved (or rejected) before this boundary.
if re.search(r"pub\(super\)\s+amount\s*:\s*f64", term):
    problems.append("term exchange request still exposes an authoritative/display f64 amount field")

# No caller may silently round or cast a value into amount_credits. The only allowed resolver is
# the checked compatibility shim, which rejects fractional values rather than inventing a value.
for label, source in (("league settlement", league), ("world contract settlement", world)):
    if re.search(r"amount_credits\s*:\s*[^,\n]*(?:round\s*\(|as\s+i64)", source):
        problems.append(f"{label} still derives amount_credits with rounding/cast")
    require(source, "whole_credits_from_compatibility_amount", label)

# World task reward events are value-bearing projections too; they must use the same exact
# resolver instead of a second float-to-integer conversion.
if re.search(
    r"credits_delta\s*:\s*[^,\n]*reward_amount[^,\n]*(?:round\s*\(|as\s+i64)",
    world_runtime,
):
    problems.append("world task reward projection still rounds/casts reward_amount")
require(world_runtime, "whole_credits_from_compatibility_amount", "world task reward projection")
require(
    world_runtime,
    "settle_world_tactics_reward_with_ledger",
    "world tactics reward settlement",
)
require(world_runtime, '"ledger_required": true', "world tactics reward settlement")
require(world_runtime, '"amount_authority": "amount_credits"', "world tactics reward settlement")
require(
    world_runtime,
    "mark_world_tactics_reward_settled",
    "world tactics post-receipt projection",
)
require(
    world,
    "seller_reopen_settlement.amount_credits == Some(expected_seller_net_credits)",
    "world reopen exact seller settlement guard",
)

# Value-bearing compatibility projections must never fall back to a raw f64/legacy field.  Keep
# these checks intentionally narrow so unrelated gameplay arithmetic remains allowed.
for label, source, patterns in (
    (
        "world tactics reward projection",
        world_runtime,
        (
            r"earned_credits\s*\+=\s*reward_credits\s+as\s+f64",
            r"earned_credits\s*\+=\s*reward\.amount",
        ),
    ),
    (
        "world contract reward projection",
        world,
        (r"earned_credits\s*\+=\s*completion\.reward_amount",),
    ),
    (
        "league reward projection",
        league,
        (r"earned_credits\s*\+=\s*reward\.amount",),
    ),
):
    for pattern in patterns:
        if re.search(pattern, source):
            problems.append(f"{label} still uses unauthenticated legacy amount: {pattern}")

# A direct f64-to-minor bridge is the specific regression this check protects against. Keep the
# pattern narrow so harmless read/display conversions (minor_to_f64) and geometry math do not
# trigger it.
for forbidden in (
    r"exact_minor_from_compatibility_amount",
    r"request\.amount\s*,\s*TERM_EXCHANGE_EXACT_CURRENCY_SCALE",
    r"amount\s*\*\s*factor\s+as\s+f64",
    r"amount\s*\.round\(\)\s*as\s+i64",
):
    if re.search(forbidden, term + league + world + world_runtime):
        problems.append(f"forbidden consumer monetary bridge present: {forbidden}")

result = {
    "status": "failed" if problems else "ok",
    "authority": "amount_credits",
    "minor_conversion": "checked_integer_scale_multiplication",
    "fractional_policy": "fail_closed",
    "callers_checked": [
        "league settlement",
        "world contract settlement",
        "world task reward projection",
    ],
    "problems": problems,
}
print(json.dumps(result, ensure_ascii=False, indent=2, sort_keys=True))
raise SystemExit(1 if problems else 0)
