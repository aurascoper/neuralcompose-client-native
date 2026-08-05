#!/usr/bin/env python3
"""Continuous spoken conversation with memory: talk, it listens, answers, remembers.

Unlike turn.sh (one stateless exchange) and dialectic.py (two models arguing while
you listen), this is you and the model, back and forth, with history in-session and
durable memory across sessions via neural-memory.

The mic stays open. It detects when you start and stop speaking rather than making
you press Enter — the noise floor is calibrated at startup because this machine's
ALC245 runs about 40 dB hot and a fixed threshold would either never trigger or
never stop.

NOT CLAIMED: promotes no support-matrix row, links nothing into the product, and is
not evidence any backend works. Links onnxruntime for Kokoro; the product runtime
decision is deferred and unmade. See README.md.

MEMORY IS agentInference, ALWAYS. neural-memory clamps evidenceClass in two
independent layers — nothing spoken here can become `observed` or a decision. It
writes to a SEPARATE store (voice.db) so conversational material never mixes with
the curated evidence corpus.

Usage:  converse.py [--turns N] [--silent] [--no-memory] [--db PATH]
"""
import json
import os
import subprocess
import sys
import tempfile
import urllib.request
from array import array
from datetime import datetime, timezone
from pathlib import Path

from speak import MODEL, VOICES

HERE = Path(__file__).resolve().parent
SERVER = os.environ.get("SERVER", "http://127.0.0.1:8080")
WHISPER = Path(os.environ.get("WHISPER", Path.home() / "src/whisper.cpp/build/bin/whisper-cli"))
WMODEL = Path(os.environ.get("WMODEL", Path.home() / "src/whisper.cpp/models/ggml-base.en.bin"))
NM_BIN = Path(os.environ.get(
    "NEURAL_MEMORY_BIN", Path.home() / "src/neural-memory-server/target/release/neural-memory-mcp"))
EMBED_URL = os.environ.get("EMBED_URL", "http://127.0.0.1:8082")
EMBED_PROFILE = os.environ.get(
    "EMBED_PROFILE", "fc7d829d6242fccfa0f024bd5175ee611bdcd920f08ff7fcc60db8b59d4eb359")

RATE = 16000
FRAME = 1600                 # 100 ms of s16 mono
SILENCE_END_S = 1.2          # silence this long after speech ends the utterance
MAX_UTTERANCE_S = 30.0       # hard cap
LISTEN_TIMEOUT_S = 15.0      # no speech for this long -> keep waiting, say so once
SPEECH_FACTOR = 2.0          # speech is this many x the calibrated noise floor
FLOOR_MIN = 120              # never trust a floor below this (silent room -> hair trigger)
GATE_MAX = 6000              # a gate above this is unreachable by normal speech
ONSET_FRAMES = 3             # consecutive frames over the gate before we call it speech
CALIBRATE_S = 3.0
CALIBRATE_PCTL = 0.20        # low percentile, NOT median — see calibrate()

SYSTEM = (
    "You are in a spoken conversation. Your reply is READ ALOUD, so use plain "
    "conversational prose: no markdown, no asterisks, no headings, no lists, no "
    "emoji. Three to five sentences — enough to say something substantive, short "
    "enough to listen to. Speak naturally, as a person would. If the transcript "
    "looks garbled, say what you think was meant rather than complaining about it."
)


def now_iso() -> str:
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def rms(buf: bytes) -> float:
    a = array("h")
    a.frombytes(buf)
    if not a:
        return 0.0
    return (sum(x * x for x in a) / len(a)) ** 0.5


class Mic:
    """Streams raw s16 from arecord and cuts utterances on silence.

    arecord -t raw is used rather than pw-record because pw-record writes a WAV
    header to stdout and we want a bare sample stream. It still goes through
    PipeWire via the ALSA compat layer.
    """

    def __init__(self) -> None:
        self.proc = subprocess.Popen(
            ["arecord", "-q", "-f", "S16_LE", "-r", str(RATE), "-c", "1", "-t", "raw", "-"],
            stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)

    def _frame(self) -> bytes:
        return self.proc.stdout.read(FRAME * 2)

    def calibrate(self, seconds: float = CALIBRATE_S) -> float:
        """Low percentile, not median, and capped.

        This mic emits frequent transient spikes to full scale — measured over
        3 s: median 1801, max 32768. A 1 s MEDIAN caught enough spikes to report
        a floor of 6262, which put the gate at 18785 (57% of full scale) and made
        it unreachable by ordinary speech: the loop listened and never answered.
        A low percentile ignores the spikes, and GATE_MAX is the backstop for when
        even that is fooled.
        """
        levels = sorted(rms(self._frame()) for _ in range(int(seconds * RATE / FRAME)))
        floor = max(levels[int(len(levels) * CALIBRATE_PCTL)], FLOOR_MIN)
        return min(floor * SPEECH_FACTOR, GATE_MAX)

    def meter(self, gate: float) -> None:
        """Live level display. Speak and watch: if the bar never passes the gate
        marker, the gate is wrong — pass --threshold with a number that works."""
        print(f"gate = {gate:.0f}. Speak; Ctrl-C to stop.\n", file=sys.stderr)
        while True:
            f = self._frame()
            if not f:
                return
            level = rms(f)
            width = min(int(level / 400), 70)
            mark = int(gate / 400)
            bar = "".join("|" if i == mark else ("#" if i < width else " ")
                          for i in range(max(width, mark) + 1))
            print(f"{level:7.0f} {'OVER' if level > gate else '    '} {bar}",
                  file=sys.stderr, flush=True)

    def utterance(self, gate: float) -> bytes | None:
        """Block until speech starts, then return it once the speaker stops.

        Onset needs ONSET_FRAMES consecutive frames over the gate. A single frame
        is not enough on this mic: its full-scale transients would trigger an
        utterance made entirely of a click.
        """
        frames: list[bytes] = []
        pending: list[bytes] = []
        silent_for = 0.0
        waited = 0.0
        announced = False
        while True:
            f = self._frame()
            if not f:
                return None
            level = rms(f)
            if not frames:
                if level > gate:
                    pending.append(f)
                    if len(pending) >= ONSET_FRAMES:
                        frames, pending = pending, []
                else:
                    pending.clear()
                    waited += FRAME / RATE
                    if waited > LISTEN_TIMEOUT_S and not announced:
                        print("(listening…)", file=sys.stderr, flush=True)
                        announced = True
                continue
            frames.append(f)
            silent_for = silent_for + FRAME / RATE if level <= gate else 0.0
            if silent_for >= SILENCE_END_S or len(frames) * FRAME / RATE >= MAX_UTTERANCE_S:
                return b"".join(frames)

    def close(self) -> None:
        self.proc.terminate()
        self.proc.wait()


def transcribe(pcm: bytes) -> str:
    import wave
    with tempfile.TemporaryDirectory() as d:
        wav = Path(d) / "u.wav"
        with wave.open(str(wav), "wb") as w:
            w.setnchannels(1)
            w.setsampwidth(2)
            w.setframerate(RATE)
            w.writeframes(pcm)
        out = subprocess.run([str(WHISPER), "-m", str(WMODEL), "-f", str(wav), "-nt", "-np"],
                             capture_output=True, text=True).stdout
    return " ".join(out.split())


def chat(messages: list[dict], max_tokens: int = 220, temperature: float = 0.7) -> str:
    body = json.dumps({"messages": messages, "chat_template_kwargs": {"enable_thinking": False},
                       "max_tokens": max_tokens, "temperature": temperature,
                       "stream": False}).encode()
    req = urllib.request.Request(f"{SERVER}/v1/chat/completions", data=body,
                                 headers={"content-type": "application/json"})
    with urllib.request.urlopen(req, timeout=180) as r:
        text = json.load(r)["choices"][0]["message"]["content"]
    for bad in ("**", "*", "#", ">"):
        text = text.replace(bad, "")
    return " ".join(text.split())


class Memory:
    """neural-memory over stdio JSON-RPC. No HTTP: the server has no listener.

    Two traps the wire format sets, both handled here:
      - results are DOUBLE ENCODED — content[0].text is a JSON *string*
      - tool failures arrive as isError:true, not as a JSON-RPC error
    """

    def __init__(self, db: Path, embed: bool = True) -> None:
        args = [str(NM_BIN), "--db", str(db), "--as-of", now_iso()]
        if embed:
            args += ["--embed-url", EMBED_URL, "--embed-profile", EMBED_PROFILE]
        self.proc = subprocess.Popen(args, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                     stderr=subprocess.DEVNULL, text=True, bufsize=1)
        self._id = 0
        self._rpc("initialize", {"protocolVersion": "2025-06-18", "capabilities": {},
                                 "clientInfo": {"name": "spoken-loop", "version": "0"}})

    def _rpc(self, method: str, params: dict) -> dict:
        self._id += 1
        self.proc.stdin.write(json.dumps(
            {"jsonrpc": "2.0", "id": self._id, "method": method, "params": params}) + "\n")
        self.proc.stdin.flush()
        return json.loads(self.proc.stdout.readline())

    def _tool(self, name: str, args: dict):
        resp = self._rpc("tools/call", {"name": name, "arguments": args})
        result = resp.get("result", {})
        text = result.get("content", [{}])[0].get("text", "")
        if result.get("isError"):
            print(f"(memory: {name} failed: {text})", file=sys.stderr)
            return None
        try:
            return json.loads(text)
        except json.JSONDecodeError:
            return text

    def recall(self, query: str, limit: int = 5):
        # asOf is passed per call: --as-of is frozen at launch, so a long
        # conversation would otherwise score recency against its start time.
        return self._tool("recall", {"query": query, "limit": limit, "asOf": now_iso()})

    def remember(self, claim: str, heard: str = "") -> None:
        """sourceLocator carries the VERBATIM transcript, not just a timestamp.

        Measured failure rate on the first real session: 7 of 8 stored claims were
        wrong, and the commonest cause was a confident mis-transcription laundered
        into an assertion ("neural-memory server" -> "narrow memory server").
        A reject list cannot catch that — the claim is not hedged, it is just
        false. What it CAN do is stay traceable: with the utterance in the
        locator, a later reader sees the words that produced the claim and can
        spot the error instead of inheriting it.
        """
        loc = f"spoken-loop/converse/{now_iso()} heard={heard!r}" if heard else \
              f"spoken-loop/converse/{now_iso()}"
        self._tool("remember", {"claim": claim, "occurredAt": now_iso(),
                                "sourceLocator": loc})

    def close(self) -> None:
        try:
            self.proc.stdin.close()
        finally:
            self.proc.terminate()
            self.proc.wait()

    @staticmethod
    def backfill(db: Path) -> None:
        """`remember` does NOT write vectors — verified: 2 memories, 0 embeddings.

        The semantic branch reports ran:true and then searches nothing, so a
        session's own memories are lexical-only until this runs. FTS is a pure OR
        of tokens, so "beverage preferences" does not match "drinks his coffee
        black" — exactly the miss embeddings exist to catch. Backfilling at exit
        makes this session's memories semantically findable by the next one.
        """
        embed_bin = NM_BIN.parent / "neural-memory-embed"
        if not embed_bin.exists():
            return
        r = subprocess.run(
            [str(embed_bin), "--db", str(db), "--url", EMBED_URL,
             "--model-sha256", "3e24342164b3d94991ba9692fdc0dd08e3fd7362e0aacc396a9a5c54a544c3b7",
             "--revision", "v1.5", "--dims", "768"],
            capture_output=True, text=True)
        tail = (r.stdout or r.stderr).strip().splitlines()
        if tail:
            print(f"  (embedded: {tail[-1]})", file=sys.stderr)


DISTIL = (
    "Below is ONE thing the user said, transcribed from speech. Extract a single "
    "durable fact about the user or their work that would be worth recalling in a "
    "future conversation — a preference, a decision, a constraint, a project fact. "
    "Write it as one standalone sentence.\n\n"
    "Reply with exactly NONE if: there is nothing durable (small talk, a question, "
    "a pleasantry); OR the transcript looks garbled or you are unsure what was "
    "said. Do not guess at a garbled word, and do not hedge — a fact you are not "
    "sure of must be NONE, because this is written to permanent memory."
)

# Anything matching these is discarded rather than stored. Every one of these was
# observed in a claim that actually reached the store.
REJECT = (
    "my training", "training cutoff", "i am not", "i'm not", "i don't", "as an ai",
    "may be", "might be", "possibly", "perhaps", "unclear", "not sure", "appears to",
    "or a custom", "beyond my", "i may", "assistant",
)


def durable_fact(heard: str) -> str | None:
    """Distil one storable fact from what the USER said, or None.

    Deliberately NOT given the assistant's reply. Records like "…which is beyond
    my training cutoff" and "…which may be a future version or a custom build"
    reached the store because the model's own hedging was fed in and came back
    attributed to the user. A fact about the user can only come from the user.

    The distiller is also nondeterministic — the same shape of input returns NONE
    on one run and a hedged invention on the next — so the reject list is a second
    gate rather than a belt-and-braces nicety.
    """
    if len(heard.split()) < 4:
        return None
    fact = chat([{"role": "system", "content": DISTIL},
                 {"role": "user", "content": heard}], max_tokens=80, temperature=0.2)
    if not fact or fact.strip().upper().startswith("NONE") or len(fact) < 16:
        return None
    low = fact.lower()
    if any(bad in low for bad in REJECT):
        return None
    return fact


def main() -> int:
    import argparse
    ap = argparse.ArgumentParser(description="Spoken conversation with memory.")
    ap.add_argument("--turns", type=int, default=0, help="stop after N exchanges (0 = until Ctrl-C)")
    ap.add_argument("--silent", action="store_true", help="print only, do not speak")
    ap.add_argument("--no-memory", action="store_true", help="skip neural-memory entirely")
    ap.add_argument("--db", type=Path,
                    default=Path.home() / ".local/share/neural-memory/voice.db")
    ap.add_argument("--voice", default=os.environ.get("KOKORO_VOICE", "af_heart"))
    ap.add_argument("--threshold", type=float,
                    help="override the speech gate (skip calibration)")
    ap.add_argument("--no-log", action="store_true",
                    help="do not append the transcript to converse-transcript.jsonl")
    ap.add_argument("--meter", action="store_true",
                    help="show live mic levels against the gate and exit")
    opt = ap.parse_args()

    if opt.meter:
        mic = Mic()
        gate = opt.threshold if opt.threshold else mic.calibrate()
        try:
            mic.meter(gate)
        except KeyboardInterrupt:
            pass
        finally:
            mic.close()
        return 0

    for p in (WHISPER, WMODEL):
        if not p.exists():
            sys.exit(f"converse: missing {p}")

    kokoro = None
    if not opt.silent:
        from kokoro_onnx import Kokoro
        kokoro = Kokoro(str(MODEL), str(VOICES))   # once, not per turn

    mem = None if opt.no_memory else Memory(opt.db)
    log = None if opt.no_log else open(
        opt.db.parent / "converse-transcript.jsonl", "a", encoding="utf-8")
    mic = Mic()
    gate = opt.threshold if opt.threshold else mic.calibrate()
    print(f"speech gate {gate:.0f} of 32767. Talk when ready; Ctrl-C to stop.\n"
          f"(if it only ever listens, run --meter and pass --threshold)\n", flush=True)

    history: list[dict] = []
    turn = 0
    try:
        while not opt.turns or turn < opt.turns:
            pcm = mic.utterance(gate)
            if pcm is None:
                break
            heard = transcribe(pcm)
            if not heard or heard.strip("[](). ") == "":
                continue          # whisper's bracketed non-speech, e.g. [BLANK_AUDIO]
            print(f"you: {heard}", flush=True)

            system = SYSTEM
            if mem:
                r = mem.recall(heard, limit=3) or {}
                hits = [h["claim"] for h in r.get("hits", [])][:3]
                if hits:
                    system += "\n\nThings you remember from earlier conversations:\n" + \
                              "\n".join(f"- {h}" for h in hits)
                    print(f"  (recalled {len(hits)})", file=sys.stderr, flush=True)

            history.append({"role": "user", "content": heard})
            reply = chat([{"role": "system", "content": system}] + history[-12:])
            history.append({"role": "assistant", "content": reply})
            print(f"model: {reply}\n", flush=True)

            if kokoro:
                import soundfile as sf
                samples, rate = kokoro.create(reply, voice=opt.voice, speed=1.0, lang="en-us")
                with tempfile.NamedTemporaryFile(suffix=".wav") as f:
                    sf.write(f.name, samples, rate)
                    subprocess.run(["pw-play", f.name], check=False)

            fact = durable_fact(heard) if mem else None
            if fact:
                mem.remember(fact, heard)
                print(f"  (remembered: {fact})", file=sys.stderr, flush=True)
            if log:
                log.write(json.dumps({"at": now_iso(), "heard": heard, "reply": reply,
                                      "remembered": fact}) + "\n")
                log.flush()
            turn += 1
    except KeyboardInterrupt:
        print("\nbye", flush=True)
    finally:
        mic.close()
        if log:
            log.close()
        if mem:
            mem.close()
            Memory.backfill(opt.db)   # vectors are written here, not by remember()
    return 0


if __name__ == "__main__":
    sys.exit(main())
