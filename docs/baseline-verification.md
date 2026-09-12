# Baseline verification

Verified on 2026-09-12 with Python 3.14.7. The implementation uses the standard
library and targets Python 3.10+; other Python versions were not run here.

## Independent, blind test authors

The public [behavior contract](baseline-engines.md) was provided before new
implementation and test authoring. Two newly spawned agents received no prior
conversation and separate directories containing only the contract, RULES.md,
clarification-rules.md, and the trusted card-game engine. Their permitted reads
excluded old and new bot implementation, observation/match implementation,
repository tests, and implementation-agent history. They could also read the
ponytail skill. They were instructed not to import or execute the implementation.

- `blind_policy_tests`: 35 policy tests and two grid tests. Syntax and rule
  fixtures were checked against the trusted core only.
- `blind_knowledge_tests`: ten observation/knowledge tests and 18 match/evaluation
  tests, plus one separate mapping regression. Syntax and rule fixtures were
  checked without importing the implementation.

Both authors attested that they never saw or ran the implementation. Root copied
all five frozen files byte-for-byte and ran them afterward. Isolation was by
fresh context, separate directories, and explicit read restrictions; it was not
an OS-enforced access boundary. SHA-256 digests below identify the frozen tests.

The independent audit found that an opponent roster documented as a mapping
accepted only dictionaries. The still-blind author wrote the separate mapping
regression from the public contract. It failed for both `UserDict` and
`MappingProxyType`; changing shared roster validation to `Mapping` made it pass.
No frozen tests were modified to fit the implementation.

```text
f17d1177b93f215a477401f80a7c2965a8fc2c5ee340d41952fbee9abc5006e3  test_policies.py
b5cbb8854a8ffac2255e93f78309fb96f2f6d5bfa9c98ef4ea47fb2ef3b44ebc  test_grid.py
50d9955e1da342b0bf5d3926a88a530d627f7d7973204db1e5987de0fd85b918  test_observation.py
bdc7a5f1dca43cd48718f5e67c41cacbcd4f5e497f5c5e05554d34212f271712  test_matches.py
593b399c41df310644c822a13b515d96f21865d78ac6177efb978b918d8f0167  test_mapping_roster.py
```

## Results and audit

- All **66 frozen baseline tests pass**, unchanged.
- `python3 -B -m unittest -q`: **148 tests pass**, including original rules,
  move generation, web bridge/HTTP tests, and new baselines. The HTTP tests need
  permission to bind a local loopback port.
- `python3 -B -m src.engines.baseline --grid --deals 1 --max-decisions 2`:
  **1,331 named configurations and 5,324 candidate games**. This deliberately
  short run verifies orchestration, not playing strength or tuned parameters.
- A separate read-only `final_audit` agent inspected the contract, implementation,
  rules, and frozen tests. Final result: **no outstanding actionable findings**.
- Its independent event-based oracle matched **3,569 transitions** across 36
  matches, including **355 recycling events** and **705 reveals**. It also checked
  observation privacy, RNG independence, tactical ordering, legal decisions,
  accounting, seat exchange, fixed-opponent evaluation, and CLI repeatability.

The trusted engine and rule documents were not modified. Trusted-engine SHA-256:
`90c9677d669bc11da6f802ddc2fa8dfad3b449ce437c82b6f70c66fe3accd1d7`.

These checks establish the specified baseline behavior; they do not establish
optimal play. Full grid tuning and evaluation on separate held-out deals remain
experiments to run with the supplied evaluator.
