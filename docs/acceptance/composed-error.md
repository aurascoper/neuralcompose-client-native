# The composed-error read — registration

Date: 2026-08-04
Status: **registration only; the table it governs has not yet been produced**

This document is a **pre-registration**, not a result. It exists to fix the reading rule
before the numbers that rule adjudicates are visible. Sections are labelled `§N` and are
cited by number; the numbering is explicit and does not depend on a renderer.

`crates/neuralcompose-llama/tests/composed_error.rs` already produced one row of numbers.
They live in the body of commit `267082d` (#33) and nowhere else — no document in
`docs/` referenced `composed_error` or `quadrature` before this file. A commit message
cannot be corrected, only superseded by another commit, and it is not where anyone looks.
#32 brought the XDNA evidence under version control for the same reason; this is the same
failure one level worse, because the numbers were adjudicated in the message too.

---

## §1 What is measured

Two error sources were established separately in `quantized_agreement.rs`, recorded in
[`quantized-models.md`](quantized-models.md):

- **quantisation alone** — CPU-f32 vs CPU-Q8_0 — `2.62e-03`
- **backend alone, quantised** — CPU-Q8_0 vs Vk-Q8_0 — `2.63e-03`

Neither is the cell a deployment occupies. An index built on CPU-f32 and queried by a
quantised Vulkan runtime crosses **both** hops at once. `quantized_agreement.rs` measured
every cell except that diagonal, and the diagonal is the one that ships.

The question is whether the two sources compose in quadrature (independent; worst case
bounded) or linearly (correlated; worst case additive).

## §2 Governance — which read is registered

**The absolute bands in the module doc govern** (`composed_error.rs:16-19`):

```
composed <= 4.2e-03  -> independent
composed >= 4.8e-03  -> correlated
in between           -> undecided, and reported as undecided
```

The printed table also emits `quadrat.` and `linear` columns computed per row from that
row's own sources (`composed_error.rs:214-215`). **Those columns are a printed
convenience, not a criterion.**

This is not a claim that the per-row statistic is worse. It is arguably better: it uses
each row's own sources instead of importing short-text sources into a long-text row. The
governing consideration is *when each was chosen*. The absolute bands were committed
before the test was run. The per-row columns became a candidate reading only after the
numbers existed, and selecting between two rules once you can see what each returns is
the researcher degree of freedom pre-registration exists to remove.

Had per-row been registered, it would be the right rule. It was not.

## §3 Band derivation and its domain

| quantity | value | source |
|---|---|---|
| quantisation alone (`q`) | `2.62e-03` | `quantized-models.md:42` |
| backend alone (`b`) | `2.63e-03` | `quantized-models.md:42` |
| quadrature `sqrt(q²+b²)` | `3.71e-03` | derived |
| linear `q+b` | `5.25e-03` | derived |
| independent band | `<= 4.2e-03` | registered |
| correlated band | `>= 4.8e-03` | registered |

**Domain.** Both sources were measured on the strings `quantized_agreement.rs` uses —
7, 10 and 6 tokens. Every string is below ggml-vulkan's op-offload threshold of 32. The
bands are therefore derived from, and valid for, the **`<32`-token regime only**.

## §4 The 36-token row, as read

`composed_error.rs` carries one `>=32`-token input (`LONG_TEXTS`, `:48-52`), measured at
**36 tokens** by `llama-tokenize` against `bge-small-en-v1.5-f32.gguf` — the comment at
`:42` is exact, and so are the `7 / 4 / 6` counts at `:34` and the `32` at `:265`.

For that row #33 recorded backend-only `2.389e-03` and composed `1.859e-03`.

**Under the registered read of §2, `1.859e-03 <= 4.2e-03` returns _independent_ — and
that threshold is being applied outside the domain it was derived in**, because `4.2e-03`
was computed from short-text sources (`2.62e-03` / `2.63e-03`) while this row's sources
differ and its quantisation-only figure is not yet measured. Both halves of that sentence
are the verdict. Neither is a hedge on the other.

**This supersedes #33.** The commit body adjudicated the same row against the per-row
`quadrat.` and `linear` columns ("still below both the quadrature and linear
predictions"). That is a different rule from the one registered in §2. Anyone who reads
the commit message first should treat it as the same measurement decided under a rule
this document replaces — **not** as an independent confirmation.

## §5 The sub-source reading — UNVERIFIED

`composed` (`1.859e-03`) is below `backend_only` (`2.389e-03`). It is tempting to read
that as the two perturbations partially cancelling, since under any additive model
quadrature is `>= max(q,b)` and linear is larger still.

**That inference does not hold on these figures.** Both are `max_diff`, an order
statistic over 384 components. The composed perturbation's largest single coordinate can
be smaller than either source's largest coordinate simply because **a different
coordinate won** — no cancellation required. The test file states this at `:124-128`:

> `max_diff` is an ORDER STATISTIC — it reports one component and says nothing about the
> other 383 — so a composed max that falls below both sources could be a coincidence of
> which component won. L2 aggregates every component and cannot be gamed that way, so the
> two together decide whether an apparent cancellation is real.

The L2 columns for that row were printed (`:217-222`) and never captured. So the reading
is **UNVERIFIED**, and the resolution rule is registered here in advance:

- Cancellation is **established** only if the L2 triple agrees with the max triple —
  `composed_l2 < max(quant_l2, backend_l2)`.
- If `composed_l2 >= max(quant_l2, backend_l2)` while the max triple shows the reverse,
  the reading is an **order-statistic artifact** and is recorded as **refuted**, not
  quietly dropped.

## §6 A mechanism, as hypothesis

Conditional on §5 surviving, and stated as a hypothesis rather than a finding:

If Vulkan's quantised matmul kernel dequantises to f32 and accumulates in a way closer to
CPU-f32 than CPU-Q8_0's own accumulation is, then CPU-f32 → Vk-Q8_0 would genuinely be a
shorter hop than CPU-Q8_0 → Vk-Q8_0, and the composed error would sit below the backend
leg for a real reason rather than a coincidental one.

Testable against the quantisation-only figure for the same row, which §8's run produces.
If §5 is refuted this hypothesis is moot and should be struck rather than kept as a
standing question.

## §7 Second registration, for the `>=32`-token regime

§3's bands do not cover this regime. A replacement is registered here, **before** the run
that produces the numbers it will judge.

**Register the function, not constants.** `quant_only`, `backend_only` and `composed` all
derive from the same three embedding sets `{cpu_f32, cpu_q, gpu_q}`
(`composed_error.rs:199-203`), so there is no run that measures this regime's sources
without simultaneously making `composed` computable. Constants derived from the regime
therefore cannot be registered ahead of the answer they adjudicate. A rule can be.

Slack ratios are **inherited** from §3 rather than invented — `4.2/3.71 = 1.132` and
`4.8/5.25 = 0.914` — so the new registration carries the original's tolerance.

**Evaluated as an ordered chain. The order is part of the registration.**

```
quad   = sqrt(q^2 + b^2)
linear = q + b

1. if min(q,b) == 0 or max(q,b)/min(q,b) > 3   -> UNDECIDED (out of validity domain)
2. else if composed <  max(q,b)                -> CANCELLING  (requires §5 agreement)
3. else if composed <= 1.132 * quad            -> INDEPENDENT
4. else if composed >= 0.914 * linear          -> CORRELATED
5. else                                        -> UNDECIDED
```

**Why the chain must be ordered.** `quad >= max(q,b)` holds for all non-negative `q,b`,
so `1.132 * quad > max(q,b)` always. Every value satisfying arm 2 therefore also
satisfies arm 3. Unordered, the rule returns two verdicts for the same input.

**Why the validity gate is required.** Arms 3 and 4 invert when the sources are unequal.
Solving `1.132*sqrt(q²+b²) = 0.914*(q+b)` gives roots at `q/b = 0.2893` and `q/b =
3.4567`, so beyond roughly a `3.46:1` source ratio the independent threshold rises
*above* the correlated one and a value can satisfy both arms at once. Worked, with `b=1`:

| `q/b` | `1.132 × quad` | `0.914 × linear` | |
|---|---|---|---|
| 1.0 | 1.601 | 1.828 | ordered |
| 0.5 | 1.266 | 1.371 | ordered |
| 0.2 | **1.154** | **1.097** | **inverted** |

The rule is well-behaved where it was calibrated — §3's sources are `2.62` / `2.63`, a
ratio of ~1 — and degenerates where they diverge. The `>= 32`-token row is exactly where
they might: `backend_only` is `2.389e-03` and `quant_only` for that row is unmeasured. If
it lands small, the ratio approaches the inversion boundary. The `<= 3` gate sits inside
`3.4567` with margin.

Registering the failure mode is stronger than registering a rule that quietly returns two
answers. When arm 1 fires, the row is **undecided by construction** and needs its own
registration. Arm 2's test is ratio-independent and may still be *reported* as an
observation in that case; it is not a verdict.

## §8 Reproduction

Fixtures: `bge-small-en-v1.5-{f32,Q8_0,Q5_K_M,Q4_K_M}.gguf` in `$NC_TEST_MODEL_DIR`
(default `$HOME/models`), produced per `quantized-models.md`.

```sh
env LLAMA_CPP_DIR=$HOME/src/llama.cpp \
    LLAMA_CPP_LIB_DIR=$HOME/src/llama.cpp/build-vulkan/bin \
    GGML_OP_OFFLOAD_MIN_BATCH=999999 \
    NC_REQUIRE_BACKEND=1 NC_REQUIRE_VULKAN=1 NC_REQUIRE_QUANT=1 \
    cargo test -p neuralcompose-llama --test composed_error -- --nocapture --test-threads=1
```

**`GGML_OP_OFFLOAD_MIN_BATCH` must be in the environment at process exec.** It is read
once by ggml-vulkan at device registration and latched behind a `static bool initialized`
(`ggml/src/ggml-vulkan/ggml-vulkan.cpp:18221-18253`), and this repository triggers that
registration from a `std::sync::Once` in `init_backend_once` (`src/lib.rs:244-259`).
Setting it from inside Rust after any `Embedder::load*` or `devices()` call is a **silent
no-op** — offload stays live while `offload_suppressed()` (`:54-59`) reports `true` and
the long input is admitted to the measurement.

Nothing in this crate can suppress offload. `gpu_layers = 0` controls **weight residency
only**: it reaches `mparams.n_gpu_layers` (`csrc/nc_llama_shim.c:113-116`) and nothing
sets `cparams.op_offload`, which defaults to `true`. `backend_id()` returning
`llama-cpp-cpu` is a label derived from `requested_gpu_layers == 0` (`lib.rs:458-463`),
not a statement about where the matmuls ran.

What catches the failure downstream is `assert_cpu_arm_is_really_the_cpu` (`:89-99`),
which runs over the same list the claim is computed from and fails on bit-identical
arms. The suppression check alone would lie; the guard is what makes the arm trustworthy.

### §8.1 The suppression was verified, not assumed

Run before the measurement it gates, on 2026-08-04. Both arms of
`a_zero_gpu_layer_run_silently_uses_the_gpu` under `NC_LLAMA_VERBOSE=1
GGML_SCHED_DEBUG=2` — the verbose flag is required, because `GGML_SCHED_DEBUG` output
routes through `GGML_LOG_DEBUG` and `nc_llama_log_quiet()` (`lib.rs:250`) silences it by
default, so a control run would print nothing and read as a clean zero.

The test emits 22 graphs: ten context reserves per `Embedder`, then one compute each.
**Every graph is backend-pure** — no graph mixed CPU and Vulkan matmuls, so the
partial-offload case in §10 did not arise here. The two computes are identifiable by
tensor size: `Qcur-0` at `48K` is 32 tokens × 384 × 4 bytes, against `768K` for the
worst-case 512-token reserves.

| graph | what it is | control | `MIN_BATCH=999999` |
|---|---|---|---|
| 21 | `ngl=0` embed, 32 tokens | **all-Vulkan** (72/72) | **all-CPU** (72/72) |
| 22 | `ngl=99` embed, 32 tokens | all-Vulkan (72/72) | all-Vulkan (72/72) |

Aggregate over all 22 graphs: `MUL_MAT` nodes total 1584 in both arms, with exactly
**216 moving from Vulkan to CPU** (1008→792 and 576→792). The variable altered **only
the baseline arm**; the GPU arm is untouched at identical tensor size. That is the same
manipulation-with-control that `vulkan-performance-claim.md:30-33` established from
timings, shown here at the scheduler level.

Assertions agree with the log: the control took the `assert_eq!` branch and printed
`CONFIRMED` (`:291-295`), the suppressed arm took `assert_ne!` (`:278-282`). Both passed.

**One instrument note.** The scheduler prints the backend field truncated to five
characters, so node lines read `[  Vulka  ]`, not `Vulkan0`. A filter matching `Vulkan0`
against node lines returns empty in *both* arms and reads as a clean suppression. Only
the `## SPLIT #n:` headers carry the full device name. Match `Vulka`, or read splits.

## §9 What CI does not cover

CI never executes this test. `.github/workflows/ci.yml` runs `cargo test` on
`ubuntu-latest` with `LLAMA_CPP_DIR` unset, so the crate compiles to a stub and `ready()`
short-circuits at `:150-152`. `NC_REQUIRE_QUANT` — the switch that turns a skip into a
failure — is never set in CI, so the test reports `ok` having verified nothing.

Every number this document governs comes from a manual run on `ok-cyberdeck`.

## §10 Known defects in the test file

Recorded here so they are not discovered as surprises. **Fixed after the §8 run, not
before** — nothing changes between registration and measurement.

- **`GGML_OP_OFFLOAD_MIN_BATCH` is read two ways.** `offload_suppressed()` (`:54-59`)
  requires `n > 512`; the second test (`:275`) branches on mere presence. At `999999`
  they agree, so this does not affect the registered run. It bites on malformed input:
  `abc` or `0` makes upstream `atoi` return `0` — offload always on — while `:275` takes
  the suppressed branch and asserts `assert_ne!` with a message stating the opposite of
  what happened. A value like `100` really suppresses while `offload_suppressed()`
  reports `false`, silently narrowing coverage (announced at `:70-75`, at least).
- **The CPU-arm guard is blind to partial offload.** `assert_cpu_arm_is_really_the_cpu`
  fires only on bit-identity, which detects *total* offload. A graph where only some ops
  cross the batch threshold would diverge slightly, pass the guard, and still be partly
  GPU. Closing it needs a shim entry point exposing the device's effective
  `op_offload_min_batch_size`, following the `active_threads()` / `active_ubatch()`
  read-back precedent (`lib.rs:469-487`) — a swept parameter that silently failed to
  apply produces a flat result indistinguishable from a refuted hypothesis.
- **No token count is asserted.** The `36`, `32` and `7 / 4 / 6` figures are comments.
  All five were verified by `llama-tokenize` on 2026-08-04 and were exact, but nothing in
  the suite would notice if a string were edited below the threshold, which would move a
  row into the wrong regime while every guard still passed.

## No row is promoted

- This document registers a reading rule and records one superseded adjudication. It
  produces no measurement and advances no rung.
- Both `linux/x86_64` GGUF rows remain `RuntimeSmokeValidated`.
- §5 is open. The composed-below-backend reading is **unadjudicated**, not confirmed.
- §7 governs no numbers yet. It is registered ahead of the run, which is the point.
