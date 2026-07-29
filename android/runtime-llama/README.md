# android/runtime-llama

The M7-B on-device generation substrate: a **minimal, pinned** llama.cpp CPU
runtime for Android arm64, plus a thin NeuralCompose JNI wrapper.

## Not vendored, and not opaque

Nothing here ships a prebuilt binary or a source dump. `build-llama-android.sh`
fetches llama.cpp at the exact commit in `llama-source.lock.json`, **verifies
the source tree hash before compiling anything**, and builds with the exact
flags recorded in that lock file. A moved tag or a tampered tree fails the
build rather than silently producing a different runtime.

Build outputs live under `build/` and are git-ignored. Model weights are never
committed and never packaged into the APK.

```sh
./build-llama-android.sh          # verifies, then cross-compiles for arm64-v8a
```

## Scope

In: Android **arm64-v8a**, **CPU** backend, the two pinned M7-B Qwen
candidates (Qwen2.5-0.5B-Instruct Q4_K_M and Qwen3-0.6B Q4_K_M), a **single
active inference session**.

Out, deliberately: Vulkan, NNAPI, QNN, x86/x86_64 Android, iOS, desktop,
arbitrary model families, runtime-pack downloading, and multiple simultaneous
models. This is an M7-B execution path, not general-purpose infrastructure —
if it starts becoming that, it should leave this PR and become its own
milestone.

## JNI surface

Deliberately small. Kotlin holds an **opaque handle** and never touches a
native pointer, and the native layer never accepts an arbitrary path from UI
code — only a path the model-pack layer has already verified.

```text
createSession(verifiedModelPath, runtimeConfig) -> handle
inspectModel(handle)                            -> resolved GGUF metadata
renderChatPrompt(handle, messages, reasoningMode) -> rendered bytes + hashes
generate(handle, prompt, sampler, callback)
cancel(handle)
destroySession(handle)
```

Load order, with any disagreement failing *before* inference:

```text
model-pack phase Ready
→ recheck verified path and active usability
→ open the exact verified artifact
→ inspect GGUF metadata
→ compare against manifest and conversion record
→ bind resolved model + variant + runtime identities
→ create native context
```

## Why prompt rendering is native

Non-thinking mode is a benchmark **admission gate**, not a preference. Qwen3's
non-thinking template does not omit the reasoning block — it pre-fills an
empty, already-closed one, and feeding that prompt by hand still produced
reasoning prose while emitting no `<think>` tag at all (see
`docs/acceptance/m7-b-conversion.md`).

So the prompt is rendered by the **pinned llama.cpp template machinery** with
reasoning off — never by Kotlin string concatenation and never by a separate
handwritten mobile template. The host harness and this adapter must produce
identical rendered-prompt and input-token hashes for every frozen corpus case,
or the run is inadmissible.
