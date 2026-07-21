# v2.3.0 legacy private-testnet configuration compatibility

Current v2.3.0 workflows and runbooks should use v2.3.0 or neutral configuration paths.

The following historical configuration family remains tracked for reproducibility only:

- `configs/private-testnet/v2_2_*/*`

These files preserve topology and rehearsal inputs used by v2.2.x evidence. They are not current defaults and must not be copied into new operator instructions without an explicit v2.3.0 review.

Current private-testnet templates belong under `configs/private-testnet/v2_3_0/` or a version-neutral path referenced by a current workflow.
