# Paper Raid: Reproducible Synthetic Ablation

This is the audited legacy-golden qualification lane. It is not a Challenge Pack activation and it grants no ranking or economic authority.

## Objective

Reproduce the deterministic signal-threshold baseline on the supplied public synthetic dataset, retain the intentionally failed threshold run in the paper evidence, and deliver one evidence-bound ablation as a short paper.

## Frozen materials

- `dataset.csv` contains twelve synthetic observations and no natural-person data.
- `baseline.py` is the immutable golden-kernel reference implementation.
- `evaluator.py` is the candidate-sealed strict Review adapter.

The baseline result must identify run `baseline-seed-17`, seed `17`, mode `baseline`, and the exact metrics derived from the frozen CSV. Preserve failed work and provenance; do not replace any material or infer authority from a mutable catalog.
