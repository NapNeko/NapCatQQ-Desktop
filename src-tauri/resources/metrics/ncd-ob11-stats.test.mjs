// 线级回归：探针只能挂 node 核心模块（net/http），不能依赖 NC/SL 的 ws 实现。
// 跑：node --test src-tauri/resources/metrics/
import { test, before, after } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import http from 'node:http';
import net from 'node:net';
import { spawnSync } from 'node:child_process';
import { createRequire } from 'node:module';
import { WebSocketServer, WebSocket } from 'ws';

const require = createRequire(import.meta.url);

const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'ncd-probe-'));
const outPath = path.join(tmpDir, 'net-stats.json');
const nodesPath = path.join(tmpDir, 'nodes.json');

/** 端口要在探针加载前定好：nodes.json 只在 require 时读第一份 */
async function freePort() {
    return await new Promise((resolve, reject) => {
        const srv = net.createServer();
        srv.on('error', reject);
        srv.listen(0, '127.0.0.1', () => {
            const port = srv.address().port;
            srv.close(() => resolve(port));
        });
    });
}

const ports = {
    httpServer: await freePort(),
    wsServer: await freePort(),
    sseServer: await freePort(),
    httpPeer: await freePort(),
    wsPeer: await freePort(),
};

fs.writeFileSync(
    nodesPath,
    JSON.stringify([
        { name: 'http-default', kind: 'httpServer', listenPort: ports.httpServer },
        { name: 'ws-default', kind: 'wsServer', listenPort: ports.wsServer },
        { name: 'sse-default', kind: 'httpSse', listenPort: ports.sseServer },
        { name: 'http-post', kind: 'httpClient', targetUrl: `http://127.0.0.1:${ports.httpPeer}/` },
        { name: 'ws-reverse', kind: 'wsClient', targetUrl: `ws://127.0.0.1:${ports.wsPeer}/onebot` },
    ]),
);

process.env.NCD_METRICS_ENABLED = '1';
process.env.NCD_METRICS_OUT = outPath;
process.env.NCD_METRICS_NODES_PATH = nodesPath;
process.env.NCD_METRICS_INTERVAL_MS = '30000';

// 必须先加载探针再建 server/client：真实场景是 preload
const probe = require('./ncd-ob11-stats.cjs');

const EVENT = JSON.stringify({
    post_type: 'message',
    message_type: 'private',
    time: 1,
    self_id: 1,
    user_id: 2,
    raw_message: 'hi',
});
const ACTION = JSON.stringify({ action: 'send_private_msg', params: { user_id: 2, message: 'hi' }, echo: '1' });

/** @type {{ close: () => Promise<void> }[]} */
const closers = [];
/** 服务器 close 会等连接排空，所以客户端必须先于 closers 断开 */
const clients = [];

function trackClient(client) {
    clients.push(client);
    return client;
}

function trackServer(server) {
    closers.push({ close: () => new Promise((resolve) => server.close(() => resolve())) });
    return server;
}

function nodeStats(name) {
    probe.writeSnapshot();
    const snap = JSON.parse(fs.readFileSync(outPath, 'utf8'));
    const found = snap.nodes.find((n) => n.name === name);
    assert.ok(found, `节点 ${name} 不在快照里`);
    return found;
}

async function waitFor(label, predicate, timeoutMs = 4000) {
    const deadline = Date.now() + timeoutMs;
    for (;;) {
        if (predicate()) return;
        if (Date.now() > deadline) assert.fail(`超时未满足：${label}`);
        await new Promise((r) => setTimeout(r, 50));
    }
}

before(async () => {
    // 正向 HTTP（被计数节点）：收 action 回 response
    trackServer(
        http
            .createServer((req, res) => {
                const chunks = [];
                req.on('data', (c) => chunks.push(c));
                req.on('end', () => {
                    res.setHeader('content-type', 'application/json');
                    res.end(JSON.stringify({ status: 'ok', retcode: 0, data: null, echo: '1' }));
                });
            })
            .listen(ports.httpServer, '127.0.0.1'),
    );

    // 正向 WS（被计数节点）：推事件 + 收 action
    const wss = new WebSocketServer({ port: ports.wsServer, host: '127.0.0.1' });
    wss.on('connection', (socket) => {
        socket.on('message', (data) => {
            socket.send(JSON.stringify({ status: 'ok', retcode: 0, echo: JSON.parse(String(data)).echo }));
        });
        socket.send(EVENT);
    });
    closers.push({ close: () => new Promise((resolve) => wss.close(() => resolve())) });

    // SSE 服务（被计数节点）：GET 订阅后连发事件
    trackServer(
        http
            .createServer((req, res) => {
                res.writeHead(200, { 'content-type': 'text/event-stream' });
                res.write(`data: ${EVENT}\n\n`);
                res.write(`data: ${EVENT}\n\n`);
            })
            .listen(ports.sseServer, '127.0.0.1'),
    );

    // 对端 HTTP 上报服务（不在 node map 里，不该被计数）
    trackServer(
        http
            .createServer((req, res) => {
                req.resume();
                req.on('end', () => res.end('{}'));
            })
            .listen(ports.httpPeer, '127.0.0.1'),
    );

    // 对端反向 WS 服务（不在 node map 里）：收事件回 action
    const peerWss = new WebSocketServer({ port: ports.wsPeer, host: '127.0.0.1' });
    peerWss.on('connection', (socket) => {
        socket.on('message', () => socket.send(ACTION));
    });
    closers.push({ close: () => new Promise((resolve) => peerWss.close(() => resolve())) });

    await new Promise((r) => setTimeout(r, 100));
});

after(async () => {
    for (const client of clients) {
        try {
            if (typeof client.terminate === 'function') client.terminate();
            else client.destroy();
        } catch (_) { /* 已断开 */ }
    }
    for (const c of closers) await c.close();
    fs.rmSync(tmpDir, { recursive: true, force: true });
});

test('正向 HTTP 服务节点计入站 action 与双向字节', async () => {
    await new Promise((resolve, reject) => {
        const req = http.request(
            { host: '127.0.0.1', port: ports.httpServer, method: 'POST', path: '/send_private_msg' },
            (res) => {
                res.resume();
                res.on('end', resolve);
            },
        );
        req.on('error', reject);
        req.end(ACTION);
    });

    await waitFor('http-default 收到 action', () => nodeStats('http-default').actions_in >= 1);
    const stats = nodeStats('http-default');
    assert.ok(stats.bytes_in > 0, '入站字节应大于 0');
    assert.ok(stats.bytes_out > 0, '出站字节应大于 0');
    assert.ok(stats.last_activity_at_ms > 0, '应有最近活动时间');
});

test('正向 WS 服务节点分别计出站事件与入站 action', async () => {
    const client = trackClient(new WebSocket(`ws://127.0.0.1:${ports.wsServer}/`));
    const received = [];
    client.on('message', (data) => received.push(String(data)));
    await new Promise((resolve, reject) => {
        client.on('open', resolve);
        client.on('error', reject);
    });
    await waitFor('客户端收到事件', () => received.length >= 1);
    client.send(ACTION);
    await waitFor('客户端收到 action 回执', () => received.length >= 2);

    await waitFor(
        'ws-default 出入站均计数',
        () => nodeStats('ws-default').events_out >= 1 && nodeStats('ws-default').actions_in >= 1,
    );
    const stats = nodeStats('ws-default');
    assert.ok(stats.bytes_out > 0, '出站字节应大于 0');
    assert.ok(stats.bytes_in > 0, '入站字节应大于 0');
    client.close();
});

test('HTTP 上报节点按 targetUrl 计出站事件', async () => {
    await new Promise((resolve, reject) => {
        const req = http.request(
            { host: '127.0.0.1', port: ports.httpPeer, method: 'POST', path: '/' },
            (res) => {
                res.resume();
                res.on('end', resolve);
            },
        );
        req.on('error', reject);
        req.end(EVENT);
    });

    await waitFor('http-post 计到事件', () => nodeStats('http-post').events_out >= 1);
    assert.ok(nodeStats('http-post').bytes_out > 0, '出站字节应大于 0');
});

test('反向 WS 节点按 targetUrl 计出站事件与入站 action', async () => {
    const client = trackClient(new WebSocket(`ws://127.0.0.1:${ports.wsPeer}/onebot`));
    await new Promise((resolve, reject) => {
        client.on('open', resolve);
        client.on('error', reject);
    });
    const back = [];
    client.on('message', (data) => back.push(String(data)));
    client.send(EVENT);
    await waitFor('反向 WS 收到 action', () => back.length >= 1);

    await waitFor(
        'ws-reverse 出入站均计数',
        () => nodeStats('ws-reverse').events_out >= 1 && nodeStats('ws-reverse').actions_in >= 1,
    );
    client.close();
});

// SnowLuma 的反向 WS 不走 http.request，是裸 net.connect + 自己拼握手 + 自己掩码。
// 这条路径只能靠 socket 层观测，覆盖它才算真的和 NC/SL 两家都对上。
test('裸 socket 握手的反向 WS 也能计数', async () => {
    const socket = trackClient(net.connect(ports.wsPeer, '127.0.0.1'));

    const key = Buffer.from('0123456789abcdef').toString('base64');
    let upgraded = false;
    const inbound = [];
    socket.on('data', (chunk) => {
        if (!upgraded) {
            if (chunk.includes('\r\n\r\n')) upgraded = true;
            return;
        }
        inbound.push(chunk);
    });
    await new Promise((resolve, reject) => {
        socket.on('error', reject);
        socket.on('connect', resolve);
    });
    socket.write(
        [
            'GET /onebot HTTP/1.1',
            `Host: 127.0.0.1:${ports.wsPeer}`,
            'Upgrade: websocket',
            'Connection: Upgrade',
            `Sec-WebSocket-Key: ${key}`,
            'Sec-WebSocket-Version: 13',
            '',
            '',
        ].join('\r\n'),
    );
    await waitFor('裸 socket 完成握手', () => upgraded);

    socket.write(maskedTextFrame(EVENT));
    await waitFor('对端回了 action', () => inbound.length >= 1);

    await waitFor(
        'ws-reverse 记到裸 socket 的收发',
        () => nodeStats('ws-reverse').events_out >= 2 && nodeStats('ws-reverse').actions_in >= 2,
    );
});

/** 客户端帧必须带掩码（RFC 6455 §5.3），探针得会解掩码才读得到正文 */
function maskedTextFrame(text) {
    const payload = Buffer.from(text, 'utf8');
    assert.ok(payload.length < 126, '测试载荷保持在短帧长度内');
    const mask = Buffer.from([0x12, 0x34, 0x56, 0x78]);
    const masked = Buffer.from(payload);
    for (let i = 0; i < masked.length; i += 1) masked[i] ^= mask[i & 3];
    return Buffer.concat([Buffer.from([0x81, 0x80 | payload.length]), mask, masked]);
}

// SnowLuma 的 http-post adapter 用全局 fetch（undici），不经 http.request
test('fetch 上报也算进 HTTP 上报节点', async () => {
    const before = nodeStats('http-post').events_out;
    const res = await fetch(`http://127.0.0.1:${ports.httpPeer}/`, {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: EVENT,
    });
    await res.text();

    await waitFor('http-post 计到 fetch 事件', () => nodeStats('http-post').events_out > before);
});

test('SSE 节点按事件标记计数，订阅 GET 不算 action', async () => {
    const res = await new Promise((resolve, reject) => {
        const req = http.request({ host: '127.0.0.1', port: ports.sseServer, method: 'GET', path: '/' }, resolve);
        req.on('error', reject);
        req.end();
    });
    const seen = [];
    res.on('data', (c) => seen.push(c));
    await waitFor('客户端收到 SSE 事件', () => Buffer.concat(seen).includes('post_type'));

    await waitFor('sse-default 计到事件', () => nodeStats('sse-default').events_out >= 2);
    assert.equal(nodeStats('sse-default').actions_in, 0, '订阅流不该被当成 action');
    res.destroy();
});

test('action 回执不算出站事件', () => {
    // ws-default 上服务端只推了 1 条事件，另一条是 action 回执
    assert.equal(nodeStats('ws-default').events_out, 1);
});

test('未配置的连接不落到任意节点', () => {
    probe.writeSnapshot();
    const snap = JSON.parse(fs.readFileSync(outPath, 'utf8'));
    assert.equal(snap.nodes.length, 5, '不应凭空多出节点');
    assert.ok(snap.memory && snap.memory.rssBytes > 0, '应有进程内存快照');
});

test('关指标时探针不落盘也不改核心模块', () => {
    const idlePath = path.join(tmpDir, 'disabled.json');
    const probePath = path.join(import.meta.dirname, 'ncd-ob11-stats.cjs').replace(/\\/g, '\\\\');
    const script = [
        'const net = require("net");',
        'const before = net.Socket.prototype._write;',
        `const p = require("${probePath}");`,
        'if (!p.disabled) { console.error("probe not disabled"); process.exit(2); }',
        'if (net.Socket.prototype._write !== before) { console.error("core patched"); process.exit(3); }',
    ].join('\n');
    const res = spawnSync(process.execPath, ['-e', script], {
        env: { ...process.env, NCD_METRICS_ENABLED: '', NCD_METRICS_OUT: idlePath },
        encoding: 'utf8',
    });
    assert.equal(res.status, 0, res.stderr);
    assert.equal(fs.existsSync(idlePath), false, '关指标不应写快照');
});
