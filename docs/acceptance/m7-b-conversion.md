# M7-B — candidate pinning and Qwen3 conversion record

Branch: `m7-b/local-qwen-generation-benchmark`
Base: `dabf68853dc265f5f1de42cf350fe44c2f93cc91` (tag `muse-golden-capture`)
Date: 2026-07-29

**No model bytes are in this repository.** Only revisions, commands, and
digests. The artifacts live in a scratch directory outside the repo.

## Why a conversion exists at all

`Qwen/Qwen2.5-0.5B-Instruct-GGUF` publishes an official **Q4_K_M**.
`Qwen/Qwen3-0.6B-GGUF` publishes **only Q8_0** — there is no official
Q4_K_M, and `Qwen/Qwen3-0.6B` carries no GGUF at all. Rather than pull an
unpinned community artifact, Candidate B's Q4_K_M is derived from pinned
official source weights, and the asymmetry is recorded in the candidate
identity as `DerivedByConversion` rather than hidden.

## The 2×2 matrix

| Model | Q4_K_M | Q8_0 |
| --- | --- | --- |
| Qwen2.5-0.5B-Instruct | official — **primary A** | official — control |
| Qwen3-0.6B non-thinking | **derived** — **primary B** | official — control |

## Pinned inputs

| Repository | Revision |
| --- | --- |
| `Qwen/Qwen2.5-0.5B-Instruct-GGUF` | `9217f5db79a29953eb74d5343926648285ec7e67` |
| `Qwen/Qwen3-0.6B` (conversion source) | `c1899de289a04d12100db370d81485cdf75e47ca` |
| `Qwen/Qwen3-0.6B-GGUF` (control + oracle) | `23749fefcc72300e3a2ad315e1317431b06b590a` |
| `ggml-org/llama.cpp` (conversion **and** quantization) | `f5b9bd39b56c7a7839a9795a100b6a00b84ac961` |

One llama.cpp commit does both steps.

## Artifact digests

| Artifact | SHA-256 | Bytes |
| --- | --- | --- |
| `qwen2.5-0.5b-instruct-q4_k_m.gguf` (official) | `74a4da8c9fdbcd15bd1f6d01d621410d31c6fc00986f5eb687824e7b93d7a9db` | 491 400 032 |
| `qwen2.5-0.5b-instruct-q8_0.gguf` (official) | `ca59ca7f13d0e15a8cfa77bd17e65d24f6844b554a7b6c12e07a5f89ff76844e` | 675 710 816 |
| `Qwen3-0.6B-Q8_0.gguf` (official) | `9465e63a22add5354d9bb4b99e90117043c7124007664907259bd16d043bb031` | 639 446 688 |
| `model.safetensors` (Qwen3 source) | `f47f71177f32bcd101b7573ec9171e6a57f4f4d31148d38e382306f42996874b` | 1 503 300 328 |
| `qwen3-0.6b-intermediate.gguf` (**intermediate**) | `7ae0f0cb94ca4531c3d1a142dc73c2219d07dd8a9793272b801af786b4041e33` | 1 509 347 552 |
| `qwen3-0.6b-q4_k_m.gguf` (**derived output**) | `28ad083b63a939387aff53bb3d413056ed07fd96deb05bed3416b13a556105c2` | 484 220 128 |

## Exact commands

```sh
python convert_hf_to_gguf.py models/qwen3-src \
  --outfile models/qwen3-0.6b-intermediate.gguf --outtype auto

llama-quantize models/qwen3-0.6b-intermediate.gguf \
  models/qwen3-0.6b-q4_k_m.gguf Q4_K_M
```

Quantizer report: 456.11 MiB at 5.09 BPW.

```yaml
importance_matrix: none
calibration_dataset: none
allow_requantize: false
pure_quantization: false
source_precision: bf16
conversion_commit: f5b9bd39b56c7a7839a9795a100b6a00b84ac961
quantizer_commit: f5b9bd39b56c7a7839a9795a100b6a00b84ac961
```

No importance matrix and no calibration corpus were used, and none was
invented to fill the field.

## Conversion-oracle result

The derived Q4 was compared against the official Q8_0 on non-numerical
semantics. **19 of 20 critical fields agree exactly**, including the entire
tokenizer:

| Field | Result |
| --- | --- |
| `general.architecture` | `qwen3` — agree |
| `tokenizer.ggml.tokens` | 151 936 entries, digest `5b669fd655ce42f0` — agree |
| `tokenizer.ggml.merges` | 151 387 entries, digest `a358fea26249a689` — agree |
| `tokenizer.ggml.token_type` | digest `bfe240dc73651f08` — agree |
| BOS / EOS / padding ids | 151643 / 151645 / 151643 — agree |
| `tokenizer.ggml.model`, `.pre`, `.add_bos_token` | `gpt2`, `qwen2`, false — agree |
| context / block count / embedding / heads / rope | 40960 / 28 / 1024 / 16+8 / 1e6 — agree |
| **`tokenizer.chat_template`** | **differs** |

### The template difference is real but tested-immaterial

The official GGUF embedded an **older snapshot** of the same Qwen template.
The source repo's current template restructures the reverse-scan loop and
adds defensive `is string` guards; it is not a different chat format.

Rendering both templates over benchmark-shaped message sets produced
**byte-identical output in every case**:

| Case | `enable_thinking=False` | `enable_thinking=True` |
| --- | --- | --- |
| single user turn | identical (`f27ef49d33466898`) | identical (`f07362de63658367`) |
| system + user | identical (`f069b43d0c94d9a7`) | identical (`625888159c33520c`) |
| multi-turn | identical (`4c6058b3427354d6`) | identical (`7c1b3254b340b1f9`) |

The divergence is therefore recorded as a known, tested-immaterial
difference rather than treated as a silent equivalence. It must be re-tested
if the prompt corpus grows message shapes not covered above.

## Non-thinking mode: the naive gate is not sufficient

Qwen3's non-thinking template does **not** omit the think block — it
pre-fills an **empty, already-closed** one:

```text
…<|im_start|>assistant\n<think>\n\n</think>\n\n
```

Feeding that exact prompt to the derived Q4 by hand still produced reasoning
prose:

```text
[Start thinking]

Okay, the user is asking for three primary colors… Let me think…
```

No literal `<think>` tag appeared in the generation, so a gate that only
searches for `<think>`/`</think>` **would have passed a run in which
reasoning plainly occurred.** Comparing a reasoning Qwen3 against a
non-reasoning Qwen2.5 would have been a confounded benchmark.

Driving the runtime's own template instead suppresses it correctly:

```sh
llama-cli -m qwen3-0.6b-q4_k_m.gguf --jinja --reasoning off …
```

Output: `Primary colors are red, blue, and yellow.` — no think tags, no
`[Start thinking]`, no chain-of-thought prose.

**Consequences for the benchmark harness:**

1. Non-thinking must be driven by the runtime's chat template with reasoning
   off, never by a hand-assembled prompt string.
2. The admission gate must scan the generated continuation for reasoning
   *prose markers*, not only for `<think>` tags.
3. The pre-filled empty think block belongs to the **prompt**, so the scan
   must run over the continuation only.

## Host smoke (macOS, CPU-only build, not device evidence)

| Candidate | Prompt t/s | Generation t/s | Output |
| --- | --- | --- | --- |
| Qwen2.5 official Q4_K_M | 343.5 | 125.5 | `Red, blue, and yellow are the primary colors.` |
| Qwen3 derived Q4_K_M | 192.0 | 75.1 | `Primary colors are red, blue, and yellow.` |

Both answer correctly and terminate. **These are host numbers on a CPU-only
build and say nothing about Pixel 8a performance** — no device claim is made
here.

## Non-claims

- No benchmark has been run; no candidate is promoted or preferred.
- Host throughput is not device throughput.
- Quality was not scored — two smoke answers are not a quality panel.
- The Q8_0 controls have not been run at all yet.
- The Android runtime does not exist yet; nothing has executed on a phone.

---

## Restore record — this file and the v6-1 implementation were off `main` for a day

**This document, and the v6-1 implementation it accompanies, were absent from
`main` between 2026-08-03 and 2026-08-04.** Recorded here so the recovery path
stops living only inside a revert commit message.

### The sequence

| commit | date | what it did |
|---|---|---|
| `a3a8382` | 2026-07-31 | `feat(m7-b): v6-1 declaration, Rust-owned corpus, and exact arithmetic` — where v6-1 landed |
| `54f2a31` | 2026-07-31 | `fix(m7-b): seal v6 corpus and evaluator authority` — five further tests; **the complete v6-1 state** |
| `759c1f3` | 2026-08-03 | merge of PR #19 `feat/headless-linux-runtime`, which had been branched from `m7-b/local-qwen-generation-benchmark` instead of `main` and so carried PR #5's v6 work with it |
| `81b4f0b` | 2026-08-03 | revert of `759c1f3` — correct in intent, but it removed the v6 work as collateral |
| *this commit* | 2026-08-04 | restores the v6-1 tree from `54f2a31` onto current `main` |

PR #19's own content was re-landed cleanly by PR #21 (`crates/neuralcompose-headless`,
`tools/fixture-eeg-server`), so only PR #5 content needed restoring.

**The restore source is `54f2a31`, not `a3a8382`.** `54f2a31` contains `a3a8382`
plus 4,337 bytes of `generation_eval.rs` and 7,595 bytes of `m7b_eval_protocol.rs`.
Restoring from `a3a8382` would have silently dropped five tests. Their presence is
the check that the right commit was used — see
`m7-b-v6-1-restore.stdout.txt`.

### A guard gap this restore surfaced — recorded, not fixed

`contracts/README.md` says *"Editing a prompt, reordering the list, or changing
either taxonomy axis changes the corpus identity and therefore the declaration
identity — that is the intent, not a hazard."* The first half is true. The second
half does not currently hold as a **guard**, because nothing asserts a pinned
identity. Mutation-tested on 2026-08-04:

| mutation to `m7b-corpus-v1.json` | `check-fixtures.sh` | `cargo test` |
|---|---|---|
| capitalisation changed inside a prompt message | **passes** | **passes** |
| JSON indentation collapsed | **fails — caught** | — |

`include_bytes!` is why. The compiled value tracks the file, so a content edit
moves both sides of the diff together and the round-trip still matches. The check
proves **serialization fidelity** — that the committed artifact and the compiled
value are the same bytes — which is exactly what its comment claims. It does not,
and cannot by construction, detect that someone changed what a prompt *says*.

This is the same shape as a correct assertion that never meets its failing input.
**It is pre-existing in `54f2a31`, not introduced by this restore**, and it is
left unfixed here deliberately: closing it means pinning an expected corpus digest
in a test, which is a contract decision belonging to #7's authority-boundary
discussion rather than to a restore.

### What this restore does NOT establish

- **`ios/NeuralComposeCore/Tests/SmokeTests.swift` is not restored.** Its 334 v6
  lines are out of scope and **have never been compiled**. `ci.yml` states that
  `ios/NeuralComposeCore/Tests` uses XCTest and is not compiled in CI, and that
  the Linux Swift job is parse-only and non-blocking on purpose. That policy is
  unchanged by this work.
- **`android/runtime-llama/`** (4 files) is not restored.
- **Of #7's three named evidence items, two are produced** — a CI run, and the
  extended `check-fixtures.sh` output. The third, *"the quota map reproduced in
  the issue thread as filed"*, is deliberately **not** produced, because it
  requires commenting on #7.
- **#7 remains open as an evidence gate.** It was left open on purpose. Nothing
  here closes it, and this restore is not a claim that it is satisfied.
