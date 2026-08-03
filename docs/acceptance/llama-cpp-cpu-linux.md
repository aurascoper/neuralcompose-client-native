# `linux/x86_64/llama-cpp-cpu` — runtime smoke acceptance

Date: 2026-08-03
Branch: `feat/llama-cpp-cpu-backend`
Base: `22ff650`

The first model backend in this repository that has ever executed. Every row in
`docs/support-matrix.md` has read `Contracted` — "schema and deterministic Rust
behaviour exist, and nothing has ever executed" — since the matrix was created
on 2026-07-29.

## Claim

| Claim | State |
|---|---|
| A GGUF model loads and runs through Rust on `linux/x86_64` | **ACHIEVED** |
| Output agrees with llama.cpp's own reference binary | **ACHIEVED — <2e-6 across sampled dimensions** |
| The same input produces byte-identical output | **ACHIEVED** |
| `linux/x86_64/llama-cpp-cpu` → `RuntimeSmokeValidated` | **PROPOSED — see Scope** |
| Any other row promoted | **NO** |

## Named hardware and software

| Field | Value |
|---|---|
| Machine | GPD, AMD Ryzen AI 9 HX 370 w/ Radeon 890M (Strix Point) |
| OS | Ubuntu 26.04 LTS, kernel 7.0.0-28-generic |
| CPU | 24 cores; 22 GiB RAM total |
| Rust | 1.97.1 |
| Compiler | g++/gcc 15.2.0 (Ubuntu 15.2.0-16ubuntu1), cmake 4.2.3 |
| llama.cpp | `d0bfb1981266c271cd0536a8aa7c5e863e7cdf61` |
| Build flags | `-DBUILD_SHARED_LIBS=ON -DGGML_VULKAN=OFF -DGGML_NATIVE=ON` |
| Backend ID | `llama-cpp-cpu` |
| Runtime ABI | `nc-gguf-v1` |
| Fixture model | bge-small-en-v1.5, f32 GGUF, 384-dim, 33.2 M params |
| Model size | 133 609 568 bytes |
| Model sha256 | `bf40c42ad7d89382e9ba7376d5c4b73f6b556cb541fab37aaa1da9c320149b65` |

`n_gpu_layers` is pinned to 0 in the shim. A row claiming `llama-cpp-cpu` must
not silently offload, or the evidence would describe a different backend than
the one named — and this machine has a working Vulkan device (`RADV STRIX1`)
that llama.cpp would happily have used.

## Why the FFI goes through a C shim

`llama_model_params` and `llama_context_params` are large structs that change
between llama.cpp releases. Hand-declaring them in Rust means maintaining a
byte-exact mirror of a moving target, and a mismatch is **not** a compile error
— it is silent memory corruption, because caller and callee disagree about
where a field lives. `bindgen` would solve it and is a build dependency this
repo has not taken.

So the version-sensitive part stays in `csrc/nc_llama_shim.c`, compiled against
the real `llama.h` where the compiler checks it. Rust sees only opaque pointers,
`int32_t`, `float*` and `const char*` — types whose layout the platform ABI
fixes. When llama.cpp changes its API, the shim fails to compile, loudly, at
build time.

## Agreement with the reference implementation

The oracle is llama.cpp's own `llama-embedding` binary, built from the same
checkout. Reference values were produced by running it and pasting the output —
not written by hand. That precaution is not theoretical: an earlier draft of the
`band_power` agreement test carried plausible-looking constants that were
invented, and they were **wrong by a third**.

```
llama-embedding -m bge-small-en-v1.5-f32.gguf -p "the headband is on" \
  --embd-output-format json
  → [-0.0113142, -0.0021324, 0.0365841, -0.0786307, 0.024272, …]

neuralcompose-headless --embed "the headband is on"
  → [-0.0113142, -0.0021324, 0.0365841, -0.0786307, 0.0242720, …]
```

Eight dimensions sampled across each of two strings, all within 2e-6. The
tolerance matches the ~7 significant figures the reference JSON prints; a looser
bound would let a genuinely different pooling strategy pass.

Semantic anchors, also measured from the oracle: cosine 0.818246 between "the
headband is on" and "the EEG headset is being worn", 0.438293 against
"quarterly tax filing deadline". A backend returning a constant vector would
pass every numeric agreement test and fail this one.

## A skipped test reports `ok` — so skipping is made loud

The agreement tests skip when the backend or the model is absent, because
neither is in the repository and CI has no C++ toolchain guarantee and no
133 MB model. But **a skipped Rust test still prints `ok`**, so a stub build
produced exactly the same "6 passed" line as a real one — an unbuilt backend
looking verified, which is precisely the failure this file exists to prevent.

`NC_REQUIRE_BACKEND=1` converts every skip into a hard failure, and is verified
to do so in both directions:

| build | `NC_REQUIRE_BACKEND=1` |
|---|---|
| stub (`LLAMA_CPP_DIR` unset) | **fails** — "refusing to report a pass that verifies nothing" |
| real | passes, 6/6 |

Any run producing acceptance evidence must set it.

## Workspace state

| build | clippy | tests |
|---|---|---|
| stub (what CI sees) | 0 errors | 184 passed, 0 failed |
| real, `NC_REQUIRE_BACKEND=1` | 0 errors | 184 passed, 0 failed |

The crate compiles to a stub with `LLAMA_CPP_DIR` unset, so CI on five runners
stays green without a C++ toolchain, and `is_available()` reports the difference
at runtime.

## Scope — what this does and does not promote

**Proposed:** `linux/x86_64` / `llama-cpp-cpu` / `nc-gguf-v1`,
`Contracted → RuntimeSmokeValidated`. The rung's definition is "a deterministic
fixture model executed successfully," and all three words are separately
evidenced above: deterministic (byte-identical across runs), fixture (a named
model with a recorded digest, not the candidate), executed (through Rust, on
this named machine).

**Not promoted, and none of it follows:**

- **`linux/x86_64/llama-cpp-vulkan`.** A different backend. Vulkan works on this
  machine and was explicitly compiled out; nothing here is evidence for it.
- **The three Windows and Android/iOS rows.** Different OSes, never run.
- **`DeviceValidated` for this row.** That rung requires the *real candidate
  model*, and bge-small-en-v1.5 is a fixture chosen for size and determinism.
- **Anything about the EEG path.** `--embed` deliberately opens no socket and
  touches no headband: the model backend and the ingest path are separate
  claims, and running them in one command would blur which one any evidence is
  about.
- **`ReleaseSupported`.** No signing, packaging, install, upgrade or removal
  exists.

## Non-claims

- **llama.cpp is not vendored and not pinned by this repository.** It is located
  at build time via `LLAMA_CPP_DIR`. The commit above is recorded in the binary
  through `NC_LLAMA_COMMIT` at build time so it cannot drift from the artefact,
  but nothing prevents a different checkout being used tomorrow.
- **The model is not in the repository** and there is no verification policy
  wired to its digest yet. The sha256 above is recorded here by hand; the
  `model_pack` contract exists in the core but this backend does not yet consult
  it.
- **No performance claim.** Latency was not measured and no throughput target
  exists.
- **`Embedder` is neither `Send` nor `Sync`,** and this has not been tested
  under concurrency — the types are withheld on the conservative reading of
  llama.cpp's threading model, not on evidence that sharing fails.
- **One fixture model, one architecture (BERT), one quantisation (f32).** A
  quantised model or a different architecture has never been loaded through this
  path.
