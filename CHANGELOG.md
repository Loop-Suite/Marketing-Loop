# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.1] - 2026-08-10

Patch release covering bugs found by a real-execution review pass (haiku backend,
`--prior`-chained rounds) run against the v0.1.0 tag; see `evals/README.md` for the full
account.

### Fixed

- [#25](https://github.com/Loop-Suite/Marketing-Loop/issues/25) `select_lenses`'s own prompt
  asked the model to pick lenses fitting "the content type and its nature" but `content_type`
  was never actually passed into the prompt-construction call, even though `main.rs` had it
  available. A capable model can sometimes paper over the gap; a weaker one (haiku) correctly
  refused to guess, surfacing as a hard "Lens selection failed" crash instead of a silently
  arbitrary lens selection.
- [#27](https://github.com/Loop-Suite/Marketing-Loop/issues/27) The `--prior` fix-check
  carry-over filter checked for a literal `CONFIRMED` status, so blocking-tier `MERGED` findings
  (which #17 already made score/verdict-relevant in 0.1.0) were invisible to `fixcheck::run` and
  silently dropped from cross-round tracking instead of being re-checked. The filter now uses
  `quantify::counts_toward_score`, the same predicate #17 uses for score/verdict, so cross-round
  tracking stays consistent with whatever a round's own numbers actually counted.

[0.1.1]: https://github.com/Loop-Suite/Marketing-Loop/releases/tag/v0.1.1

## [0.1.0] - 2026-08-10

Initial tagged release. `marketing-loop` is a Rust CLI port of
[Code-Review-Loop](https://github.com/Loop-Suite/Code-Review-Loop)'s persona+discourse review
architecture onto the marketing-content-review domain: independent per-persona ("lens") review,
followed by discourse cross-validation (AGREE/CHALLENGE/CONNECT/SURFACE), deterministic
policy/compliance checks, and multi-round review chaining via `--prior`.

Everything below was found and fixed across two static-review passes and one real-execution
review pass against the actual CLI (`codereview review --backend claude --model haiku`); see
`evals/README.md` for the full account, including which bugs static review caught for free and
which only surfaced by actually running the binary.

### Added

- `review`/`describe`/`improve`/`ask` subcommands: LLM-backed lens selection, independent
  per-lens review, discourse cross-validation, deterministic policy checks, and Markdown report
  generation (`report.rs`).
- `--prior` round chaining: carries forward unresolved (`STILL_OPEN`) findings from a previous
  round's `--out` directory into the current round via `fixcheck.rs`.
- `--backend claude-cli|openrouter` LLM backends, with a separate `--cheap-model` for
  lower-stakes calls (lens selection, requirements verification, fix-check).
- First unit test suite (71 tests) covering `state.rs`, `quantify.rs`, `checks.rs`/`policy.rs`,
  and `discourse.rs` core logic, plus explicit-JSON-null deserialization tolerance for every
  LLM-facing struct — this repo had zero tests before.
- `evals/README.md`: an empirical record of every bug below, including which were caught by
  static review alone vs. only by running the real binary, and real per-run API cost.

### Fixed

- [#2](https://github.com/Loop-Suite/Marketing-Loop/issues/2) `state::load` crashed with "Is a
  directory" when `--prior` was given a directory path (the documented, natural way to pass a
  previous round's `--out`).
- [#3](https://github.com/Loop-Suite/Marketing-Loop/issues/3) `fixcheck::run` never received the
  revised content, so FIXED/STILL_OPEN verdicts were guessed blind from the prior finding alone.
- [#4](https://github.com/Loop-Suite/Marketing-Loop/issues/4) Console output printed the
  pre-`--prior`-merge verdict/score while `report.md` printed the post-merge one — same run, two
  disagreeing answers depending on where you looked.
- [#5](https://github.com/Loop-Suite/Marketing-Loop/issues/5) `content_length_check` compared a
  word count against a limit documented and configured as a character count.
- [#6](https://github.com/Loop-Suite/Marketing-Loop/issues/6) A round's own local finding id
  could collide with a re-confirmed `--prior` finding's id, silently resurrecting a `REJECTED`
  finding as `CONFIRMED` and double-counting its score deduction. Fixed by namespacing carried-
  over prior findings with a `prior-` id prefix.
- [#7](https://github.com/Loop-Suite/Marketing-Loop/issues/7) `requirements::verify` ran before
  the `--prior` merge, so re-confirmed findings were invisible to the requirements-verification
  LLM call.
- [#8](https://github.com/Loop-Suite/Marketing-Loop/issues/8) Findings that discourse `CONNECT`ed
  (merged) into another finding were completely dropped from `report.md` — no record they ever
  existed.
- [#9](https://github.com/Loop-Suite/Marketing-Loop/issues/9) An LLM response containing an
  explicit JSON `null` (not just a missing field) crashed the whole run and discarded every
  finding already computed in it, across 6 files sharing the same deserialization pattern. Fixed
  with a shared `null_as_default` deserializer.
- [#10](https://github.com/Loop-Suite/Marketing-Loop/issues/10) CONNECT moves never actually
  merged findings: the model reliably returns multiple target ids as one comma-joined string
  (e.g. `"claims_compliance-3,copy_craft-1"`), but `target_finding_id` was typed as a single
  `String`, so the id lookup always failed silently.
- [#18](https://github.com/Loop-Suite/Marketing-Loop/issues/18) Extended the #9 fix to required
  fields (no `#[serde(default)]` at all) across `discourse.rs`, `lens.rs`, `describe.rs`,
  `improve.rs`, `fixcheck.rs`, and `requirements.rs` that had the identical explicit-null crash
  exposure but were outside #9's original fix scope.

### Security

- [#17](https://github.com/Loop-Suite/Marketing-Loop/issues/17) A finding from a `tier =
  "blocking"` lens (e.g. `claims_compliance`) that discourse merged (`CONNECT`) into another
  finding was invisible to `quantify::score`/`verdict`, letting a real, unaddressed compliance
  violation resolve to `APPROVE` — observed on a real run (score 100/100 on copy making three
  separate unsubstantiated guarantee claims). Blocking-tier `MERGED` findings now still weigh on
  score/verdict; non-blocking-tier `MERGED` findings keep the original consolidation behavior.
- [#19](https://github.com/Loop-Suite/Marketing-Loop/issues/19) The marketing content under
  review — written by the party with the strongest incentive to get an `APPROVE` — was spliced
  into every LLM prompt verbatim with no delimiter distinguishing it from instructions. Added
  explicit framing telling the model that content-under-review is untrusted data, and any
  embedded instruction-like text inside it is grounds for a finding, never an instruction to
  obey. Defense-in-depth, not a complete prompt-injection defense.

[0.1.0]: https://github.com/Loop-Suite/Marketing-Loop/releases/tag/v0.1.0
