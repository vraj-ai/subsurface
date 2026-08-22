# Confidence has three levels, decided by rules

Confidence is the mechanism that stops someone acting on a Finding that isn't
supported, so it cannot be the model's self-assessment — models rate their own
output badly and confidently, which is the exact failure `docs/adr/0001` guards
against. A 0-100 score would be worse still: it reads as measured when it is
invented.

Three levels, assigned by rule from the shape of the Evidence:

- **Stated** — a commit message, doc, or comment says the why in words.
- **Inferred** — no one wrote the why, but it is reconstructable from what
  changed alongside it.
- **None** — no rationale was recorded. The Finding says so and asserts nothing.

The What/When half of a Finding is derived from git and carries no Confidence;
only the Why half does.
