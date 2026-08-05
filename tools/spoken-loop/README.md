# Spoken-loop MVP

One spoken turn on Linux, end to end, no Apple framework and no cloud:

```
mic → whisper.cpp (CPU) → llama-server (Vulkan, HTTP) → espeak-ng → speakers
```

Built 2026-08-04 on `ok-cyberdeck` (GPD, Ryzen AI 9 HX 370 / Radeon 890M,
Ubuntu 26.04, kernel 7.0.0-28-generic).

## NOT CLAIMED, AND THIS MATTERS MORE THAN WHAT IS

This demo **promotes no support-matrix row**. `attained_support_status()` returns
`None` for everything here and must continue to.

- It is **not** a `RuntimeSmokeValidated` candidate. That rung requires a
  *deterministic fixture model*, and a live microphone is the opposite of
  deterministic.
- It links **no** `nc-gguf-v1` backend under any row's terms. It talks to
  llama-server over **HTTP**, which is a third transport mechanism — the repos
  use WebSocket (`tungstenite`, port 8788) and subprocess (`claude -p`). That
  choice is for this demo and is **not a precedent**.
- It measures **nothing**. No latency or throughput number here is a claim; see
  "If you ever time this" below.
- It lives in `tools/`, which is **not a product surface**. Like the two Muse
  bridges beside it, it is not on the SwiftPM, Gradle or Cargo target graph and
  ships in no app. Being in this repository is a filing decision, not a claim.
- **Ownership is still undecided.** Whether the Swift `BCICore` dialectic or the
  Rust client-native audio contract should own a spoken loop is exactly the
  machine-ownership question this demo was built to defer until the loop works.
  It sits in `tools/` because that is where things live that work and have not
  been assigned an owner — not because client-native won the argument.

What it **does** prove is narrower and real: a spoken turn completes on this
machine, on Linux, locally.

Say this before citing the demo anywhere. A working spoken demo is exactly the
artifact that later gets read as evidence the platform works.

## The design rule: exactly one Vulkan context

PR #30 fixed concurrent Vulkan context creation/teardown **inside one process**,
with a process-local device lock (`neuralcompose-llama/src/lib.rs:280-285`).

This demo is three processes, so that class never arises — but it would create a
class that lock **cannot** touch: a process-local mutex cannot serialise context
creation *across* processes. A Vulkan-built whisper starting while llama-server
already held a context would be concurrent creation on one iGPU with nothing
serialising it, and PR #30's fix does not reach it.

Hence: **llama-server holds the only inference Vulkan context**, long-lived,
created once at startup. whisper is built `-DGGML_VULKAN=OFF`; espeak-ng is
formant synthesis with no runtime, no model and no GPU. This is also the lazier
build — no Vulkan whisper compile.

Verify it held:

```sh
fuser -v /dev/dri/renderD128 2>&1 | grep -iE 'llama|whisper'
```

Only `llama-server` should appear. (The desktop compositor, Firefox and friends
also hold that node — there is one render node on this machine — so grep for the
inference processes rather than expecting a single line.)

## Setup

Versions are pinned because an unpinned artifact is the thing this codebase keeps
having to correct later.

| Component | Version / digest |
| --- | --- |
| llama.cpp | `d0bfb1981266c271cd0536a8aa7c5e863e7cdf61` (the support matrix's commit) |
| whisper.cpp | `306c88f` — release v1.9.2 (#3970) |
| whisper model | `ggml-base.en.bin`, 147 964 211 bytes, sha256 `a03779c86df3323075f5e796cb2ce5029f00ec8869eee3fdfb897afe36c6d002` |
| LLM | `Qwen3-14B-Q4_K_M` from `~/.cache/llama.cpp/` |
| espeak-ng | apt `1.52.0+dfsg-5build1` |

**1. llama-server, Vulkan.** The Vulkan tree was configured `LLAMA_BUILD_SERVER=OFF`,
so the target did not exist and had to be enabled:

```sh
cmake -B build-vulkan -DLLAMA_BUILD_SERVER=ON -S ~/src/llama.cpp
cmake --build ~/src/llama.cpp/build-vulkan --target llama-server -j
```

**2. whisper.cpp, CPU only:**

```sh
git clone https://github.com/ggml-org/whisper.cpp ~/src/whisper.cpp
cmake -B build -DGGML_VULKAN=OFF -DCMAKE_BUILD_TYPE=Release -S ~/src/whisper.cpp
cmake --build ~/src/whisper.cpp/build -j
~/src/whisper.cpp/models/download-ggml-model.sh base.en
```

**3. TTS — Kokoro-82M (default), espeak-ng (fallback).**

```sh
python3 -m venv .venv                      # Ubuntu 26.04 is PEP-668 managed
.venv/bin/pip install kokoro-onnx soundfile
mkdir -p models && cd models
curl -sSLO https://github.com/thewh1teagle/kokoro-onnx/releases/download/model-files-v1.0/kokoro-v1.0.onnx
curl -sSLO https://github.com/thewh1teagle/kokoro-onnx/releases/download/model-files-v1.0/voices-v1.0.bin
sudo apt install espeak-ng                 # fallback, and Kokoro's phonemiser
```

| artifact | bytes | sha256 |
| --- | --- | --- |
| `kokoro-v1.0.onnx` | 325 532 387 | `7d5df8ecf7d4b1878015a32686053fd0eebe2bc377234608764cc0ef3636a6c5` |
| `voices-v1.0.bin` | 28 214 398 | `bca610b8308e8d99f32e6fe4197e7ec01679264efed0cac9140fe9c29f1fbf7d` |

Pick the engine and voice with environment variables:

```sh
TTS=espeak ./turn.sh                       # fall back to robotic
KOKORO_VOICE=bm_george ./turn.sh           # 54 voices; af_heart is the default
KOKORO_SPEED=1.15 ./turn.sh
```

`turn.sh` falls back to espeak-ng automatically if the venv or models are absent.

### THE DEMO LINKS ONNXRUNTIME. THE PRODUCT DECISION IS DEFERRED AND UNMADE.

client-native's single-inference-runtime rule governs the **shipped binary** —
memory and load time on a handheld doing BLE ingest, classification and
generation at once. It does not reach this out-of-tree demo, which links nothing
into the product and promotes no row. **Nothing about this demo's dependency
graph is a precedent for client-native's.** A working demo must not become the
argument that the runtime decision was already taken; that is promotion by
implication wearing a new costume.

Kokoro does **not** clone voices — it ships 54 fixed ones. For a dialectic that
is a feature rather than a gap: the functional requirement is *distinguishable
poles*, and voice identity is not the point. Giving thesis and antithesis
different voices does something for a dialectic that cloning one voice does not.
It also keeps the consent surface smaller. See `TTS-EVALUATION.md`.

espeak-ng over Piper is deliberate: Piper is onnxruntime, a second inference
runtime in the same process, which is what `NeuralCompose/CLAUDE.md:41`'s
single-runtime rule exists to prevent. `llama-tts` **is** built in both trees at
this commit and would keep it single-runtime, but it needs OuteTTS-0.2-500M plus
WavTokenizer converted to GGUF — about 1 GB and two conversion passes
(`llama.cpp/tools/tts/README.md`). For proving the loop closes, espeak-ng is
right and its 1998 voice is not what is being demonstrated. **`llama-tts` is the
upgrade path when the voice starts mattering, and it needs no new runtime.**

**4. Run the server** (leave it up — this is the one Vulkan context):

```sh
~/src/llama.cpp/build-vulkan/bin/llama-server \
  -m ~/.cache/llama.cpp/Qwen_Qwen3-14B-GGUF_Qwen3-14B-Q4_K_M.gguf \
  -ngl 99 --host 127.0.0.1 --port 8080
```

Port 8080 is free; 8788 is the EEG stream's.

**5. Take a turn:** `./turn.sh` — speak, press Enter.

## `converse.py` — open mic, back and forth, with memory

The one you probably want. No Enter key: the mic stays open, it detects when you
start and stop talking, answers in 3–5 sentences, speaks, and listens again.
History is kept in-session and durable facts persist across sessions.

```sh
.venv/bin/python converse.py                    # Ctrl-C to stop
.venv/bin/python converse.py --turns 3 --silent # print only, bounded
.venv/bin/python converse.py --no-memory        # skip neural-memory entirely
```

### Silence detection, calibrated not hardcoded

The noise floor is measured at startup and speech is 3× it. A fixed threshold
would be wrong on this machine in both directions — the ALC245 runs ~40 dB hot,
and the measured floor here was **6262** with the speech gate at 18785. An
utterance ends after 1.2 s below the gate, caps at 30 s, and after 15 s of nothing
it prints `(listening…)` once rather than sitting mute.

Set the mic to 30% first (`wpctl set-volume @DEFAULT_AUDIO_SOURCE@ 30%`). At 100%
the input rails and whisper hears `[MUSIC PLAYING]` instead of words — that is
what made speaker-to-mic testing look impossible earlier. At 30% the same test
transcribes **perfectly**.

It reads raw samples from `arecord -t raw` rather than `pw-record`, because
`pw-record` writes a WAV header to stdout and this needs a bare sample stream. It
still goes through PipeWire via the ALSA compat layer.

### Memory: neural-memory over stdio, into a separate store

There is **no JEPA** in neural-memory-server — `grep -ri 'jepa|predictive'`
returns zero hits across the repo. What exists is lexical + entity + provenance
retrieval with an optional embedding branch, and that branch is what this uses.

Voice memories go to `~/.local/share/neural-memory/voice.db`, **not** the curated
evidence store, so conversational material never mixes with the 255-row corpus.
After each exchange the model is asked to extract one durable fact, or reply
`NONE`; only durable facts are written. Everything is `agentInference` — the
server clamps evidence class in two independent layers, so nothing spoken can
forge higher trust.

The server has **no HTTP listener**; it is stdio JSON-RPC (bare NDJSON, no
Content-Length framing), so this spawns it as a subprocess. Two wire-format traps,
both handled in `Memory`:

- results are **double-encoded** — `content[0].text` is a JSON *string*, not an object
- tool failures come back as `isError: true`, **not** as a JSON-RPC error
- `--as-of` is frozen at launch, so `asOf` is passed on every `recall`

### `remember` does not write vectors — verified

Measured: after two `remember` calls, `memories: 2, embeddings: 0`, while
`semanticBranch` reported `ran: true`. The branch was searching nothing.

So a query only matches lexically, and FTS is a pure **OR of tokens** — "what
beverage preferences does the user have" does **not** match "drinks his coffee
black". After backfilling, that same query returns it via `['semantic']` at 0.716,
which lexical missed completely.

`converse.py` therefore runs `neural-memory-embed` on exit, so a session's
memories are semantically findable by the next one. Requires a second llama-server
on port 8082 with nomic-embed-text v1.5, CPU-only:

```sh
~/src/llama.cpp/build-cpu/bin/llama-server \
  -m ~/.cache/llama.cpp/nomic-embed-text-v1.5.Q8_0.gguf \
  --embedding --host 127.0.0.1 --port 8082 -c 2048 -ngl 0
```

`-ngl 0` matters: it must not take a Vulkan context. Verified only llama-server
(the 14B) holds the render node with both servers up.

The project's own README says the semantic branch **did not replicate** in its
evaluation (+0.038 MRR against a +0.050 threshold) while the entity branch did
(+0.190 on alias queries). It is enabled here by choice; the honest expectation is
modest.

## `dialectic.py` — seed a topic, then listen

`turn.sh` is one exchange: you speak, one model answers. That is a chatbot with a
microphone, not a dialectic. `dialectic.py` is the loop on top of it — you seed a
topic and then listen while two positions argue.

```sh
.venv/bin/python dialectic.py "should cities ban private cars from their centres"
.venv/bin/python dialectic.py --mic                  # speak the topic, Enter to stop
.venv/bin/python dialectic.py --wav seed.wav --turns 8
.venv/bin/python dialectic.py --silent "..."         # print only, no audio
```

Three things make it a dialectic rather than two monologues:

- **Two positions.** Thesis argues the affirmative, antithesis presses the
  strongest objections. Each is told not to concede merely to be agreeable.
- **Tension carried across turns.** Each side receives the whole exchange, with
  the other's utterances as `user` and its own as `assistant`, so it answers what
  was actually said.
- **Distinguishable poles.** `af_heart` and `bm_george` by default
  (`VOICE_THESIS` / `VOICE_ANTITHESIS` to change). The listener tracks *which
  position is speaking* by voice — which is the whole reason Kokoro's fixed
  voices are a feature here and its inability to clone is not a gap.

Kokoro is loaded **once** for the whole exchange rather than per utterance, and
`temperature` is 0.85 because two instances of one model at low temperature
converge into agreement instead of holding a disagreement.

### It stages a disagreement; it does not hold one

Both positions are the same model with different instructions. There is no
independent reasoner on either side, and the appearance of two minds is a
property of the prompting, not of the system. Worth saying plainly, because a
spoken exchange in two voices is *much* more persuasive as evidence of
independent reasoning than it has any right to be.

### Two failures worth keeping

The first version leaked `--turns`' value into the topic (`topic: 4 should
cities ban…`) from hand-rolled `sys.argv` parsing — replaced with `argparse`,
which is stdlib and was the correct answer to begin with.

The first version also had Thesis *open* by rebutting a point nobody had made:
the style prompt says "respond to what they just said", and on turn 1 there is
nothing, so the model obligingly invented something to answer. Turn 1 now gets a
different instruction. A prompt that is right for every turn but the first will
look right in review and be wrong in production.

## Running it offline

**Setup needs network once. Running needs none.** Verified 2026-08-04 by running
the full loop with `HTTP_PROXY`/`HTTPS_PROXY` pointed at a dead port and
`HF_HUB_OFFLINE=1`, allowing only loopback:

```sh
env HTTP_PROXY=http://127.0.0.1:9 HTTPS_PROXY=http://127.0.0.1:9 \
    NO_PROXY=127.0.0.1,localhost HF_HUB_OFFLINE=1 \
    ./turn.sh ~/src/whisper.cpp/samples/jfk.wav          # completed in 13.8 s
```

`NO_PROXY` must include loopback or `curl` will try to proxy its own request to
llama-server, which fails for a reason that has nothing to do with being offline.

What each stage needs at run time:

| stage | needs network? | why |
| --- | --- | --- |
| llama-server | **no**, if started with `-m <path>` | `-hf` is the flag that downloads. Use a local path and it never reaches out |
| whisper.cpp | no | reads `ggml-base.en.bin` from disk |
| Kokoro | no | `kokoro-onnx` pulls **no** `huggingface_hub`; the ONNX file and voice pack are local |
| phonemisation | no | `espeakng-loader` bundles **both** `libespeak-ng.so` and `espeak-ng-data` inside the venv — the system `espeak-ng` package is not even required for Kokoro |
| onnxruntime | no | CPU provider, local wheel |

So the one-time network cost is: `pip install`, the two Kokoro artifacts, the
whisper model, and whatever GGUF you point llama-server at. Copy the venv, the
`models/` directory and the GGUF to an air-gapped machine and nothing else is
needed.

Both `.venv` and `models/` are gitignored, so they never travel with the repo.

### A fresh checkout of this directory does not run

`git clone` gives you four text files. **You must create `.venv` and `models/`
here before `./turn.sh` will do anything** — it exits with a message rather than
failing obscurely, but it will not run. Do the setup above, or point at an
existing one:

```sh
cd tools/spoken-loop
ln -s /path/to/existing/.venv .venv       # both are gitignored, symlink or real
ln -s /path/to/existing/models models
```

`turn.sh` must also be run **from this directory** — it is at
`tools/spoken-loop/turn.sh`, not the repository root — and any WAV you pass is a
path from wherever you are, not from here.

## Verified on 2026-08-04

| Stage | Result |
| --- | --- |
| whisper.cpp, CPU | JFK sample transcribed correctly |
| llama-server, Vulkan | model loaded in 8.5 s; `/health` → `{"status":"ok"}` |
| Actually on GPU | `gpu_busy_percent` 59, `mem_info_vram_used` 8.0 G, PID holds `renderD128` |
| HTTP seam | chat completion returned in 0.97 s |
| `enable_thinking:false` | clean reply, no `<think>` block |
| Mic capture | 16 kHz mono s16 WAV, signal present |
| espeak-ng | installed; whisper transcribes its output, 1 word off in 12 |
| Kokoro-82M | 5.08 s of audio in 1.92 s wall — **2.6× realtime** on CPU, incl. model load |
| Kokoro intelligibility | whisper round-trip clean; only "prosody" missed, and closer than espeak's |
| onnxruntime providers | `['AzureExecutionProvider', 'CPUExecutionProvider']` — **no GPU provider**, so it cannot race llama-server's Vulkan context |
| **Full loop** | `./turn.sh ~/src/whisper.cpp/samples/jfk.wav` → heard → replied → spoke, **17.6 s** |

### Tell the model its reply will be spoken

The first full-loop run produced `**bold**`, `###` headings and a blockquote —
which a TTS reads aloud as punctuation noise. The root fix is a system prompt
saying the reply will be spoken and must be plain prose; `turn.sh` also strips
markdown as insurance. That change alone took the turn from 68 s to 17.6 s,
because the model stopped writing an essay.

Qwen3 emits `<think>` blocks by default, and a spoken loop would read the
reasoning aloud. `chat_template_kwargs.enable_thinking = false` suppresses it;
`turn.sh` also strips any residual block, in case a template ignores the flag.

## Calibration: the mic clips, and 60 % is not enough

A bare ambient capture peaked at **100 % full scale** with 23 % of samples pinned
at the rail, and whisper read the transient as `[GUNSHOT]`.

**Use 30 %, not 60 %.** Measured on 2026-08-04, 2 s ambient captures:

| `wpctl` source volume | peak | rms | mean |
| --- | --- | --- | --- |
| 100 % | 32768 (railed) | 23222 | −17431 |
| 60 % | 32768 (railed) | 15394 | −10665 |
| 30 % | 5239 | 1261 | −421 |
| 10 % | 865 | 47 | −1 |

```sh
wpctl set-volume @DEFAULT_AUDIO_SOURCE@ 30%
```

Two traps found while calibrating, both of which cost real time:

- **The apparent DC offset is an artefact, not a fault.** A mean of −17431 looks
  like a broken or floating input. It is asymmetric rail-pinning skewing the
  mean, and it vanishes to −1 once the signal has headroom. Do not go hunting for
  a hardware fault on the strength of it.
- **`amixer` changes do not stick.** The underlying ALSA gain is enormous —
  `Capture` at 63/63 (**+30 dB**) plus `Internal Mic Boost` (**+10 dB**) — but
  PipeWire manages the mixer and reverts `amixer sset` almost immediately. Set
  capture gain through `wpctl`, not `amixer`.

Tune against your actual voice and distance. A real microphone in a real room is
not the one on the datasheet, and this one runs 40 dB hot.

### Speaker-to-mic loopback does not work on this machine

Playing synthesized speech through the speakers and capturing it on the internal
mic produced `[MUSIC PLAYING]` from whisper — the handheld's speaker/mic
proximity and enclosure make the acoustic path unusable for self-testing. Test
components against a **file** instead: whisper transcribes espeak's own output
essentially correctly (one word off in a 12-word sentence).

## Seat-scoped, like everything else here

PipeWire and pipewire-pulse are **user-session services**, so a systemd *system*
service has no sound path at all. This is the same class as the
`/dev/accel/accel0` logind-ACL finding and `/dev/kfd`: fine for an interactive
demo, and a decision the moment anything is meant to run unattended.

Note also there is no `pactl` on this machine — use `pw-record` / `pw-play` /
`wpctl`.

## If you ever time this

Capture power state first, with
`neuralcompose-client-native/scripts/capture-power-state.sh`. The 890M
power-gates, and the 14B's 147.91 pp512 figure comes from the AC/battery pair
with the unresolved confound. A latency number taken here without power state
repeats that mistake exactly.

## Deliberately not here

- **No multi-turn, no dialectic.** One turn. The dialectic waits until the loop
  is boring.
- **No VAD, no barge-in, no streaming.** Push-to-talk, whole utterance, whole
  reply. No silence-detection tuning, and a demo operator knows when they stopped
  talking.
- **No `audio.rs` integration.** The core's recording state machine
  (`Idle → Ready → Recording → Persisting → Recorded …`) is the right contract
  eventually — this demo owns the mic, files and clock exactly as a shell would —
  but wiring it in before the loop works adds a second failure surface.
