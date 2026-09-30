#!/usr/bin/env python3
"""Kokoro-82M TTS: text on argv or stdin -> WAV file.

Replaces espeak-ng in turn.sh for natural prosody. Kokoro is Apache-2.0, 82M
params, CPU-only here — it does NOT clone voices, it ships 54 fixed ones. For a
dialectic that is a feature: the functional requirement is distinguishable
poles, not voice identity.

THE DEMO LINKS ONNXRUNTIME. THE RUNTIME DECISION FOR THE PRODUCT IS DEFERRED AND
UNMADE. client-native's single-inference-runtime rule governs the shipped
binary's memory and load time on a handheld; it does not reach this out-of-tree
demo, which links nothing into the product and promotes no support-matrix row.
Nothing here is a precedent for what the product links.

Usage:  speak.py OUT.wav [TEXT...]        (text from argv, else stdin)
Env:    KOKORO_VOICE (default af_heart), KOKORO_SPEED (default 1.0),
        KOKORO_VOLUME (default 1.0, a linear sample scale)

KOKORO_VOLUME exists because Kokoro has no volume parameter of its own, and
`Prosody.volume` is a real dimension of the dialectic's voices — a receding
pole is quieter than a present one. Applied as a plain sample scale after
synthesis, clamped so a stray value cannot clip the output into noise.
"""
import os
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
MODEL = HERE / "models" / "kokoro-v1.0.onnx"
VOICES = HERE / "models" / "voices-v1.0.bin"


def main() -> int:
    if len(sys.argv) < 2:
        print(__doc__, file=sys.stderr)
        return 2
    out = sys.argv[1]
    text = " ".join(sys.argv[2:]).strip() or sys.stdin.read().strip()
    if not text:
        print("speak.py: nothing to say", file=sys.stderr)
        return 1

    for p in (MODEL, VOICES):
        if not p.exists():
            print(f"speak.py: missing {p} — see README", file=sys.stderr)
            return 1

    import soundfile as sf
    from kokoro_onnx import Kokoro

    kokoro = Kokoro(str(MODEL), str(VOICES))
    samples, rate = kokoro.create(
        text,
        voice=os.environ.get("KOKORO_VOICE", "af_heart"),
        speed=float(os.environ.get("KOKORO_SPEED", "1.0")),
        lang="en-us",
    )
    # Clamped at 2.0: above that a scale stops being volume and starts being
    # clipping, which is audible as distortion rather than as loudness.
    volume = max(0.0, min(2.0, float(os.environ.get("KOKORO_VOLUME", "1.0"))))
    if volume != 1.0:
        samples = samples * volume

    sf.write(out, samples, rate)
    return 0


if __name__ == "__main__":
    sys.exit(main())
