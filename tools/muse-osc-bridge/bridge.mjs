// Mind Monitor OSC (UDP) → /api/eeg/stream WebSocket.
//
// The golden-capture gate assumes "Muse → bridge → /api/eeg/stream → app",
// but nothing in this repo actually spoke the first hop: the macOS app
// consumes Mind Monitor OSC internally and never re-exports it. This is that
// missing hop, and nothing more.
//
// It is a DEVELOPMENT TOOL. It binds all interfaces so a phone on the same
// LAN can reach it, which also means anyone on that LAN can read the stream —
// run it on a trusted network only. It never writes to disk, so raw EEG
// leaves no trace here; persistence is the client's job.
//
//   node bridge.mjs                     # OSC :5000 → ws :8788
//   MUSE_OSC_PORT=5001 WS_PORT=9000 node bridge.mjs
//
// Mind Monitor: Settings → OSC Stream Target = this machine's LAN IP, port
// 5000. The app must then point at ws://<this machine>:8788/api/eeg/stream.

import dgram from 'node:dgram';
import http from 'node:http';
import { WebSocketServer } from 'ws';

const OSC_PORT = Number(process.env.MUSE_OSC_PORT ?? 5000);
const WS_PORT = Number(process.env.WS_PORT ?? 8788);
const EEG_ADDRESS = '/muse/eeg';
// Batch size matches the Gate 4 stub so the client sees the same shape from
// either source — the contract permits a single sample or an array.
const BATCH = 8;

// ---- OSC decoding (only what Mind Monitor actually sends) ----

function readPaddedString(buf, offset) {
  let end = offset;
  while (end < buf.length && buf[end] !== 0) end++;
  if (end >= buf.length) return null;
  const value = buf.toString('ascii', offset, end);
  return { value, next: offset + Math.ceil((end - offset + 1) / 4) * 4 };
}

/** Returns an array of float args for `/muse/eeg`, or null for anything else. */
function decodeEegMessage(buf) {
  const addr = readPaddedString(buf, 0);
  if (!addr || addr.value !== EEG_ADDRESS) return null;
  const tags = readPaddedString(buf, addr.next);
  if (!tags || !tags.value.startsWith(',')) return null;
  let cursor = tags.next;
  const floats = [];
  for (const tag of tags.value.slice(1)) {
    if (tag === 'f') {
      if (cursor + 4 > buf.length) return null;
      floats.push(buf.readFloatBE(cursor));
      cursor += 4;
    } else if (tag === 'i') {
      cursor += 4;
    } else if (tag === 's') {
      const s = readPaddedString(buf, cursor);
      if (!s) return null;
      cursor = s.next;
    } else if (tag === 'd') {
      cursor += 8;
    }
    // Unknown tags: stop rather than guess at alignment.
    else return floats.length ? floats : null;
  }
  return floats;
}

/** Mind Monitor may send bundles; walk them rather than dropping the packet. */
function* eachMessage(buf) {
  if (buf.length >= 8 && buf.toString('ascii', 0, 7) === '#bundle') {
    let cursor = 16; // '#bundle\0' + 8-byte timetag
    while (cursor + 4 <= buf.length) {
      const size = buf.readInt32BE(cursor);
      cursor += 4;
      if (size <= 0 || cursor + size > buf.length) return;
      yield* eachMessage(buf.subarray(cursor, cursor + size));
      cursor += size;
    }
    return;
  }
  yield buf;
}

// ---- state ----

let firstSampleAtNs = null;
let sampleCount = 0;
let droppedCount = 0;
let lastEegAtMs = 0;
let pending = [];
const clients = new Set();

function broadcast(text) {
  for (const ws of clients) {
    if (ws.readyState === 1) ws.send(text);
  }
}

const socket = dgram.createSocket('udp4');

socket.on('message', (buf) => {
  for (const message of eachMessage(buf)) {
    const floats = decodeEegMessage(message);
    if (!floats) continue; // /muse/acc, /muse/gyro, /muse/batt, ... — not EEG
    if (floats.length < 4) {
      // Truncated EEG is corruption, not benign traffic: count it, never
      // pad it into a sample that looks real.
      droppedCount++;
      continue;
    }
    const channels = floats.slice(0, 4);
    if (!channels.every(Number.isFinite)) {
      droppedCount++;
      continue;
    }
    const nowNs = process.hrtime.bigint();
    if (firstSampleAtNs === null) firstSampleAtNs = nowNs;
    // Seconds since STREAM START — the wire contract's axis, never wall
    // clock. Mind Monitor carries no per-sample timestamp, so this is an
    // arrival time and is honest about being one.
    const timestamp = Number(nowNs - firstSampleAtNs) / 1e9;
    sampleCount++;
    lastEegAtMs = Date.now();
    pending.push({ timestamp, channels });
    if (pending.length >= BATCH) {
      broadcast(JSON.stringify(pending));
      pending = [];
    }
  }
});

socket.on('error', (err) => {
  console.error(`OSC socket error: ${err.message}`);
  process.exit(1);
});

socket.bind(OSC_PORT, '0.0.0.0', () => {
  console.log(`OSC listening on UDP 0.0.0.0:${OSC_PORT} (Mind Monitor target)`);
});

// ---- WebSocket ----

const server = http.createServer((req, res) => {
  if (req.url === '/health') {
    const silentMs = lastEegAtMs ? Date.now() - lastEegAtMs : null;
    res.writeHead(200, { 'Content-Type': 'application/json' });
    res.end(
      JSON.stringify({
        oscPort: OSC_PORT,
        clients: clients.size,
        samples: sampleCount,
        dropped: droppedCount,
        // null until the first EEG packet: "no data yet" is not "healthy".
        msSinceLastEeg: silentMs,
      }),
    );
    return;
  }
  res.writeHead(404, { 'Content-Type': 'application/json' });
  res.end(JSON.stringify({ error: 'not found', path: req.url }));
});

const wss = new WebSocketServer({ noServer: true });

server.on('upgrade', (req, sock, head) => {
  if (new URL(req.url, 'http://x').pathname !== '/api/eeg/stream') {
    sock.destroy();
    return;
  }
  wss.handleUpgrade(req, sock, head, (ws) => {
    clients.add(ws);
    console.log(`client connected (${clients.size} total)`);
    ws.on('close', () => {
      clients.delete(ws);
      console.log(`client disconnected (${clients.size} total)`);
    });
    ws.on('error', () => clients.delete(ws));
  });
});

server.listen(WS_PORT, '0.0.0.0', () => {
  console.log(`WebSocket on ws://0.0.0.0:${WS_PORT}/api/eeg/stream`);
  console.log('LAN-visible by design — trusted networks only.');
});

setInterval(() => {
  const silent = lastEegAtMs ? Math.round((Date.now() - lastEegAtMs) / 1000) : null;
  console.log(
    `samples=${sampleCount} dropped=${droppedCount} clients=${clients.size} ` +
      (silent === null ? 'no EEG yet' : `last EEG ${silent}s ago`),
  );
}, 10_000);
