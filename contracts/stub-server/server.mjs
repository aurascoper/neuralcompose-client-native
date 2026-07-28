import http from 'node:http';
import { URL } from 'node:url';
import { WebSocketServer, WebSocket } from 'ws';

const HOST = '0.0.0.0';
const PORT = 8787;
const CHANNELS = ['TP9', 'AF7', 'AF8', 'TP10'];

let streaming = true;
let acceptingStreams = true;
let sampleIndex = 0;
let packetsReceived = 0;
let heartbeatAt = Date.now();

const wss = new WebSocketServer({ noServer: true });

function reply(res, body, status = 200) {
  res.writeHead(status, {
    'Content-Type': 'application/json',
    'Access-Control-Allow-Origin': '*',
    'Access-Control-Allow-Methods': 'GET,POST,OPTIONS',
  });
  res.end(JSON.stringify(body));
}

function state() {
  return {
    streaming,
    acceptingStreams,
    websocketClients: wss.clients.size,
    packetsReceived,
    lastHeartbeat: new Date(heartbeatAt).toISOString(),
  };
}

const server = http.createServer((req, res) => {
  const url = new URL(req.url ?? '/', `http://${req.headers.host ?? `127.0.0.1:${PORT}`}`);

  if (req.method === 'OPTIONS') return reply(res, { ok: true });

  if (url.pathname === '/api/diagnostics') {
    return reply(res, {
      transport: 'Synthetic',
      sampleRate: 256,
      packetsReceived,
      packetsDropped: 0,
      packetLossEstimate: 0,
      packetJitterMillis: 0.2,
      lastInterArrivalMillis: 3.90625,
      lastHeartbeat: new Date(heartbeatAt).toISOString(),
      boundPort: PORT,
      localInterfaceName: 'gate4-stub',
    });
  }

  if (url.pathname === '/api/health') {
    const wallClock = heartbeatAt / 1000;
    return reply(res, CHANNELS.map((channel, i) => ({
      channel,
      status: 'healthy',
      rms: 35 + i * 4,
      samples: packetsReceived,
      timestamp: sampleIndex / 256,
      lastSampleWallClock: wallClock,
    })));
  }

  if (url.pathname === '/api/classifier') {
    return reply(res, {
      intent: 'rest',
      confidence: 0.91,
      distribution: {
        rest: 0.91,
        jawClench: 0.03,
        singleBlink: 0.02,
        doubleBlink: 0.02,
        select: 0.02,
      },
      windowSequence: Math.floor(sampleIndex / 256),
      endTimestamp: sampleIndex / 256,
    });
  }

  if (url.pathname === '/api/pipeline-mode') {
    return reply(res, {
      source: 'synthetic',
      sourceProfile: 'Gate 4 reconnect fixture',
      classifier: 'mock',
      predictor: 'stub',
      transportDetail: `WS ${PORT} · localhost`,
      isFullyLive: false,
      substitutionSummary: 'Synthetic transport fixture; no inference performed',
    });
  }

  if (url.pathname === '/control/pause') {
    streaming = false;
    console.log('PAUSED: socket stays open, heartbeat and samples stop');
    return reply(res, state());
  }

  if (url.pathname === '/control/resume') {
    streaming = true;
    acceptingStreams = true;
    heartbeatAt = Date.now();
    console.log('RESUMED');
    return reply(res, state());
  }

  if (url.pathname === '/control/drop') {
    const duration = Math.max(500, Number(url.searchParams.get('ms') ?? 2500));
    streaming = false;
    acceptingStreams = false;

    for (const client of wss.clients) {
      client.close(1012, 'gate4 temporary outage');
    }

    console.log(`DROPPED: rejecting streams for ${duration} ms`);

    setTimeout(() => {
      acceptingStreams = true;
      streaming = true;
      heartbeatAt = Date.now();
      console.log('RECOVERED: accepting WebSocket connections');
    }, duration);

    return reply(res, { ...state(), recoveryInMs: duration });
  }

  if (url.pathname === '/control/state') {
    return reply(res, state());
  }

  return reply(res, { error: 'not found', path: url.pathname }, 404);
});

server.on('upgrade', (req, socket, head) => {
  const url = new URL(req.url ?? '/', `http://${req.headers.host ?? `127.0.0.1:${PORT}`}`);

  if (url.pathname !== '/api/eeg/stream') {
    socket.destroy();
    return;
  }

  wss.handleUpgrade(req, socket, head, ws => {
    wss.emit('connection', ws);
  });
});

wss.on('connection', ws => {
  if (!acceptingStreams) {
    ws.close(1013, 'temporarily unavailable');
    return;
  }

  console.log(`WebSocket open (${wss.clients.size} client(s))`);
  ws.on('close', () => console.log(`WebSocket closed (${wss.clients.size} client(s))`));
});

// Eight-sample batches every 31.25 ms = 256 samples/second.
// The client explicitly accepts both individual and batched frames.
setInterval(() => {
  if (!streaming || !acceptingStreams) return;

  const batch = Array.from({ length: 8 }, (_, offset) => {
    const n = sampleIndex + offset;
    const t = n / 256;

    return {
      timestamp: t,
      channels: [
        45 * Math.sin(2 * Math.PI * 8 * t),
        32 * Math.sin(2 * Math.PI * 10 * t + 0.5),
        36 * Math.sin(2 * Math.PI * 12 * t + 1.0),
        42 * Math.sin(2 * Math.PI * 6 * t + 1.5),
      ],
    };
  });

  sampleIndex += batch.length;
  packetsReceived += batch.length;
  heartbeatAt = Date.now();

  const payload = JSON.stringify(batch);
  for (const client of wss.clients) {
    if (client.readyState === WebSocket.OPEN) client.send(payload);
  }
}, 31.25);

server.listen(PORT, HOST, () => {
  console.log(`Gate 4 stub: http://127.0.0.1:${PORT}`);
  console.log(`EEG stream:  ws://127.0.0.1:${PORT}/api/eeg/stream`);
  console.log('Controls: /control/pause /resume /drop?ms=2500 /state');
});
