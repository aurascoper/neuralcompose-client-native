# Replacing Personal Voice: TTS evaluation

Goal: derobotified prosody on Linux, to replace `NeuralCompose`'s Personal Voice
+ `AVSpeechSynthesizer` path. espeak-ng works and is intelligible to whisper but
sounds like 1998.

Measured on `ok-cyberdeck` 2026-08-04. **Nothing here promotes a support-matrix
row.**

## Two goals that are often conflated

- **(a) Natural prosody** — speech that does not sound robotic.
- **(b) Cloning a specific person's voice** — what Personal Voice actually does.

Apple's Personal Voice is **not** zero-shot: it is on-device *fine-tuning* from
~150 recorded sentences, trained overnight while charging and locked. It adapts a
modified FastSpeech2 acoustic model plus a WaveRNN vocoder. Reported MOS 3.68 vs
3.85 for real recordings.
(https://machinelearning.apple.com/research/personal-voice)

Most candidates solve (a) *or* (b). Qwen3-TTS is notable for doing both.

## The candidate: Qwen3-TTS via llama.cpp

llama.cpp merged Qwen3-TTS on **2026-08-04** (PR #26254), rewriting `llama-tts`
from an OuteTTS demo into a model-agnostic tool over `libmtmd`. This is the only
option that keeps the **single-runtime rule** — no onnxruntime, no PyTorch, same
ggml runtime already in use.

- Apache-2.0 for **code and weights** (unusual and important)
- 1.7B params, 10 languages
- `-Base` does **3-second zero-shot voice cloning** from reference audio
- GGUF at `ggml-org/Qwen3-TTS-12Hz-1.7B-Base-GGUF`: Q4_K_M 1.04 GB, Q8_0 1.85 GB

### Built without disturbing the pinned checkout

`~/src/llama.cpp` stays at `d0bfb1981` — the commit both Linux support-matrix
rows cite. Master was built in a **separate git worktree** so the pinned tree
never moved:

```sh
git -C ~/src/llama.cpp worktree add ~/src/llama.cpp-tts origin/master   # 6ea215d17
cmake -B build-cpu -DGGML_VULKAN=OFF -DCMAKE_BUILD_TYPE=Release -DLLAMA_BUILD_SERVER=OFF
cmake --build build-cpu --target llama-tts -j 6      # NOTE: -j 6, see below
```

`--tts-lang` present in the new binary and absent in the old one is the quick
check that you have Qwen3-TTS support rather than the OuteTTS-only build.

### Speed: fast enough (this was the open question)

No published CPU RTF existed for this model on this class of hardware. Measured:

```
frames generated: 33, speed: 16.11 frames/s
frames generated: 65, speed: 16.05 frames/s
```

The codec runs at 12 Hz, so **16.05 frames/s ≈ 1.34× realtime (RTF ≈ 0.75)** on
CPU alone, Q4_K_M, no GPU. Speed was the thing most likely to disqualify this
model, and it does not.

### Status: BLOCKED — it crashes

```
ggml/src/ggml-cpu/ops.cpp:4886: GGML_ASSERT(i01 >= 0 && i01 < ne01) failed
  ggml_compute_forward_get_rows
  → clip_encode → mtmd_gen_audio_process → qwen3tts_gen_audio_pipeline::step_gen
```

An out-of-range row index in the audio decode path. Reproducible. (Called "a
generated code token outside the codebook" here originally — see the RESOLVED
section below; it is not a code token at all.)

Two things learned narrowing it:

- **`--tts-speaker-file` is required for the `-Base` variant.** Without it the
  run aborts almost immediately. With a reference (used `whisper.cpp`'s
  `samples/jfk.wav`) it generates ~65 frames first. The README example includes
  the flag; omitting it is not a graceful degradation.
- **The mmproj is Q8_0 even when the LM is Q4_K_M.** So the crash occurs in an
  already-8-bit decoder; decoder quantization is not the variable.

**Quantization hypothesis REFUTED.** Running the LM at Q8_0 crashes identically —
same assert, frame 58 vs Q4_K_M's frame 65. Both quants also stay faster than
realtime (Q8_0: 14.25 frames/s ≈ 1.19×; Q4_K_M: 16.05 ≈ 1.34×), so the choice
between them is quality-vs-speed, not stability.

**Better hypothesis, from a warning I initially skimmed past:**

```
W load: special_eos_id is not in special_eog_ids - the tokenizer config may be incorrect
```

If the EOS token is not registered as end-of-generation, generation runs *past*
the end of the utterance instead of stopping, and then emits a code outside the
codebook — which is exactly an out-of-range `get_rows`. Both runs dying at ~5 s
of audio (58–65 frames at 12 Hz) for a sentence of roughly that length fits this
much better than random token corruption, which would scatter far more.

### The assert is identical in every case

Verbatim, in all seven runs below, at every prompt length and both quantizations:

```
/home/aurascoper/src/llama.cpp-tts/ggml/src/ggml-cpu/ops.cpp:4886: GGML_ASSERT(i01 >= 0 && i01 < ne01) failed
#5  ggml_abort () from build-cpu/bin/libggml-base.so.0
#6  ggml_compute_forward_get_rows () from build-cpu/bin/libggml-cpu.so.0
#13 clip_encode(clip_ctx*, clip_encode_params*) () from build-cpu/bin/libmtmd.so.0
#14 mtmd_gen_audio_process_impl(mtmd_context*, mtmd_gen_inp const*, mtmd_gen_out*)
#16 qwen3tts_gen_audio_pipeline::step_gen(int, float const*, float const**)
```

One symptom, reached from what look like two different triggers.

### Crash point vs prompt length

| prompt chars | frames reached | quant |
| --- | --- | --- |
| 3 (`Hi.`) | **0** — dies at the first GEN_WAV call, before the 32-frame print | Q4_K_M |
| 12 | < 32 | Q4_K_M |
| 28 | < 32 | Q4_K_M |
| 44 | 65 | Q4_K_M |
| 44 | 58 | Q8_0 |
| 62 | 33 | Q4_K_M |
| 120 | 66 | Q4_K_M |
| **398** (≈25 s of speech, ≈300 frames of content) | **64** | Q4_K_M |

**The ceiling is real and content-independent.** A 398-character prompt whose
content needs roughly 300 frames still died at 64. Nothing in any configuration
exceeded 66 frames. That is a hard limit, not a content effect.

**Short prompts die well below the ceiling**, and the ceiling cannot explain
them. Length correlates with crash frame only noisily below the ceiling — 62
chars reached 33 while 44 chars reached 65 — so this is not a clean function of
length either.

### RESOLVED: `inp_code0` is never uploaded on a GEN_WAV call

The earlier reading below was **wrong**, and one measurement refuted it. The
assert is a bounds check on dimension 1, so print `ne01` and `i01` at the failure:

```c
// ggml/src/ggml-cpu/ops.cpp, immediately before the assert at :4886
if (i01 < 0 || i01 >= ne01) {
    fprintf(stderr, "GETROWS-DIAG src0='%s'(%s) ne01=%lld i01=%lld"
            "  src1='%s'(%s) n_idx=%lld  as_f32=%f\n",
            src0->name, ggml_type_name(src0->type), (long long) ne01, (long long) i01,
            src1->name, ggml_type_name(src1->type),
            (long long) (ne10*ne11*ne12), (double) *(float *)(char *) src1->data);
}
```

`cmake --build build-cpu --target ggml-cpu -j 6` — one translation unit.

```
Hi.        GETROWS-DIAG src0='a.gen.code.out_embd.weight'(q8_0) ne01=3072 i01=1098738097  src1='inp_code0'(i32) n_idx=1  as_f32=15.838304
Hi.        GETROWS-DIAG src0='a.gen.code.out_embd.weight'(q8_0) ne01=3072 i01=1100981860  src1='inp_code0'(i32) n_idx=1  as_f32=19.956245
124 chars  GETROWS-DIAG src0='a.gen.code.out_embd.weight'(q8_0) ne01=3072 i01=1099895540  src1='inp_code0'(i32) n_idx=1  as_f32=17.884254
```

**`ne01` is 3072, not 64.** It is the code0 codebook height, and has nothing to do
with the ~64-frame ceiling. The overrun-past-a-fixed-dimension explanation is dead.

**`i01` is not an out-of-range code token.** It is ~1.1 × 10⁹, and reinterpreting
those bits as float32 gives 15.84, 19.96, 17.88 — plausible logit magnitudes,
different every run. `inp_code0` is holding **float data that was never written to
it as an index**.

The wiring is otherwise correct, which is why this was hard to see from outside:
`inp_code0` is declared `GGML_TYPE_I32` and flagged `ggml_set_input()`
(`tools/mtmd/models/qwen3tts-gen.cpp:669-671`), `set_input_i32` asserts the type
(`tools/mtmd/clip.cpp:4122`), and `params->code0` is bounds-checked against the
codebook before upload (`clip.cpp:4779-4784`).

The defect is that **none of that runs on the failing path.**
`clip_graph_qwen3tts_gen::build()` always builds both sub-graphs — the comment at
`qwen3tts-gen.cpp:650` says so outright: *"both sub-graphs are always built, so
the topology stays constant; ggml_build_forward_select() then picks the one that
actually runs."* But `set_inputs` branches
(`clip.cpp:4733`): on `CLIP_GEN_PROCESS_GEN_WAV` it uploads `inp_codes` and the
c2w state and returns — **`inp_code0` is never uploaded, and its bounds check is
in the `else` branch that never executes.** The GEN_CODE `ggml_get_rows` node is
still in the graph with an unwritten input buffer, and gets computed against
whatever floats the allocator left there.

That is why every backtrace passes through `mtmd_gen_audio_process` — the wav
path — and never the code path.

**Prompt length changes only when the first GEN_WAV call happens, not what
happens.** `Hi.` never reaches the 32-frame progress print and dies at the first
GEN_WAV call; the 124-char prompt prints `frames generated: 32` then `64`, then
dies at the identical diagnostic. Same tensor, same `ne01`, same garbage class.
One mechanism, two arrival times.

This also disposes of the `special_eos_id is not in special_eog_ids` warning as
the cause. It may still be a real tokenizer-config problem, but it cannot produce
a float bit pattern in an i32 index tensor, and the crash happens on the first
GEN_WAV call regardless of whether generation had anything left to say.

**Fix belongs upstream, one of:** upload `inp_code0` unconditionally in
`set_inputs`, or keep `ggml_build_forward_select` from admitting the unselected
sub-graph's input nodes into the computed graph. The second is the general fix;
the first is the one-liner that unblocks it.

**No fix upstream yet.** `origin/master` is still `6ea215d17` — zero new commits
since the build, none touching `tools/tts` or `tools/mtmd`. Reportable as-is: the
diagnostic patch above, the three log lines, and the `set_inputs` branch.

### Superseded reading (kept for the record)

> *Generation never terminates cleanly, and dies at whichever comes first: the end
> of its own content, or a ~64-frame ceiling* — consistent with the EOG warning,
> and wrong. It was assembled entirely from black-box observations (crash frame vs
> prompt length, across seven runs and two quantizations). Every one of those
> observations was accurate; the inference from them was not. The `ne01`/`i01`
> print cost one recompile of one file and refuted it outright.
>
> What survives from that work: **not** quantization (identical at Q8_0 and
> Q4_K_M), and the ~64-frame ceiling is real but is a *separate* phenomenon — it
> bounds how far generation gets before the wav call, nothing more.

## Fallback if Qwen3-TTS stays broken: Kokoro-82M

**Apache-2.0, 82M params / ~327 MB, 54 fixed voices**, well attested
faster-than-realtime on CPU, widely described as the naturalness/speed sweet
spot. Solves goal (a) only: **no voice cloning at all.**

### The single-runtime rule does not reach this demo

An earlier draft of this document treated onnxruntime as disqualifying. That was
a misapplication. The single-runtime rule governs the **shipped client-native
binary** — memory and load time on a handheld doing BLE ingest, classification
and generation simultaneously. This demo lives in `tools/`, claims no
support-matrix row, and links nothing into the product — the venv beside it is
not in any target graph, exactly as `tools/muse-ble-bridge/.venv` is not.

**Being committed here does not change that**, and the risk that it looks like it
does is the reason this section exists.

So Kokoro is fine *here*, and the honest form is to say so before it exists:

> **The demo links onnxruntime. The runtime decision for the product is deferred
> and unmade.** Nothing about this demo's dependency graph is a precedent for
> client-native's, in the same way `docs/hardware/` establishes facts and claims
> no rung.

What must not happen is a working demo becoming the argument that the decision
was already taken. That is promotion by implication in a new costume.

### For a dialectic, two voices beat one cloned voice

"Kokoro cannot clone" reads as a bigger loss than it is for this use. A dialectic
holds thesis and antithesis in tension across turns, and the functional
requirement there is **distinguishable poles** — voice *identity* is not the
point. Kokoro ships 54 voices; giving the two positions different ones does
something for a dialectic that cloning a single voice does not.

The cloning gap is expensive for anything Personal-Voice-shaped and cheap for
this.

## Rejected, with reasons

| Option | Why not |
| --- | --- |
| **Piper** | Audibly synthetic next to Kokoro. Also `rhasspy/piper` went read-only Oct 2025 and relicensed MIT → **GPL-3.0**; active fork is `OHF-Voice/piper1-gpl` |
| **F5-TTS** | Code MIT but **weights CC-BY-NC** — commercial use prohibited |
| **XTTS-v2 / Coqui** | CPML non-commercial, and Coqui dissolved Jan 2024, so no one can sell a licence |
| **Chatterbox** | Genuinely good and MIT, but PyTorch and pulls CUDA-flavoured torch even for CPU — heaviest violation of the runtime rule |
| **Sesame CSM / LFM2** | Named in llama.cpp issue #21956 as *candidates* only. Nothing implemented |
| **OuteTTS** | Superseded by Qwen3-TTS in the same tool |

## Consent and disclosure

**Not legal advice, and deliberately not specific.** EU AI Act Article 50
transparency obligations for synthetic audio began applying on 2026-08-02, and
several US states have extended right-of-publicity to voice replicas. The details
are still bedding in and should be checked with someone qualified before anything
ships.

The part that matters for a design decision now is directional and robust to the
details:

**Not cloning a real person's voice is a materially smaller compliance surface
than cloning one.** That is a near-term argument for Kokoro independent of the
crash, and it compounds with the dialectic point above — the use case does not
need cloning, and not needing it removes an obligation rather than deferring one.

Worth noting how the risk profile changed: Personal Voice required 150 enrolment
sentences, and that friction made consent essentially implicit. Zero-shot cloning
from three seconds of audio removes it. Accidentally cloning someone becomes
possible in a way it structurally was not before — cheap to design around now,
expensive to retrofit.

## Operational note: bound your build jobs

An unbounded `cmake --build -j` (24 threads) run while llama-server held ~8 GB
with the 14B resident triggered a **global OOM** at 17:28:09. systemd killed the
whole `ptyxis-spawn` scope, taking llama-server with it. Both tasks reported exit
144 with nothing in their own logs — the evidence was only in `journalctl`, since
`dmesg` needs root and the killed process never gets to write.

Use `-j 6` on this box, and do not build while a large model is resident.
