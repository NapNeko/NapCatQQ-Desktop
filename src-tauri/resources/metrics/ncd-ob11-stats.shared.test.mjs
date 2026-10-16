// Shared-daemon regressions run in their own Node test process, with the probe
// preloaded before any Bot or peer server starts listening.
import { test, before, after } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import http from 'node:http';
import net from 'node:net';
import { createRequire } from 'node:module';
import { WebSocketServer, WebSocket } from 'ws';

const require = createRequire(import.meta.url);
const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'ncd-probe-shared-'));
const registryDir = path.join(tmpDir, 'registry');
fs.mkdirSync(registryDir);

const servers = new Set();
const wsServers = new Set();
const sockets = new Set();
const wsClients = new Set();
const agents = new Set();

// Register cleanup before async setup, so a failed listen/assertion also drains
// keep-alive sockets and WebSocket connections instead of hanging the test run.
after(async () => {
    for (const agent of agents) agent.destroy();
    for (const client of wsClients) client.terminate();
    for (const wss of wsServers) for (const client of wss.clients) client.terminate();
    for (const socket of sockets) socket.destroy();
    await Promise.all([
        ...[...wsServers].map((wss) => new Promise((resolve) => wss.close(resolve))),
        ...[...servers].map((server) => new Promise((resolve) => server.close(resolve))),
    ]);
    fs.rmSync(tmpDir, { recursive: true, force: true });
});

function listen(server, port) {
    return new Promise((resolve, reject) => {
        const onError = (error) => { server.off('listening', onListening); reject(error); };
        const onListening = () => { server.off('error', onError); resolve(server.address().port); };
        server.once('error', onError);
        server.once('listening', onListening);
        server.listen(port, '127.0.0.1');
    });
}

const ports = {};

function atomicJson(file, value) {
    fs.mkdirSync(path.dirname(file), { recursive: true });
    fs.writeFileSync(`${file}.tmp`, JSON.stringify(value));
    fs.renameSync(`${file}.tmp`, file);
}

function descriptor(botId, accountId, nodes, enabled = true, intervalMs = 30000) {
    const entry = {
        botId, accountId: String(accountId),
        nodesPath: path.join(tmpDir, `${botId}-nodes.json`),
        statsOut: path.join(tmpDir, `${botId}-stats.json`),
        enabled, intervalMs,
    };
    atomicJson(entry.nodesPath, nodes);
    saveDescriptor(entry);
    return entry;
}

function saveDescriptor(entry) {
    atomicJson(path.join(registryDir, `${entry.botId}.json`), entry);
}

const httpUrl = (suffix) => `http://127.0.0.1:${ports.httpPeer}${suffix}`;
const wsUrl = (suffix) => `ws://127.0.0.1:${ports.wsPeer}${suffix}`;
let sharedWsUrl;
let hotWsUrl;
let alpha;
let beta;
let hot;

function initializeDescriptors() {
    sharedWsUrl = `ws://127.0.0.1:${ports.sharedWs}/same`;
    hotWsUrl = `ws://127.0.0.1:${ports.hotWs}/hot`;
    alpha = descriptor('alpha', 101, [
        { name: 'same-http', kind: 'httpServer', listenPort: ports.alphaHttp },
        { name: 'same-ws', kind: 'wsServer', listenPort: ports.alphaWs },
        { name: 'path-http', kind: 'httpClient', targetUrl: httpUrl('/bot') },
        { name: 'deep-http', kind: 'httpClient', targetUrl: httpUrl('/bot/deep') },
        { name: 'path-ws', kind: 'wsClient', targetUrl: wsUrl('/bot') },
        { name: 'deep-ws', kind: 'wsClient', targetUrl: wsUrl('/bot/deep') },
        { name: 'same-reverse', kind: 'wsClient', targetUrl: sharedWsUrl },
    ]);
    beta = descriptor('beta', 202, [
        { name: 'same-http', kind: 'httpServer', listenPort: ports.betaHttp },
        { name: 'same-ws', kind: 'wsServer', listenPort: ports.betaWs },
        { name: 'path-http', kind: 'httpClient', targetUrl: httpUrl('/botB') },
        { name: 'path-ws', kind: 'wsClient', targetUrl: wsUrl('/botB') },
        { name: 'same-reverse', kind: 'wsClient', targetUrl: sharedWsUrl },
    ]);
    hot = descriptor('hot', 303, [
        { name: 'hot-ws', kind: 'wsClient', targetUrl: hotWsUrl },
    ], false);
    probe.writeSnapshot();
}

process.env.NCD_METRICS_ENABLED = '1';
process.env.NCD_METRICS_REGISTRY_DIR = registryDir;
delete process.env.NCD_METRICS_OUT;
delete process.env.NCD_METRICS_NODES_PATH;
delete process.env.NCD_METRICS_NODES_JSON;
const probe = require('./ncd-ob11-stats.cjs');

const event = (selfId) => JSON.stringify({ post_type: 'message', self_id: selfId, raw_message: 'hello' });
const ACTION = JSON.stringify({ action: 'send_private_msg', params: { user_id: 42, message: 'hello' }, echo: '1' });
const RESPONSE = JSON.stringify({ status: 'ok', retcode: 0, data: null, echo: '1' });
const delay = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

async function waitFor(label, predicate, timeoutMs = 4000) {
    const deadline = Date.now() + timeoutMs;
    while (!predicate()) {
        if (Date.now() >= deadline) assert.fail(`Timed out: ${label}`);
        await delay(25);
    }
}

function readSnapshot(entry, force = true) {
    if (force) probe.writeSnapshot();
    return JSON.parse(fs.readFileSync(entry.statsOut, 'utf8'));
}

function stats(entry, name, force = true) {
    const node = readSnapshot(entry, force).nodes.find((item) => item.name === name);
    assert.ok(node, `${entry.botId}/${name} must appear in its own snapshot`);
    return node;
}

function traffic(node) {
    return {
        events: node.events_out, actions: node.actions_in,
        sent: node.bytes_out, received: node.bytes_in,
    };
}

function makeHttpServer(handler) {
    const server = http.createServer(handler);
    servers.add(server);
    server.on('connection', (socket) => {
        sockets.add(socket);
        socket.once('close', () => sockets.delete(socket));
    });
    return server;
}

function makeWsServer(onConnection) {
    const server = makeHttpServer();
    const wss = new WebSocketServer({ server });
    wsServers.add(wss);
    wss.on('connection', onConnection);
    return server;
}

function respond(req, res) {
    req.resume();
    req.once('end', () => {
        res.setHeader('content-type', 'application/json');
        res.end(RESPONSE);
    });
}

function wsPeer(client, req) {
    client.on('message', () => client.send(ACTION));
    if (req.headers['x-self-id']) client.send(ACTION);
}

const reportRequests = [];
const reportWireBytes = new WeakMap();

function recordReport(req, res) {
    const previous = reportWireBytes.get(req.socket) || { read: 0, written: 0 };
    const record = { body: '', complete: false };
    reportRequests.push(record);
    req.on('data', (chunk) => { record.body += chunk.toString(); });
    req.once('end', () => {
        res.setHeader('content-type', 'application/json');
        res.end(RESPONSE, () => {
            const current = { read: req.socket.bytesRead, written: req.socket.bytesWritten };
            record.received = current.read - previous.read;
            record.sent = current.written - previous.written;
            reportWireBytes.set(req.socket, current);
            record.complete = true;
        });
    });
}

before(async () => {
    const bound = await Promise.all([
        ['alphaHttp', makeHttpServer(respond)],
        ['betaHttp', makeHttpServer(respond)],
        ['httpPeer', makeHttpServer(respond)],
        ['alphaWs', makeWsServer((client) => {
            client.on('message', () => client.send(RESPONSE));
            client.send(event(101));
        })],
        ['betaWs', makeWsServer((client) => {
            client.on('message', () => client.send(RESPONSE));
            client.send(event(202));
        })],
        ['wsPeer', makeWsServer(wsPeer)],
        ['sharedWs', makeWsServer(wsPeer)],
        ['hotWs', makeWsServer(wsPeer)],
        ['sharedHttp', makeHttpServer(recordReport)],
        ['lateWs', makeWsServer(wsPeer)],
    ].map(async ([name, server]) => [name, await listen(server, 0)]));
    Object.assign(ports, Object.fromEntries(bound));
    initializeDescriptors();
});

async function post(port, requestPath, body, agent) {
    return await new Promise((resolve, reject) => {
        let socket;
        let previousRead = 0;
        let previousWritten = 0;
        const req = http.request({ host: '127.0.0.1', port, path: requestPath, method: 'POST', agent }, (res) => {
            res.resume();
            res.once('error', reject);
            res.once('end', () => resolve({
                socket,
                sent: socket.bytesWritten - previousWritten,
                received: socket.bytesRead - previousRead,
            }));
        });
        req.once('socket', (assigned) => {
            socket = assigned;
            previousRead = socket.bytesRead;
            previousWritten = socket.bytesWritten;
        });
        req.once('error', reject);
        req.setTimeout(4000, () => req.destroy(new Error('HTTP request timed out')));
        req.end(body);
    });
}

async function connectWs(url, options) {
    const client = new WebSocket(url, options);
    wsClients.add(client);
    const received = [];
    client.on('message', (message) => received.push(String(message)));
    await new Promise((resolve, reject) => {
        client.once('open', resolve);
        client.once('error', reject);
    });
    return { client, received };
}

async function closeWs(client) {
    if (client.readyState === WebSocket.CLOSED) return;
    await new Promise((resolve) => {
        const timeout = setTimeout(() => client.terminate(), 1000);
        client.once('close', () => { clearTimeout(timeout); resolve(); });
        client.close();
    });
}

test('two Bots with identical forward-node names write isolated HTTP and WS snapshots', async () => {
    assert.equal(probe.disabled, false);
    assert.equal(fs.existsSync(hot.statsOut), false, 'disabled Bot must not publish a snapshot');

    await post(ports.alphaHttp, '/send_private_msg', ACTION);
    assert.equal(stats(alpha, 'same-http').actions_in, 1);
    assert.deepEqual(traffic(stats(beta, 'same-http')), { events: 0, actions: 0, sent: 0, received: 0 });
    const alphaHttp = traffic(stats(alpha, 'same-http'));
    await post(ports.betaHttp, '/send_private_msg', ACTION);
    assert.deepEqual(traffic(stats(alpha, 'same-http')), alphaHttp);
    assert.equal(stats(beta, 'same-http').actions_in, 1);
    for (const entry of [alpha, beta]) {
        assert.ok(stats(entry, 'same-http').bytes_out > 0);
        assert.ok(stats(entry, 'same-http').bytes_in > 0);
        assert.ok(readSnapshot(entry).memory.rssBytes > 0);
    }

    for (const [entry, port, other] of [[alpha, ports.alphaWs, beta], [beta, ports.betaWs, alpha]]) {
        const otherBefore = traffic(stats(other, 'same-ws'));
        const { client, received } = await connectWs(`ws://127.0.0.1:${port}/`);
        await waitFor('forward WS event', () => received.length === 1);
        client.send(ACTION);
        await waitFor('forward WS response', () => received.length === 2);
        const own = stats(entry, 'same-ws');
        assert.equal(own.events_out, 1);
        assert.equal(own.actions_in, 1);
        assert.ok(own.bytes_out > 0 && own.bytes_in > 0);
        assert.deepEqual(traffic(stats(other, 'same-ws')), otherBefore);
        await closeWs(client);
    }
});

test('HTTP keep-alive reassigns every request to the longest path at a segment boundary', async () => {
    const agent = new http.Agent({ keepAlive: true, maxSockets: 1 });
    agents.add(agent);
    let previousSocket;
    for (const [entry, name, requestPath, selfId] of [
        [alpha, 'path-http', '/bot', 101],
        [beta, 'path-http', '/botB', 202],
        [alpha, 'deep-http', '/bot/deep/child?query=1', 101],
        [beta, 'path-http', '/botB/child', 202],
    ]) {
        const owners = [[alpha, 'path-http'], [alpha, 'deep-http'], [beta, 'path-http']];
        const beforeStats = owners.map(([owner, node]) => traffic(stats(owner, node)));
        const result = await post(ports.httpPeer, requestPath, event(selfId), agent);
        if (previousSocket) assert.equal(result.socket, previousSocket, 'fixture must reuse the same TCP connection');
        previousSocket = result.socket;
        for (let index = 0; index < owners.length; index += 1) {
            const [owner, node] = owners[index];
            const afterStats = traffic(stats(owner, node));
            const previous = beforeStats[index];
            if (owner === entry && node === name) {
                assert.equal(afterStats.events, previous.events + 1);
                assert.equal(afterStats.sent - previous.sent, result.sent, 'all request wire bytes belong to its path');
                assert.equal(afterStats.received - previous.received, result.received, 'all response wire bytes belong to its path');
            } else {
                assert.deepEqual(afterStats, previous, `${owner.botId}/${node} must not receive ${requestPath} traffic`);
            }
        }
    }
    agent.destroy();
});

test('fetch attributes same-host traffic by its complete request path', async () => {
    for (const [entry, name, requestPath, selfId] of [
        [beta, 'path-http', '/botB', 202],
        [alpha, 'deep-http', '/bot/deep/child', 101],
        [alpha, 'path-http', '/bot/child', 101],
    ]) {
        const owners = [[alpha, 'path-http'], [alpha, 'deep-http'], [beta, 'path-http']];
        const beforeStats = owners.map(([owner, node]) => traffic(stats(owner, node)));
        const response = await fetch(httpUrl(requestPath), {
            method: 'POST', headers: { 'content-type': 'application/json' }, body: event(selfId),
        });
        await response.text();
        for (let index = 0; index < owners.length; index += 1) {
            const [owner, node] = owners[index];
            const afterStats = traffic(stats(owner, node));
            const previous = beforeStats[index];
            if (owner === entry && node === name) {
                assert.equal(afterStats.events, previous.events + 1);
                assert.ok(afterStats.sent > previous.sent && afterStats.received > previous.received);
            } else assert.deepEqual(afterStats, previous);
        }
    }
});

test('reverse WS chooses /botB and /bot/deep without charging /bot', async () => {
    for (const [entry, name, requestPath, selfId] of [
        [beta, 'path-ws', '/botB', 202],
        [alpha, 'deep-ws', '/bot/deep/child', 101],
        [alpha, 'path-ws', '/bot/child', 101],
    ]) {
        const owners = [[alpha, 'path-ws'], [alpha, 'deep-ws'], [beta, 'path-ws']];
        const beforeStats = owners.map(([owner, node]) => traffic(stats(owner, node)));
        const { client, received } = await connectWs(wsUrl(requestPath));
        client.send(event(selfId));
        await waitFor('reverse WS action', () => received.length === 1);
        for (let index = 0; index < owners.length; index += 1) {
            const [owner, node] = owners[index];
            const afterStats = traffic(stats(owner, node));
            const previous = beforeStats[index];
            if (owner === entry && node === name) {
                assert.equal(afterStats.events, previous.events + 1);
                assert.equal(afterStats.actions, previous.actions + 1);
                assert.ok(afterStats.sent > previous.sent && afterStats.received > previous.received);
            } else assert.deepEqual(afterStats, previous);
        }
        await closeWs(client);
    }
});

test('shared reverse WS URL stays unowned until self_id or X-Self-ID identifies its Bot', async () => {
    for (const [entry, selfId, other] of [[alpha, 101, beta], [beta, 202, alpha]]) {
        const ownBefore = traffic(stats(entry, 'same-reverse'));
        const otherBefore = traffic(stats(other, 'same-reverse'));
        const { client, received } = await connectWs(sharedWsUrl);
        assert.deepEqual(traffic(stats(entry, 'same-reverse')), ownBefore, 'ambiguous handshake must not pick a Bot');
        assert.deepEqual(traffic(stats(other, 'same-reverse')), otherBefore);
        client.send(event(selfId));
        await waitFor('shared reverse WS action', () => received.length === 1);
        const own = traffic(stats(entry, 'same-reverse'));
        assert.equal(own.events, ownBefore.events + 1);
        assert.equal(own.actions, ownBefore.actions + 1);
        assert.ok(own.sent > ownBefore.sent && own.received > ownBefore.received);
        assert.deepEqual(traffic(stats(other, 'same-reverse')), otherBefore);
        await closeWs(client);
    }

    const alphaBefore = traffic(stats(alpha, 'same-reverse'));
    const betaBefore = traffic(stats(beta, 'same-reverse'));
    const { client, received } = await connectWs(sharedWsUrl, { headers: { 'X-Self-ID': '202' } });
    await waitFor('action identified by handshake header', () => received.length === 1);
    const identified = traffic(stats(beta, 'same-reverse'));
    assert.equal(identified.events, betaBefore.events);
    assert.equal(identified.actions, betaBefore.actions + 1);
    assert.ok(identified.sent > betaBefore.sent && identified.received > betaBefore.received);
    assert.deepEqual(traffic(stats(alpha, 'same-reverse')), alphaBefore);
    await closeWs(client);
});

test('registry discovers new Bots both on an explicit snapshot and on its timer', async () => {
    ports.addedHttp = await listen(makeHttpServer(respond), 0);
    const added = descriptor('added', 404, [
        { name: 'same-http', kind: 'httpServer', listenPort: ports.addedHttp },
    ]);
    probe.writeSnapshot();
    assert.equal(stats(added, 'same-http').actions_in, 0);
    const alphaBefore = traffic(stats(alpha, 'same-http'));
    await post(ports.addedHttp, '/send_private_msg', ACTION);
    assert.equal(stats(added, 'same-http').actions_in, 1);
    assert.deepEqual(traffic(stats(alpha, 'same-http')), alphaBefore);

    const timed = descriptor('timed', 505, []);
    await waitFor('timer discovers added descriptor without writeSnapshot', () => fs.existsSync(timed.statsOut), 3500);
    assert.deepEqual(readSnapshot(timed, false).nodes, []);
});

test('disabled Bot resumes the same WS connection and stops counting and publishing when disabled again', async () => {
    const { client, received } = await connectWs(hotWsUrl);
    client.send(event(303));
    await waitFor('disabled connection still carries application traffic', () => received.length === 1);
    probe.writeSnapshot();
    assert.equal(fs.existsSync(hot.statsOut), false);

    hot.enabled = true;
    saveDescriptor(hot);
    probe.writeSnapshot();
    assert.deepEqual(traffic(stats(hot, 'hot-ws')), { events: 0, actions: 0, sent: 0, received: 0 },
        'traffic sent while disabled must not be charged after activation');
    client.send(event(303));
    await waitFor('same connection works after activation', () => received.length === 2);
    const enabled = traffic(stats(hot, 'hot-ws'));
    assert.equal(enabled.events, 1);
    assert.equal(enabled.actions, 1);
    assert.ok(enabled.sent > 0 && enabled.received > 0);

    hot.enabled = false;
    saveDescriptor(hot);
    probe.writeSnapshot();
    const frozenFile = fs.readFileSync(hot.statsOut, 'utf8');
    client.send(event(303));
    await waitFor('disabled established connection still works', () => received.length === 3);
    probe.writeSnapshot();
    await delay(1100);
    assert.equal(fs.readFileSync(hot.statsOut, 'utf8'), frozenFile, 'disabled descriptor must stop snapshot publication');

    hot.enabled = true;
    saveDescriptor(hot);
    probe.writeSnapshot();
    assert.deepEqual(traffic(stats(hot, 'hot-ws')), enabled, 'disabled traffic must not accumulate in counters');
    client.send(event(303));
    await waitFor('same connection resumes a second time', () => received.length === 4);
    assert.equal(stats(hot, 'hot-ws').events_out, enabled.events + 1);
    assert.equal(stats(hot, 'hot-ws').actions_in, enabled.actions + 1);
    await closeWs(client);
});

test('descriptor intervalMs updates the live snapshot schedule', async () => {
    hot.intervalMs = 30000;
    saveDescriptor(hot);
    const previous = readSnapshot(hot).collectedAtMs;
    await delay(1200);
    assert.equal(readSnapshot(hot, false).collectedAtMs, previous, 'long interval must hold the last snapshot');

    hot.intervalMs = 1000;
    saveDescriptor(hot);
    await waitFor('shortened interval takes effect without explicit refresh',
        () => readSnapshot(hot, false).collectedAtMs > previous, 3500);
});

test('fetch to a shared HTTP URL rejects a disabled or removed Bot and preserves split pending events', async () => {
    const url = `http://127.0.0.1:${ports.sharedHttp}/report`;
    const nodes = [{ name: 'same-report', kind: 'httpClient', targetUrl: url }];
    const inactive = descriptor('report-inactive', 606, nodes, false);
    const active = descriptor('report-active', 707, nodes);
    probe.writeSnapshot();

    async function report(selfId, onPending) {
        const parts = ['{"post_', 'type":"message",', `"self_id":${selfId},"raw_message":"hello"}`];
        const beforeCount = reportRequests.length;
        let controller;
        const body = new ReadableStream({ start(streamController) { controller = streamController; } });
        const responsePromise = fetch(url, {
            method: 'POST', duplex: 'half', body,
            headers: {
                'content-type': 'application/json',
                // A fixed length lets JSON markers cross transport writes without
                // inserting HTTP chunk framing between the split marker bytes.
                'content-length': String(Buffer.byteLength(parts.join(''))),
            },
        });
        try {
            controller.enqueue(Buffer.from(parts[0]));
            await waitFor('first streamed body fragment arrives',
                () => reportRequests.length > beforeCount && reportRequests[beforeCount].body === parts[0]);
            onPending();
            controller.enqueue(Buffer.from(parts[1]));
            await waitFor('split post_type arrives before self_id',
                () => reportRequests[beforeCount].body === parts[0] + parts[1]);
            onPending();
            controller.enqueue(Buffer.from(parts[2]));
            controller.close();
            const response = await responsePromise;
            await response.text();
            await waitFor('HTTP peer records complete wire exchange', () => reportRequests[beforeCount].complete);
            return reportRequests[beforeCount];
        } catch (error) {
            try { controller.error(error); } catch (_) { /* Already closed. */ }
            await responsePromise.catch(() => {});
            throw error;
        }
    }

    for (const state of ['disabled', 'removed']) {
        if (state === 'removed') {
            fs.unlinkSync(path.join(registryDir, `${inactive.botId}.json`));
            probe.writeSnapshot();
        }
        const beforeRejected = traffic(stats(active, 'same-report'));
        await report(606, () => {
            assert.deepEqual(traffic(stats(active, 'same-report')), beforeRejected,
                `${state} Bot's incomplete event must not be assigned to the active Bot`);
        });
        assert.deepEqual(traffic(stats(active, 'same-report')), beforeRejected,
            `${state} Bot's event and wire bytes must not be assigned to the active Bot`);
        assert.equal(fs.existsSync(inactive.statsOut), false);

        const beforeAccepted = traffic(stats(active, 'same-report'));
        const wire = await report(707, () => {
            assert.deepEqual(traffic(stats(active, 'same-report')), beforeAccepted,
                'event and bytes remain pending until its self_id arrives');
        });
        const afterAccepted = traffic(stats(active, 'same-report'));
        assert.equal(afterAccepted.events, beforeAccepted.events + 1,
            'post_type observed before self_id must be retained and counted once');
        assert.equal(afterAccepted.actions, beforeAccepted.actions);
        assert.equal(afterAccepted.sent - beforeAccepted.sent, wire.received,
            'the active Bot receives exactly its own complete request wire bytes');
        assert.equal(afterAccepted.received - beforeAccepted.received, wire.sent,
            'the active Bot receives exactly its own response wire bytes');
    }
});

test('an already upgraded unregistered WS connection becomes observable after descriptor discovery', async () => {
    for (const [botId, selfId, discovery] of [['late-manual', 808, 'manual'], ['late-timed', 909, 'timer']]) {
        const url = `ws://127.0.0.1:${ports.lateWs}/${botId}`;
        const { client, received } = await connectWs(url);
        const originalSocket = client._socket;
        client.send(event(selfId));
        await waitFor('unregistered WS carries traffic before discovery', () => received.length === 1);

        const entry = descriptor(botId, selfId, [{ name: 'late-reverse', kind: 'wsClient', targetUrl: url }]);
        if (discovery === 'manual') probe.writeSnapshot();
        else await waitFor('timer discovers descriptor for existing WS', () => fs.existsSync(entry.statsOut), 3500);
        assert.deepEqual(traffic(stats(entry, 'late-reverse', false)),
            { events: 0, actions: 0, sent: 0, received: 0 }, 'pre-registration traffic must stay uncounted');
        const previousWritten = originalSocket.bytesWritten;
        const previousRead = originalSocket.bytesRead;
        client.send(event(selfId));
        await waitFor('original WS receives action after discovery', () => received.length === 2);
        assert.equal(client._socket, originalSocket, 'Bot registration must not require a new connection');
        const observed = stats(entry, 'late-reverse');
        assert.equal(observed.events_out, 1);
        assert.equal(observed.actions_in, 1);
        assert.equal(observed.bytes_out, originalSocket.bytesWritten - previousWritten);
        assert.equal(observed.bytes_in, originalSocket.bytesRead - previousRead);
        assert.ok(observed.bytes_out > 0 && observed.bytes_in > 0);
        await closeWs(client);
    }
});
