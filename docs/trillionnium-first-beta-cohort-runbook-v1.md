# Trillionnium First-Beta Cohort Runbook v1

Purpose: collect **real human** first-session evidence for Trillionnium World without lowering the bar or mixing synthetic browser E2E into beta-readiness scoring.

## What this gate proves

The existing browser and first-human E2E gates prove the product can complete the loop mechanically. This cohort gate proves 5-10 actual first-time players can understand and finish the first loop:

1. land on `/world`
2. identify who they are
3. know where to go
4. tap the primary action
5. train before attack
6. win/finish the first route
7. claim reward
8. open the next route

## Non-negotiables

- Use 5-10 real participants.
- Use fresh or reset sessions.
- Do not coach the next click while the task is running.
- Do not put emails, phone numbers, handles, or other direct contact info in the evidence file.
- Synthetic/browser runs are rejected by the validator.
- Keep OSM live ingestion disabled, do not promote MapLibre, and keep Rust as the source of truth.

## Evidence file

Save real observations as JSON, then run:

```bash
CEX_ENV_FILE=run/local-production/.env \
TRILLIONNIUM_FIRST_BETA_COHORT_EVIDENCE_PATH=run/first-beta-cohort/cohort-YYYYMMDD.json \
scripts/check-trillionnium-first-beta-cohort-evidence.sh
```

The script writes:

```text
run/first-beta-cohort/first-beta-cohort-summary-<epoch>.json
```

Green thresholds:

- participant count: 5-10
- completion rate: >= 80%
- reward claim rate: >= 80%
- next-route-opened rate: >= 70%
- median time to first action: <= 90s
- median time to reward claim: <= 600s
- average confusion events: <= 2 per participant

## Evidence schema

A copyable template is tracked at:

```text
docs/templates/trillionnium-first-beta-cohort-evidence-template-v1.json
```

Keep `template: true` while drafting. The validator must reject template files; only flip it to `false` after replacing every placeholder with real, consented, anonymized observations.

Use this shape; anonymized participant ids only:

```json
{
  "contract_version": "trillionnium_first_beta_cohort_evidence_v1",
  "synthetic": false,
  "template": false,
  "cohort_name": "first-beta-YYYYMMDD",
  "operator_attestation": {
    "real_human_participants": true,
    "fresh_or_reset_sessions": true,
    "no_staff_coaching_during_task": true,
    "notes": "Participants were asked to complete the first route on /world without step-by-step coaching."
  },
  "participants": [
    {
      "participant_id": "p01",
      "consent_obtained": true,
      "fresh_or_reset_session": true,
      "staff_prompted_next_click": false,
      "steps": {
        "first_screen_understood": true,
        "opened_primary_cta": true,
        "entered_tactics_board": true,
        "trained_before_attack": true,
        "completed_first_route": true,
        "reward_claimed": true,
        "next_route_opened": true
      },
      "timings_seconds": {
        "time_to_first_action": 42,
        "time_to_reward_claim": 360
      },
      "confusion_events": [
        {
          "category": "copy_unclear",
          "step": "first_screen",
          "note": "Keep short and non-identifying."
        }
      ],
      "dropoff_step": null,
      "observer_notes": "No personal data. Describe UX only."
    }
  ]
}
```

## What to fix from the summary

Use the summary's `top_confusion_categories` and participant `dropoff_step` fields to decide the next UI slice:

- first-screen confusion -> tighten who/where/click/reward copy
- primary CTA missed -> move/relabel CTA or reduce competing visual weight
- training-before-attack failure -> strengthen route hint and default action
- reward claim failure -> make reward result and next-route handoff more explicit
- next-route drop-off -> turn claim result into a stronger route continuation

Do not raise first-beta playability above 9 until this gate is green with real participants.
