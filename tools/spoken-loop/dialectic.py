#!/usr/bin/env python3
"""A spoken dialectic: you seed a topic, then two positions argue and you listen.

Thesis and antithesis alternate, each hearing the other, each in its own voice.
Distinguishable poles are the functional requirement — a listener tracks WHICH
POSITION is speaking by voice, not by remembering who said what. That is why
Kokoro's fixed voices are a feature here and its inability to clone is not a gap.

NOT CLAIMED: this promotes no support-matrix row, links nothing into the product,
and is not evidence that any backend works. It is a demo in tools/. It links
onnxruntime for Kokoro; the product's runtime decision is deferred and unmade.
See README.md.

The dialectic is genuinely two-sided only in form. Both positions are the same
model with different instructions, so this stages a disagreement rather than
holding one — no independent reasoner is on either side.

Usage:
  dialectic.py "should cities ban cars"      topic as text
  dialectic.py --wav seed.wav                topic transcribed from a WAV
  dialectic.py --mic                         topic spoken, Enter to stop
Options:
  --turns N        total utterances, default 6 (3 each)
  --silent         print only, do not speak
"""
import json
import os
import subprocess
import sys
import tempfile
import urllib.request
from pathlib import Path

from speak import MODEL, VOICES  # same artifacts as the single-turn loop

HERE = Path(__file__).resolve().parent
SERVER = os.environ.get("SERVER", "http://127.0.0.1:8080")
WHISPER = Path(os.environ.get("WHISPER", Path.home() / "src/whisper.cpp/build/bin/whisper-cli"))
WMODEL = Path(os.environ.get("WMODEL", Path.home() / "src/whisper.cpp/models/ggml-base.en.bin"))

STYLE = (
    "Your reply is SPOKEN ALOUD. Plain conversational prose only: no markdown, "
    "no asterisks, no headings, no lists, no stage directions. Two or three "
    "sentences, never more. Do not concede merely to be agreeable, and do not "
    "repeat a point you have already made."
)
# Turn 1 has nothing to answer. Without this the opener rebuts an argument
# nobody made — the style prompt says "respond to what they just said", and the
# model duly invents something to respond to.
OPENING = " Open the discussion by stating your position. Nothing has been said yet."
REPLYING = (" Speak directly to the other position: respond to what they actually "
            "just said rather than restating your own case.")

POSITIONS = [
    ("Thesis", os.environ.get("VOICE_THESIS", "af_heart"),
     "You hold the AFFIRMATIVE position on the topic. Argue for it with concrete "
     "reasons and examples."),
    ("Antithesis", os.environ.get("VOICE_ANTITHESIS", "bm_george"),
     "You hold the NEGATIVE position on the topic. Press the strongest objections "
     "and expose what the affirmative case overlooks."),
]


def transcribe(wav: Path) -> str:
    for p in (WHISPER, WMODEL):
        if not p.exists():
            sys.exit(f"dialectic: missing {p}")
    out = subprocess.run([str(WHISPER), "-m", str(WMODEL), "-f", str(wav), "-nt", "-np"],
                         capture_output=True, text=True).stdout
    return " ".join(out.split())


def record() -> str:
    """Push-to-talk, same contract as turn.sh: speak, press Enter."""
    with tempfile.TemporaryDirectory() as d:
        wav = Path(d) / "seed.wav"
        print("Speak the topic. Press Enter when you're done.", file=sys.stderr)
        rec = subprocess.Popen(
            ["pw-record", "--rate", "16000", "--channels", "1", "--format", "s16", str(wav)])
        try:
            input()
        except EOFError:
            pass
        rec.terminate()
        rec.wait()
        return transcribe(wav)


def say_llm(system: str, exchange: list[tuple[str, str]], speaker: str) -> str:
    """`exchange` is [(position_name, text)]. The other side's turns are `user`."""
    messages = [{"role": "system", "content": f"{system}\n\n{STYLE}"}]
    for who, text in exchange:
        messages.append({"role": "assistant" if who == speaker else "user", "content": text})
    body = json.dumps({
        "messages": messages,
        "chat_template_kwargs": {"enable_thinking": False},
        "max_tokens": 160,
        "temperature": 0.85,   # two identical models need some spread to diverge
        "stream": False,
    }).encode()
    req = urllib.request.Request(f"{SERVER}/v1/chat/completions", data=body,
                                 headers={"content-type": "application/json"})
    with urllib.request.urlopen(req, timeout=180) as r:
        text = json.load(r)["choices"][0]["message"]["content"]
    # Belt-and-braces: the style prompt is the root fix, this catches lapses.
    for bad in ("**", "*", "#", ">"):
        text = text.replace(bad, "")
    return " ".join(text.split())


def main() -> int:
    import argparse

    ap = argparse.ArgumentParser(description="A spoken dialectic: seed a topic, then listen.")
    ap.add_argument("topic", nargs="*", help="topic as text")
    ap.add_argument("--wav", type=Path, help="transcribe the topic from a WAV")
    ap.add_argument("--mic", action="store_true", help="speak the topic, Enter to stop")
    ap.add_argument("--turns", type=int, default=6, help="total utterances (default 6)")
    ap.add_argument("--silent", action="store_true", help="print only, do not speak")
    opt = ap.parse_args()
    turns, silent = opt.turns, opt.silent

    if opt.mic:
        topic = record()
    elif opt.wav:
        topic = transcribe(opt.wav)
    else:
        topic = " ".join(opt.topic).strip()
    if not topic:
        ap.error("give a topic, or --wav FILE, or --mic")
    print(f"topic: {topic}\n")

    kokoro = None
    if not silent:
        from kokoro_onnx import Kokoro
        kokoro = Kokoro(str(MODEL), str(VOICES))  # loaded ONCE, not per turn

    exchange: list[tuple[str, str]] = []
    for i in range(turns):
        name, voice, stance = POSITIONS[i % 2]
        system = f'The topic is: "{topic}". {stance}'
        system += OPENING if not exchange else REPLYING
        text = say_llm(system, exchange, name)
        exchange.append((name, text))
        print(f"{name} ({voice}): {text}\n", flush=True)
        if kokoro:
            import soundfile as sf
            samples, rate = kokoro.create(text, voice=voice, speed=1.0, lang="en-us")
            with tempfile.NamedTemporaryFile(suffix=".wav") as f:
                sf.write(f.name, samples, rate)
                subprocess.run(["pw-play", f.name], check=False)
    return 0


if __name__ == "__main__":
    sys.exit(main())
