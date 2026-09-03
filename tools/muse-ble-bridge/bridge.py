"""Muse S (BLE) -> /api/eeg/stream WebSocket. No phone, no Mind Monitor.

The golden-capture gate assumes "Muse -> bridge -> /api/eeg/stream -> app",
but nothing in this repo spoke that first hop: the macOS app consumes Muse
internally and never re-exports it. This is that hop, taken directly over
Bluetooth via BrainFlow so no second device sits in the path.

DEVELOPMENT TOOL. It binds all interfaces so a phone on the same LAN can
reach it, which also means anyone on that LAN can read the stream — trusted
networks only. It never writes to disk: persistence is the client's job, and
raw EEG leaves no trace here.

    ./.venv/bin/python bridge.py                 # ws://0.0.0.0:8788/api/eeg/stream
    WS_PORT=9000 ./.venv/bin/python bridge.py
    MUSE_MAC=XX:XX:XX:XX:XX:XX ./.venv/bin/python bridge.py

On macOS BLE matches by NAME, not MAC — leave MUSE_MAC unset and BrainFlow
discovers the first Muse S it sees.
"""

import asyncio
import json
import os
import signal
import sys
import time

import websockets
from brainflow.board_shim import BoardIds, BoardShim, BrainFlowInputParams
from brainflow.exit_codes import BrainFlowError

WS_PORT = int(os.environ.get("WS_PORT", "8788"))
BOARD_ID = BoardIds.MUSE_S_BOARD.value
# Matches the Gate 4 stub's batching so the client sees the same shape from
# either source; the frozen contract permits one sample or an array.
BATCH = 8
POLL_SECONDS = 0.02

# The frozen channel order. BrainFlow returns Muse channels in this order for
# MUSE_S_BOARD; we assert the count rather than trusting it silently.
CHANNEL_ORDER = ["TP9", "AF7", "AF8", "TP10"]

clients: set = set()
state = {"samples": 0, "dropped": 0, "first_ns": None, "last_eeg": 0.0}


async def register(ws):
    clients.add(ws)
    print(f"client connected ({len(clients)} total)", flush=True)
    try:
        await ws.wait_closed()
    finally:
        clients.discard(ws)
        print(f"client disconnected ({len(clients)} total)", flush=True)


async def handler(ws):
    if ws.request.path != "/api/eeg/stream":
        await ws.close(code=1008, reason="unknown path")
        return
    await register(ws)


async def broadcast(text: str) -> None:
    if not clients:
        return
    await asyncio.gather(
        *(c.send(text) for c in list(clients)), return_exceptions=True
    )


async def pump(board: BoardShim) -> None:
    """Drain BrainFlow's ring buffer and publish contract-shaped frames."""
    eeg_rows = BoardShim.get_eeg_channels(BOARD_ID)
    if len(eeg_rows) < 4:
        raise SystemExit(
            f"expected >=4 EEG channels for Muse S, got {len(eeg_rows)}"
        )
    eeg_rows = eeg_rows[:4]
    pending = []
    while True:
        await asyncio.sleep(POLL_SECONDS)
        data = board.get_board_data()
        if data.shape[1] == 0:
            continue
        for i in range(data.shape[1]):
            channels = [float(data[r][i]) for r in eeg_rows]
            # A non-finite channel is corruption, never padded into a sample
            # that looks real.
            if not all(v == v and abs(v) != float("inf") for v in channels):
                state["dropped"] += 1
                continue
            now_ns = time.monotonic_ns()
            if state["first_ns"] is None:
                state["first_ns"] = now_ns
            # Seconds since STREAM START — the wire contract's axis, never
            # wall clock.
            timestamp = (now_ns - state["first_ns"]) / 1e9
            state["samples"] += 1
            state["last_eeg"] = time.time()
            pending.append({"timestamp": timestamp, "channels": channels})
            if len(pending) >= BATCH:
                await broadcast(json.dumps(pending))
                pending = []


async def report() -> None:
    while True:
        await asyncio.sleep(10)
        silent = (
            None if not state["last_eeg"] else round(time.time() - state["last_eeg"])
        )
        # "No data yet" is not "healthy" — say which it is.
        tail = "no EEG yet" if silent is None else f"last EEG {silent}s ago"
        print(
            f"samples={state['samples']} dropped={state['dropped']} "
            f"clients={len(clients)} {tail}",
            flush=True,
        )


async def main() -> None:
    BoardShim.disable_board_logger()
    params = BrainFlowInputParams()
    mac = os.environ.get("MUSE_MAC")
    if mac:
        params.mac_address = mac
    board = BoardShim(BOARD_ID, params)

    print("searching for a Muse S over Bluetooth (headband must be ON)…", flush=True)
    # One BrainFlow scan gives up after ~20s, which is a narrow window for a
    # human to wake a dozing headband. Retry a few times so "press the button
    # after launching" actually works; each attempt is its own full scan.
    attempts = int(os.environ.get("MUSE_ATTEMPTS", "4"))
    for attempt in range(1, attempts + 1):
        try:
            board.prepare_session()
            break
        except BrainFlowError as exc:
            if attempt < attempts:
                print(
                    f"attempt {attempt}/{attempts} failed ({exc}); retrying — "
                    "press the Muse button if it is dozing",
                    flush=True,
                )
                time.sleep(2)
                continue
            print(f"could not connect to the Muse: {exc}", file=sys.stderr)
            print(
                "checks: headband powered on and not paired to another app "
                "(Mind Monitor / phone), macOS Bluetooth permission granted to "
                "this terminal, headband not already streaming elsewhere.",
                file=sys.stderr,
            )
            raise SystemExit(2)

    board.start_stream()
    rate = BoardShim.get_sampling_rate(BOARD_ID)
    print(f"Muse connected — {rate} Hz, channels {CHANNEL_ORDER}", flush=True)

    stop = asyncio.Event()
    loop = asyncio.get_running_loop()
    for sig in (signal.SIGINT, signal.SIGTERM):
        loop.add_signal_handler(sig, stop.set)

    async with websockets.serve(handler, "0.0.0.0", WS_PORT):
        print(
            f"WebSocket on ws://0.0.0.0:{WS_PORT}/api/eeg/stream "
            "(LAN-visible by design — trusted networks only)",
            flush=True,
        )
        pump_task = asyncio.create_task(pump(board))
        report_task = asyncio.create_task(report())
        await stop.wait()
        pump_task.cancel()
        report_task.cancel()

    print("stopping…", flush=True)
    try:
        board.stop_stream()
        board.release_session()
    except BrainFlowError:
        pass


if __name__ == "__main__":
    asyncio.run(main())
