# tools/launcher

One click starts a hypnagogic session: `llama-server`, an EEG source if you want
one, then the dialectic loop — and stops again what it started.

```sh
tools/launcher/install.sh     # writes ~/.local/share/applications/neuralcompose.desktop
```

Then find "NeuralCompose Session" in your application menu. Or run the script
directly, which is the same thing without the icon:

```sh
tools/launcher/neuralcompose-session
```

## The thing this exists to catch

**`/usr/local/bin/llama-server` on this machine cannot use the GPU, and says so
in a way that is easy to miss.** It is rpath-linked to
`~/src/llama.cpp/build-cpu/bin`:

```console
$ ldd /usr/local/bin/llama-server | grep ggml
    libggml.so.0      => .../build-cpu/bin/libggml.so.0
    libggml-base.so.0 => .../build-cpu/bin/libggml-base.so.0
    libggml-cpu.so.0  => .../build-cpu/bin/libggml-cpu.so.0
```

ggml backends are separate shared objects loaded at runtime from the library
directory. There is no `libggml-vulkan.so` in `build-cpu/bin`, so that binary
cannot find the Vulkan backend however you invoke it — `-ngl 99` is accepted and
ignored, with one line of warning, and inference silently runs on CPU.

`~/src/llama.cpp/build-vulkan/bin/llama-server` works. Measured on this machine:
`AMD Radeon 890M Graphics (RADV STRIX1)`, `uma: 1`, `fp16: 1`, `KHR_coopmat`.

So the launcher prefers the `build-vulkan` binary, and then **checks it got what
it asked for** rather than trusting the request — the same rule
`Embedder::backend_id()` follows in the Rust.

If you want the fix system-wide, point the symlink at the working build. That is
your call and the launcher does not do it for you:

```sh
sudo ln -sf ~/src/llama.cpp/build-vulkan/bin/llama-server /usr/local/bin/llama-server
```

## How the backend is determined

Not by parsing the server log. Build 10188's `llama-server` prints **nothing**
about its backend at default verbosity — `llama-bench` prints the
`ggml_vulkan: 0 = ...` device line, the server does not — so an early version of
this script grepped for a string that is never written and reported CPU for a
working Vulkan session.

What it uses instead, in order:

| Signal | Verdict |
| --- | --- |
| `NGL=0` | CPU by request. Not a failure. |
| `no usable GPU found` in the server's log | **CPU, loudly.** Names the rpath cause above. |
| `llama-server` holds `/dev/dri/renderD128` | Vulkan, confirmed. |
| none of the above | **Unproven** — said as such, never assumed either way. |

The render-node check is `tools/spoken-loop/README.md:63` automated. It is a
direct observation of GPU use rather than a claim about it.

An **adopted** server is not inspected at all, and the launcher says so: we did
not start it and its log is not ours to read.

## Adoption

If something healthy is already answering on the port, the launcher uses it and
**leaves it running on exit**. Only processes it started are killed. Clicking the
icon never kills a server you were using.

## Configuration

Optional, at `~/.config/neuralcompose/launcher.conf` — plain shell, sourced. Any
key can also be set as an environment variable for a one-off run.

| Key | Default | Notes |
| --- | --- | --- |
| `MODEL` | `~/.cache/llama.cpp/Qwen_Qwen3-8B-GGUF_Qwen3-8B-Q6_K.gguf` | generation |
| `EMBED_MODEL` | `~/models/bge-small-en-v1.5-f32.gguf` | in-process, CPU |
| `LLAMA_SERVER` | prefers `build-vulkan`, else PATH | |
| `SERVER_URL` | `http://127.0.0.1:8080` | |
| `NGL` | `99` | `0` = CPU by request |
| `MODE` | `reflective` | the only profile with the Witness branch |
| `TURNS` | `8` | |
| `EEG` | `none` | `none` \| `fixture` \| `muse` |
| `SPEAK` / `TTS` | `1` / `kokoro` | `espeak` needs no venv |
| `MIC` | `0` | push-to-talk: speak, then press Enter |
| `LOG` | `1` | with `EEG` set, also writes the raw capture (~88 MB/hour) |

```sh
# one-off
MODE=mirror SPEAK=0 tools/launcher/neuralcompose-session
```

## The stub build, which is the easy mistake

`cargo build` or `cargo test` **without** `LLAMA_CPP_DIR` compiles
`neuralcompose-llama` to a stub whose every call returns `Unavailable` — on
purpose, so CI on four platforms needs no C++ toolchain. The binary looks
identical, starts fine, brings the whole stack up, and then dies at the embedder
minutes later. The `cargo test --workspace` in the project docs produces exactly
this.

The launcher refuses up front instead. A stub links no `libllama`, so `ldd` tells
them apart with no guessing:

```console
$ ldd target/release/neuralcompose-hypnagogic | grep -c llama
4      # real build
0      # stub
```

Mirror mode needs no embedder and runs fine on a stub, so the check is skipped
for `MODE=mirror` rather than blocking a mode that works.

## Order of operations

Every input is checked **before anything is started** — the stub check, the
model, the embedding model, and the Kokoro venv and models if `SPEAK=1`. A
missing file discovered after `llama-server` is up would leave an orphan behind
and report an error for a session that had already started something.

Teardown runs from a `trap` on `EXIT INT TERM HUP`, so closing the terminal
window reaps the children. It is idempotent, because the trap can fire twice.

## What this is not

Not packaging. `ReleaseSupported` in `docs/support-matrix.md` means signed
packaging, install, upgrade, removal and acceptance gates pass; this is a shell
script and a `.desktop` file. **It promotes no support-matrix row** and
`attained_support_status()` returns exactly what it returned before.

Not a GUI, and deliberately not. A native toolkit would put the app process on
`/dev/dri/renderD128`, taking a second Vulkan context on the same iGPU running
inference — the ceiling the code asserts at startup. It would also be the fourth
UI surface `crates/neuralcompose-headless/src/main.rs:1-5` refuses by name.

## Verified, and not

Checked on this machine, 2026-08-14, GNOME 
on Wayland with `xdg-terminal-exec` present:

- cold start, session runs, close window → no orphaned processes
- a hand-started server is adopted and still running afterwards
- `build-vulkan` → Vulkan confirmed on the render node; `/usr/local/bin` → loud
  CPU warning naming the cause. Both directions, because a launcher that
  reported Vulkan in both cases would have failed
- missing model → one sentence, non-zero exit, nothing started
- a stub build → refused before anything starts, with the rebuild command;
  `MODE=mirror` still runs on it
- a full run writes all five artifacts, and `--verify-log` and
  `--verify-capture` both pass on them
- port 8788 already held → refused, rather than a session against whatever is
  on that port (`docs/acceptance/linux-headless-runtime.md:189-193`)

**Not verified:** `EEG=muse` beyond its refusal path — that needs the headband.
`Terminal=true` on any desktop other than this one; it is resolved by
`xdg-terminal-exec`, and without that some environments run the entry with no
terminal attached, which for an interactive session means nowhere to type.
