#!/usr/bin/env bash
#
# One spoken turn: mic -> whisper.cpp (CPU) -> llama-server (Vulkan) -> espeak-ng -> speakers.
#
# NOT CLAIMED, AND THIS MATTERS MORE THAN WHAT IS
#
# This demo promotes no support-matrix row. `attained_support_status()` returns
# None for everything here and must continue to. It is not a
# RuntimeSmokeValidated candidate: that rung requires a DETERMINISTIC FIXTURE
# MODEL, and a live microphone is the opposite of deterministic. It links no
# nc-gguf-v1 backend under any row's terms — it talks to llama-server over HTTP,
# which is a third transport mechanism (the repos use WebSocket and subprocess),
# chosen for this demo and NOT a precedent.
#
# What it does prove is narrower and real: that a spoken turn completes on this
# machine, on Linux, with no Apple framework and no cloud.
#
# EXACTLY ONE VULKAN CONTEXT. llama-server holds it, long-lived, created once.
# whisper is built -DGGML_VULKAN=OFF and espeak-ng touches no GPU. PR #30 fixed
# concurrent Vulkan context creation INSIDE one process with a process-local
# device lock; a process-local mutex cannot serialise across processes, so a
# Vulkan-built whisper starting alongside the server would be a race nothing
# guards. Check with: fuser -v /dev/dri/renderD128
#
# SEAT-SCOPED. PipeWire is a user-session service, so a systemd system service
# has no sound path — same class as /dev/accel/accel0's logind ACL. Fine for an
# interactive demo; a decision the moment anything runs unattended.

set -euo pipefail

WHISPER="${WHISPER:-$HOME/src/whisper.cpp/build/bin/whisper-cli}"
WMODEL="${WMODEL:-$HOME/src/whisper.cpp/models/ggml-base.en.bin}"
SERVER="${SERVER:-http://127.0.0.1:8080}"
WORKDIR="$(mktemp -d)"
trap 'rm -rf "$WORKDIR"' EXIT

IN="$WORKDIR/turn.wav"
OUT="$WORKDIR/reply.wav"

for f in "$WHISPER" "$WMODEL"; do
  [ -e "$f" ] || { echo "missing: $f" >&2; exit 1; }
done

# TTS engine. kokoro = Kokoro-82M via onnxruntime (natural prosody, 54 voices,
# no cloning); espeak = espeak-ng (robotic, no model, always available).
# Falls back to espeak if the kokoro venv or models are missing.
TTS="${TTS:-kokoro}"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
if [ "$TTS" = kokoro ] &&
   ! { [ -x "$HERE/.venv/bin/python" ] && [ -f "$HERE/models/kokoro-v1.0.onnx" ]; }; then
  echo "kokoro not installed, falling back to espeak-ng (see README)" >&2
  TTS=espeak
fi
[ "$TTS" != espeak ] || command -v espeak-ng >/dev/null ||
  { echo "espeak-ng not installed — run: sudo apt install espeak-ng" >&2; exit 1; }
curl -sS --max-time 3 "$SERVER/health" >/dev/null 2>&1 ||
  { echo "llama-server not answering at $SERVER — start it first (see README)" >&2; exit 1; }

# 1. Record. Push-to-talk rather than VAD or a fixed window: no silence-detection
#    tuning, and a demo operator knows when they stopped talking.
# An input WAV as $1 skips the mic — lets the loop be exercised without a human,
# which is how it was tested (speaker-to-mic loopback does not work on this box).
if [ $# -ge 1 ]; then
  cp "$1" "$IN"
  echo "input: $1 (mic skipped)"
else
  echo "Speak now. Press Enter when you're done."
  pw-record --rate 16000 --channels 1 --format s16 "$IN" &
  REC=$!
  read -r _ || true
  kill -INT "$REC" 2>/dev/null || true
  wait "$REC" 2>/dev/null || true
fi
[ -s "$IN" ] || { echo "no audio captured" >&2; exit 1; }

# The ALC245 runs ~40 dB hot (ALSA Capture +30 dB, Internal Mic Boost +10 dB) and
# rails on bare ambient noise; whisper read the transient as "[GUNSHOT]". Clipped
# input transcribes badly, so warn rather than silently feeding whisper a
# squared-off waveform. 60% is NOT enough — measured, still railed. Use:
#   wpctl set-volume @DEFAULT_AUDIO_SOURCE@ 30%
# Set it with wpctl, not amixer: PipeWire manages the mixer and reverts amixer.
python3 - "$IN" <<'PY' || true
import array, sys, wave
w = wave.open(sys.argv[1]); a = array.array('h'); a.frombytes(w.readframes(w.getnframes()))
if a and max(max(a), -min(a)) >= 32767:
    print("warning: input is CLIPPING — lower mic gain "
          "(wpctl set-volume @DEFAULT_AUDIO_SOURCE@ 60%)", file=sys.stderr)
PY

# 2. Transcribe on CPU.
HEARD=$("$WHISPER" -m "$WMODEL" -f "$IN" -nt -np 2>/dev/null |
        tr '\n' ' ' | sed 's/^[[:space:]]*//; s/[[:space:]]*$//')
[ -n "$HEARD" ] || { echo "heard nothing" >&2; exit 1; }
echo "heard: $HEARD"

# 3. Generate. enable_thinking=false matters: Qwen3 emits <think> blocks by
#    default and a spoken loop would read the reasoning aloud. The sed is a
#    belt-and-braces strip in case a template ignores the flag.
#    The system prompt is the root fix for markdown: a chat model left to itself
#    answers with **bold**, ### headings and bullets, and a TTS reads those
#    aloud as punctuation noise. Telling it the reply will be SPOKEN fixes the
#    cause; the sed below is cheap insurance for when it forgets.
SYS='Your reply will be spoken aloud by a text-to-speech engine. Reply in plain
conversational prose: no markdown, no asterisks, no headings, no bullet points,
no emoji. Two or three sentences at most.'
SAID=$(jq -n --arg t "$HEARD" --arg s "$SYS" '{
         messages: [{role: "system", content: $s}, {role: "user", content: $t}],
         chat_template_kwargs: {enable_thinking: false},
         max_tokens: 120,
         stream: false
       }' |
       curl -sS "$SERVER/v1/chat/completions" \
            -H 'content-type: application/json' -d @- |
       jq -r '.choices[0].message.content' |
       perl -0777 -pe 's{<think>.*?</think>}{}gs' |
       sed -E 's/\*+//g; s/^#+[[:space:]]*//gm; s/^>[[:space:]]*//gm; s/^[[:space:]]*[-*][[:space:]]+//gm' |
       tr '\n' ' ' | sed 's/^[[:space:]]*//; s/[[:space:]]*$//')
[ -n "$SAID" ] || { echo "model returned nothing" >&2; exit 1; }
echo "said:  $SAID"

# 4. Speak. Kokoro runs on CPU via onnxruntime — it must not take a Vulkan
#    context, or it races llama-server's (see the design rule above).
if [ "$TTS" = kokoro ]; then
  "$HERE/.venv/bin/python" "$HERE/speak.py" "$OUT" "$SAID"
else
  espeak-ng -w "$OUT" "$SAID"
fi
pw-play "$OUT"
