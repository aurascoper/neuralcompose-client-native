#!/usr/bin/env python3
"""Deterministic fixture EEG stream on the frozen /api/eeg/stream contract.

STANDARD LIBRARY ONLY — no brainflow, no websockets, no pip. That is the point:
ADR-002 defines `RuntimeSmokeValidated` as "a deterministic fixture model
executed successfully", and evidence for that rung must not depend on hardware
being present or on packages being installable on the machine producing it.

It speaks the same contract `tools/muse-ble-bridge` serves — an array of samples
per frame, `timestamp` in SECONDS SINCE STREAM START (never wall clock), exactly
four microvolt channels in fixed TP9/AF7/AF8/TP10 order — so a shell that works
against this works against the real headband without changing a line.

DETERMINISTIC BY CONSTRUCTION. Samples are a fixed sum of sinusoids seeded per
channel, not random, so two runs at the same `--seconds` produce identical bytes
and a regression is visible as a diff rather than a vibe.

NOT a Muse simulator and not a physiological model. It is a wire-contract
exerciser. It carries no alpha rhythm, no artifacts, and no claim about EEG.

    python3 tools/fixture-eeg-server/server.py --seconds 5
"""

from __future__ import annotations

import argparse
import base64
import hashlib
import json
import math
import socket
import struct
import sys
import time

GUID = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11"  # RFC 6455
CHANNELS = ("TP9", "AF7", "AF8", "TP10")
SAMPLE_RATE = 256.0
BATCH = 8  # matches the BLE bridge, so the client sees the same frame shape


def handshake(conn: socket.socket) -> str | None:
    data = b""
    while b"\r\n\r\n" not in data:
        chunk = conn.recv(4096)
        if not chunk:
            return None
        data += chunk
    head = data.decode("utf-8", "replace")
    path = head.split(" ", 2)[1] if " " in head else "/"
    key = ""
    for line in head.split("\r\n"):
        if line.lower().startswith("sec-websocket-key:"):
            key = line.split(":", 1)[1].strip()
    accept = base64.b64encode(hashlib.sha1((key + GUID).encode()).digest()).decode()
    conn.sendall(
        (
            "HTTP/1.1 101 Switching Protocols\r\n"
            "Upgrade: websocket\r\nConnection: Upgrade\r\n"
            f"Sec-WebSocket-Accept: {accept}\r\n\r\n"
        ).encode()
    )
    return path


def send_text(conn: socket.socket, text: str) -> None:
    payload = text.encode("utf-8")
    header = bytearray([0x81])  # FIN + text opcode
    n = len(payload)
    if n < 126:
        header.append(n)
    elif n < 1 << 16:
        header.append(126)
        header += struct.pack(">H", n)
    else:
        header.append(127)
        header += struct.pack(">Q", n)
    conn.sendall(bytes(header) + payload)


# Fractional amplitude gained by TP10 per second, and the period over which it
# cycles. Chosen so a consumer computing RMS over a 5 s window sees it move by
# well over any sane equality tolerance within a few seconds, while staying
# inside the healthy band (never near the 2 uV dead floor or the 200 uV
# saturation bar) so the drift changes the NUMBER without changing the VERDICT.
DRIFT_DEPTH = 0.35
DRIFT_PERIOD_S = 40.0


def sample(i: int) -> dict:
    """Fixed sinusoid sum — deterministic, finite, and distinct per channel.

    TP10 additionally carries a slow amplitude drift. Every other channel is
    stationary, so a full buffer yields a CONSTANT RMS for them — which is
    correct for a periodic signal and also indistinguishable from a reading that
    is frozen, cached or computed once and reused. With three stationary
    channels and one drifting one, a consumer that reports identical health on
    every turn is now detectably wrong, deterministically and with no hardware.

    The drift is a property of this FIXTURE, not a claim about EEG: it carries
    no physiology, and a real headband's variation comes from contact and
    movement, which this still cannot exercise.
    """
    t = i / SAMPLE_RATE
    drift = 1.0 + DRIFT_DEPTH * math.sin(2 * math.pi * t / DRIFT_PERIOD_S)
    return {
        "timestamp": t,
        "channels": [
            round(
                20.0
                * (drift if CHANNELS[c] == "TP10" else 1.0)
                * math.sin(2 * math.pi * (8 + 2 * c) * t)
                + 5.0 * c,
                6,
            )
            for c in range(len(CHANNELS))
        ],
    }


def self_check() -> int:
    """Assert the drift is actually there, without a client or a socket.

    The drift exists so that a consumer reporting IDENTICAL channel health on
    every turn is detectably wrong. Nothing else guards it: delete the drift and
    every downstream check still passes, quietly losing the only property that
    distinguishes a live reading from a frozen one.
    """
    window = int(SAMPLE_RATE * 5)  # the client keeps 1280 samples

    def rms(channel: int, start: int) -> float:
        vs = [sample(i)["channels"][channel] for i in range(start, start + window)]
        return (sum(v * v for v in vs) / len(vs)) ** 0.5

    ok = True
    quarter = int(SAMPLE_RATE * DRIFT_PERIOD_S / 4)
    for c, name in enumerate(CHANNELS):
        a, b = rms(c, 0), rms(c, quarter)
        moved = abs(a - b)
        drifting = name == "TP10"
        if drifting and moved < 1.0:
            print(f"FAIL {name}: expected drift, RMS moved only {moved:.4f} uV")
            ok = False
        if not drifting and moved > 0.01:
            print(f"FAIL {name}: expected stationary, RMS moved {moved:.4f} uV")
            ok = False
        # Drift must change the NUMBER, never the VERDICT: stay clear of the
        # 2 uV dead floor and the 200 uV saturation bar the client classifies on.
        if not (2.0 < min(a, b) and max(a, b) < 200.0):
            print(f"FAIL {name}: RMS {a:.2f}/{b:.2f} uV leaves the healthy band")
            ok = False
        print(f"{'drift' if drifting else 'fixed'} {name}: {a:.4f} -> {b:.4f} uV")
    print("self-check: " + ("ok" if ok else "FAILED"))
    return 0 if ok else 1


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument(
        "--self-check",
        action="store_true",
        help="verify the drift property and exit; no socket is opened",
    )
    ap.add_argument("--host", default="127.0.0.1")
    ap.add_argument("--port", type=int, default=8788)
    ap.add_argument("--seconds", type=float, default=5.0)
    ap.add_argument("--path", default="/api/eeg/stream")
    args = ap.parse_args()
    if args.self_check:
        return self_check()

    srv = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    srv.bind((args.host, args.port))
    srv.listen(1)
    print(f"fixture server ws://{args.host}:{args.port}{args.path} "
          f"({SAMPLE_RATE:.0f} Hz, batch {BATCH}, {args.seconds}s)", file=sys.stderr)

    conn, _ = srv.accept()
    path = handshake(conn)
    if path is None:
        return 1
    if path != args.path:
        # Same rule the BLE bridge enforces: a wrong path is closed, not served.
        conn.close()
        print(f"refused path {path!r}", file=sys.stderr)
        return 1

    i = 0
    t0 = time.monotonic()
    try:
        while time.monotonic() - t0 < args.seconds:
            batch = [sample(i + k) for k in range(BATCH)]
            send_text(conn, json.dumps(batch))
            i += BATCH
            time.sleep(BATCH / SAMPLE_RATE)
    except (BrokenPipeError, ConnectionResetError):
        pass
    finally:
        try:
            conn.close()
        finally:
            srv.close()
    print(f"sent {i} samples", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
