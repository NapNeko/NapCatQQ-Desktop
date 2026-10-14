// ncd-ob11-stats: Desktop-owned OneBot traffic probe (preload via require).
// Counts frames/bytes per configured network node; writes net-stats.json atomically.
// Does not log message bodies. Controlled by env:
//   NCD_METRICS_ENABLED=1
//   NCD_METRICS_OUT=<abs path>
//   NCD_METRICS_INTERVAL_MS=3000
//   NCD_METRICS_NODES_JSON=<json array> OR NCD_METRICS_NODES_PATH=<file>
// Shared SnowLuma daemon: NCD_METRICS_REGISTRY_DIR=<directory>, containing
// { botId, accountId, nodesPath, statsOut, enabled, intervalMs } descriptors.
//
// 只挂 node 核心模块（net / http / https / diagnostics_channel）。NapCat 把 `ws` 打进
// bundle，SnowLuma 自带 `@snowluma/websocket`，针对 ws 包的 hook 拿不到正反向连接。核心模块是两边共同
// 的最底层出口，且不随上游版本漂移。
//
// 归属：服务端节点按 socket.localPort 对 listenPort；客户端节点按 remote host:port 对
// targetUrl。字节走 socket.bytesRead/bytesWritten 增量（HTTP server 走 C++ parser
// consume，不发 'data' 事件，只有这两个计数器可信）。条数：HTTP 服务端看 'request'
// 事件，WS 看帧解析。

'use strict';

const fs = require('fs');
const path = require('path');
const http = require('http');
const https = require('https');
const net = require('net');
const diagnostics = require('diagnostics_channel');

if (process.env.NCD_METRICS_ENABLED !== '1') {
    module.exports = { disabled: true };
    return;
}

const outPath = process.env.NCD_METRICS_OUT;
const registryDir = process.env.NCD_METRICS_REGISTRY_DIR;
if (!outPath && !registryDir) {
    module.exports = { disabled: true, reason: 'missing NCD_METRICS_OUT' };
    return;
}

function clampInterval(value) {
    return Math.max(1000, Math.min(30000, parseInt(value || '3000', 10) || 3000));
}
const intervalMs = clampInterval(process.env.NCD_METRICS_INTERVAL_MS);

/** 单帧超过这个大小只记条数不看正文，避免为统计缓冲大图片 */
const MAX_SCAN_PAYLOAD = 64 * 1024;
/** 半包缓冲上限；超了说明解析已经跟丢，放弃该 socket 的帧解析（字节仍算） */
const MAX_FRAME_BUFFER = 512 * 1024;
const WS_OPCODES = new Set([0x0, 0x1, 0x2, 0x8, 0x9, 0xa]);

const contexts = new Map();
/** listenPort -> candidate nodes; shared ports must never pick the first Bot */
let portIndex = new Map();
let targetIndex = [];

/** @type {Map<string, any>} */
const counters = new Map();

function ensureNode(node) {
    let c = counters.get(node.key);
    if (!c) {
        c = {
            name: node.name,
            kind: node.kind || 'unknown',
            eventsOut: 0,
            actionsIn: 0,
            bytesOut: 0,
            bytesIn: 0,
            errors: 0,
            lastActivityAtMs: 0,
        };
        counters.set(node.key, c);
    } else if (node.kind && c.kind === 'unknown') {
        c.kind = node.kind;
    }
    return c;
}

function normalizeHost(host) {
    const s = String(host || '').toLowerCase().replace(/^\[/, '').replace(/\]$/, '');
    if (!s || s === 'localhost' || s === '::1' || s === '::ffff:127.0.0.1' || s === '0.0.0.0' || s === '::') {
        return '127.0.0.1';
    }
    if (s.startsWith('::ffff:')) return s.slice(7);
    return s;
}

/** ws:// / wss:// 归一成 http 家族再解析，只为拿 host/port/path */
function parseTarget(url) {
    try {
        const raw = String(url).replace(/^ws:/i, 'http:').replace(/^wss:/i, 'https:');
        const u = new URL(raw);
        return {
            host: normalizeHost(u.hostname),
            port: u.port ? Number(u.port) : (u.protocol === 'https:' ? 443 : 80),
            path: u.pathname || '/',
        };
    } catch (_) {
        return null;
    }
}

function applyNodeMap(context, next) {
    if (!Array.isArray(next)) return;
    context.nodes = [];
    for (const n of next) {
        if (!n || !n.name) continue;
        const node = { ...n, name: String(n.name), kind: n.kind || 'unknown', botId: context.botId,
            accountId: context.accountId, key: context.botId ? `${context.botId}\0${n.name}` : String(n.name) };
        context.nodes.push(node);
        ensureNode(node);
    }
    context.keys = new Set(context.nodes.map((node) => node.key));
}

function rebuildIndexes() {
    const ports = new Map();
    const targets = [];
    for (const context of contexts.values()) {
        for (const node of context.nodes) {
            const listenPort = Number(node.listenPort || node.listen_port || 0);
            if (listenPort > 0) {
                if (!ports.has(listenPort)) ports.set(listenPort, []);
                ports.get(listenPort).push(node);
            }
            const targetUrl = node.targetUrl || node.target_url;
            if (targetUrl) {
                const t = parseTarget(targetUrl);
                if (t) targets.push({ node, ...t });
            }
        }
    }
    portIndex = ports;
    targetIndex = targets;
}

function refreshRegistry() {
    if (!registryDir) {
        let context = contexts.get('');
        if (!context) {
            context = { botId: '', accountId: '', statsOut: outPath, enabled: true, intervalMs, nodes: [], keys: new Set() };
            contexts.set('', context);
        }
        try {
            if (process.env.NCD_METRICS_NODES_JSON) {
                if (!context.loaded) applyNodeMap(context, JSON.parse(process.env.NCD_METRICS_NODES_JSON));
                context.loaded = true;
            } else if (process.env.NCD_METRICS_NODES_PATH) {
                applyNodeMap(context, JSON.parse(fs.readFileSync(process.env.NCD_METRICS_NODES_PATH, 'utf8')));
            }
        } catch (_) { /* Keep the last complete map during an atomic update. */ }
    } else {
        let files;
        try { files = fs.readdirSync(registryDir).filter((file) => file.endsWith('.json')); }
        catch (_) { return; }
        const seen = new Set();
        for (const file of files) {
            try {
                const descriptor = JSON.parse(fs.readFileSync(path.join(registryDir, file), 'utf8'));
                if (!descriptor.botId || !descriptor.statsOut || !descriptor.nodesPath) continue;
                const botId = String(descriptor.botId);
                seen.add(botId);
                let context = contexts.get(botId);
                if (!context) {
                    context = { botId, nodes: [], keys: new Set() };
                    contexts.set(botId, context);
                }
                context.accountId = String(descriptor.accountId || botId);
                context.descriptorFile = file;
                context.statsOut = descriptor.statsOut;
                if (descriptor.enabled === true && !context.enabled) context.lastWrite = 0;
                context.enabled = descriptor.enabled === true;
                context.intervalMs = clampInterval(descriptor.intervalMs);
                try { applyNodeMap(context, JSON.parse(fs.readFileSync(descriptor.nodesPath, 'utf8'))); }
                catch (_) { /* Do not replace a valid map with a partial write. */ }
            } catch (_) {
                // A malformed descriptor must not redirect traffic to another Bot.
                const previous = [...contexts.values()].find((context) => context.descriptorFile === file);
                if (previous) seen.add(previous.botId);
            }
        }
        for (const botId of contexts.keys()) if (!seen.has(botId)) contexts.delete(botId);
    }
    rebuildIndexes();
}

refreshRegistry();

function matchByPort(port) {
    return portIndex.get(Number(port)) || [];
}

function matchByHostPort(host, port, hintPath) {
    if (!port) return [];
    const h = normalizeHost(host);
    const p = Number(port);
    const matches = [];
    let longest = -1;
    const requestPath = hintPath && String(hintPath).split(/[?#]/, 1)[0];
    for (const t of targetIndex) {
        if (t.port !== p) continue;
        if (t.host !== h) continue;
        if (requestPath) {
            const targetPath = t.path === '/' ? '/' : t.path.replace(/\/$/, '');
            if (targetPath !== '/' && requestPath !== targetPath && !requestPath.startsWith(`${targetPath}/`)) continue;
            if (targetPath.length < longest) continue;
            if (targetPath.length > longest) { matches.length = 0; longest = targetPath.length; }
        }
        matches.push(t.node);
    }
    return matches;
}

function activeNode(node) {
    if (!node) return false;
    const context = contexts.get(node.botId);
    return !!(context && context.enabled && context.keys.has(node.key));
}

function payloadAccountId(payload) {
    const match = payload && payload.subarray(0, MAX_SCAN_PAYLOAD).toString('utf8')
        .match(/"self_id"\s*:\s*(?:"(\d+)"|(\d+))(?=\s*[,}])/);
    return match && (match[1] || match[2]);
}

function selectNode(candidates, payload, accountHint) {
    let matches = candidates;
    const identity = payloadAccountId(payload) || accountHint;
    if (identity) matches = matches.filter((node) => !node.accountId || node.accountId === String(identity));
    const unique = new Map(matches.map((node) => [node.key, node]));
    const node = unique.size === 1 ? unique.values().next().value : null;
    return activeNode(node) ? node : null;
}

function touch(node) {
    const c = ensureNode(node);
    c.lastActivityAtMs = Date.now();
    return c;
}

const MARK_EVENT = Buffer.from('"post_type"');
const MARK_ACTION = Buffer.from('"action"');
const MARK_RETCODE = Buffer.from('"retcode"');
const MARK_ECHO = Buffer.from('"echo"');

/**
 * 走 Buffer.indexOf 而不是 JSON.parse：这里是每条消息都过的热路径，不能再把业务已经
 * 解析过一遍的 JSON 重解一遍。
 */
function classifyPayload(buf) {
    if (!buf || !buf.length) return 'unknown';
    const head = buf.length > MAX_SCAN_PAYLOAD ? buf.subarray(0, MAX_SCAN_PAYLOAD) : buf;
    if (head.indexOf(MARK_EVENT) !== -1) return 'event';
    if (head.indexOf(MARK_ACTION) !== -1) return 'action';
    if (head.indexOf(MARK_RETCODE) !== -1 || head.indexOf(MARK_ECHO) !== -1) return 'response';
    return 'unknown';
}

/**
 * OneBot 方向语义：Bot 出站 = 事件（含心跳），入站 = action 调用。
 * 正文可读时以正文为准，这样 action 回执不会被算成事件；压缩帧/二进制帧退回方向判定。
 */
function countMessage(node, dir, payload) {
    if (!activeNode(node)) return;
    const cls = classifyPayload(payload);
    const c = touch(node);
    if (dir === 'out') {
        if (cls === 'event' || cls === 'unknown') c.eventsOut += 1;
    } else if (cls === 'action' || cls === 'unknown') {
        c.actionsIn += 1;
    }
}

function countError(node) {
    if (!activeNode(node)) return;
    const c = ensureNode(node);
    c.errors += 1;
}

// --- WebSocket 帧扫描 -------------------------------------------------------

function createFrameScanner(onMessage) {
    let buf = null;
    let skip = 0;
    let broken = false;

    return function feed(chunk) {
        if (broken || !chunk || !chunk.length) return;
        let data = Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk);
        if (skip > 0) {
            if (data.length <= skip) {
                skip -= data.length;
                return;
            }
            data = data.subarray(skip);
            skip = 0;
        }
        buf = buf && buf.length ? Buffer.concat([buf, data]) : Buffer.from(data);

        for (;;) {
            if (buf.length < 2) break;
            const b0 = buf[0];
            const b1 = buf[1];
            const opcode = b0 & 0x0f;
            const compressed = (b0 & 0x40) !== 0;
            if (!WS_OPCODES.has(opcode) || (b0 & 0x30) !== 0) {
                broken = true;
                buf = null;
                return;
            }
            const masked = (b1 & 0x80) !== 0;
            let len = b1 & 0x7f;
            let off = 2;
            if (len === 126) {
                if (buf.length < 4) break;
                len = buf.readUInt16BE(2);
                off = 4;
            } else if (len === 127) {
                if (buf.length < 10) break;
                const big = buf.readBigUInt64BE(2);
                if (big > 0x7fffffffn) {
                    broken = true;
                    buf = null;
                    return;
                }
                len = Number(big);
                off = 10;
            }
            let mask = null;
            if (masked) {
                if (buf.length < off + 4) break;
                mask = buf.subarray(off, off + 4);
                off += 4;
            }

            if (len > MAX_SCAN_PAYLOAD) {
                if (opcode === 0x1 || opcode === 0x2) onMessage(null);
                const available = buf.length - off;
                if (available >= len) {
                    buf = buf.subarray(off + len);
                    continue;
                }
                skip = len - available;
                buf = null;
                return;
            }

            if (buf.length < off + len) {
                if (buf.length > MAX_FRAME_BUFFER) {
                    broken = true;
                    buf = null;
                }
                break;
            }

            let payload = buf.subarray(off, off + len);
            if (mask) {
                const copy = Buffer.from(payload);
                for (let i = 0; i < copy.length; i += 1) copy[i] ^= mask[i & 3];
                payload = copy;
            }
            // 压缩帧（permessage-deflate）读不出正文，交给方向判定
            if (opcode === 0x1) onMessage(compressed ? null : payload);
            else if (opcode === 0x2) onMessage(null);

            buf = buf.subarray(off + len);
            if (!buf.length) {
                buf = null;
                break;
            }
        }
    };
}

// --- socket 记账 ------------------------------------------------------------

const INSTRUMENTED = Symbol('ncdMetricsInstrumented');
/** 活跃 socket 记账条目；写快照时统一采样字节增量 */
const liveSockets = new Set();
/** socket -> 记账条目，给 http 客户端 upgrade 回调找回状态 */
const entryBySocket = new WeakMap();

function sampleBytes(entry) {
    const socket = entry.socket;
    if (!socket) return;
    let read = 0;
    let written = 0;
    try {
        read = socket.bytesRead || 0;
        written = socket.bytesWritten || 0;
    } catch (_) {
        return;
    }
    const dIn = read - entry.lastRead;
    const dOut = written - entry.lastWritten;
    if (dIn <= 0 && dOut <= 0) return;
    entry.lastRead = read;
    entry.lastWritten = written;
    if (!activeNode(entry.node)) {
        if (entry.pending) {
            entry.pendingIn += Math.max(0, dIn);
            entry.pendingOut += Math.max(0, dOut);
        }
        return;
    }
    const c = ensureNode(entry.node);
    if (dIn > 0) {
        c.bytesIn += dIn;
        entry.lastRead = read;
    }
    if (dOut > 0) {
        c.bytesOut += dOut;
        entry.lastWritten = written;
    }
    c.lastActivityAtMs = Date.now();
}

function sampleAllSockets() {
    for (const entry of liveSockets) {
        try {
            sampleBytes(entry);
            if (entry.socket && entry.socket.destroyed) liveSockets.delete(entry);
        } catch (_) {
            /* 单个 socket 记账失败不影响其它 */
        }
    }
}

function enterWsMode(entry) {
    if (entry.mode === 'ws') return;
    entry.mode = 'ws';
    const onMessage = (dir, payload) => {
        const identity = payloadAccountId(payload);
        if (identity) entry.accountHint = identity;
        const node = selectNode(entry.candidates, payload, entry.accountHint);
        if (node) bindNode(entry, node);
        else if (identity) { bindNode(entry, null); entry.pending = false; }
        countMessage(entry.node, dir, payload);
    };
    entry.scanIn = createFrameScanner((payload) => onMessage('in', payload));
    entry.scanOut = createFrameScanner((payload) => onMessage('out', payload));
}

function bindNode(entry, node) {
    if (entry.node && node && entry.node.key === node.key) { entry.node = node; return; }
    sampleBytes(entry);
    entry.node = activeNode(node) ? node : null;
    if (entry.node && entry.pending) {
        const counter = touch(entry.node);
        counter.bytesIn += entry.pendingIn;
        counter.bytesOut += entry.pendingOut;
        counter.eventsOut += entry.pendingEvents || 0;
        entry.pendingEvents = 0;
    }
    entry.pendingIn = 0;
    entry.pendingOut = 0;
}

function beginRequest(entry, candidates, accountHint, requestPath, preservePending = false) {
    sampleBytes(entry);
    entry.candidates = candidates;
    entry.accountHint = accountHint;
    if (requestPath !== undefined) entry.requestPath = requestPath;
    if (!preservePending) {
        entry.pendingIn = 0;
        entry.pendingOut = 0;
    }
    entry.pending = candidates.some(activeNode);
    entry.node = null;
    // HTTP reports can come from an unconfigured/disabled account at the same
    // destination. Keep their headers pending until the body identifies the Bot.
    const deferAccountIdentity = registryDir && !accountHint && entry.address;
    if (!deferAccountIdentity) bindNode(entry, selectNode(candidates, null, accountHint));
    entry.httpEventTail = null;
    entry.identityBuffer = null;
    entry.pendingEvents = 0;
}

/** 握手响应里出现 101 就把整条连接切成 WS 模式（两个方向共用一个状态） */
function detectUpgrade(entry, text) {
    if (entry.mode !== 'http') return -1;
    const head = text.indexOf('\r\n\r\n');
    if (head === -1) return -1;
    if (!/^HTTP\/1\.[01] 101/i.test(text)) return -1;
    enterWsMode(entry);
    return head + 4;
}

const EVENT_MARKER = Buffer.from('"post_type"');

/** Buffer.indexOf 是原生查找，比 toString 后再扫便宜，也不怕正文超出截断窗口 */
function countEventMarkers(entry, buf) {
    const data = entry.httpEventTail ? Buffer.concat([entry.httpEventTail, buf]) : buf;
    const identity = (entry.identityBuffer ? Buffer.concat([entry.identityBuffer, buf]) : data).subarray(0, MAX_SCAN_PAYLOAD);
    const accountId = payloadAccountId(identity);
    if (accountId) entry.accountHint = accountId;
    const waitForAccount = registryDir && entry.address && !entry.accountHint
        && entry.candidates.some((node) => node.kind === 'httpClient');
    const node = waitForAccount ? null : selectNode(entry.candidates, identity, entry.accountHint);
    if (node) bindNode(entry, node);
    else if (accountId) {
        bindNode(entry, null);
        entry.pending = false;
        entry.pendingEvents = 0;
    }
    if (!entry.node && entry.pending) entry.identityBuffer = Buffer.from(identity);
    else entry.identityBuffer = null;
    let idx = data.indexOf(EVENT_MARKER);
    while (idx !== -1) {
        if (activeNode(entry.node)) touch(entry.node).eventsOut += 1;
        else if (entry.pending) entry.pendingEvents += 1;
        idx = data.indexOf(EVENT_MARKER, idx + EVENT_MARKER.length);
    }
    // A split marker must count once when completed in the next write.
    entry.httpEventTail = Buffer.from(data.subarray(Math.max(0, data.length - EVENT_MARKER.length + 1)));
}

function observeOutbound(entry, chunk, encoding) {
    if (!chunk) return;
    const buf = Buffer.isBuffer(chunk)
        ? chunk
        : (typeof chunk === 'string' ? Buffer.from(chunk, typeof encoding === 'string' ? encoding : 'utf8') : null);
    if (!buf || !buf.length) return;

    if (entry.mode === 'http') {
        if (entry.address && (/^(?:GET|POST|PUT|PATCH|DELETE|HEAD|OPTIONS) /.test(buf.subarray(0, 16).toString('latin1')) || entry.requestHead)) {
            entry.requestHead = entry.requestHead ? Buffer.concat([entry.requestHead, buf]) : buf;
            const end = entry.requestHead.indexOf('\r\n\r\n');
            if (end < 0) {
                if (entry.requestHead.length > MAX_SCAN_PAYLOAD) entry.requestHead = null;
                return;
            }
            const head = entry.requestHead.subarray(0, end).toString('latin1');
            const requestPath = (head.match(/^\S+\s+(\S+)/) || [])[1];
            const accountHint = (head.match(/\r\nx-self-id:\s*(\d+)/i) || [])[1];
            if (!entry.viaHttpHook && !entry.viaUndiciHook) {
                beginRequest(entry, matchByHostPort(entry.address.host, entry.address.port, requestPath), accountHint, requestPath, !entry.node);
            }
            entry.requestHead = null;
        }
        const consumed = detectUpgrade(entry, buf.subarray(0, Math.min(buf.length, 2048)).toString('latin1'));
        if (consumed >= 0) {
            if (consumed < buf.length) entry.scanOut(buf.subarray(consumed));
            return;
        }
        // SSE 没有帧头可数；SnowLuma 的 HTTP 上报走全局 fetch(undici)，不经 http.request
        // 钩子。两者都只能在字节流上扫事件标记。
        if (entry.candidates.some((node) => node.kind === 'httpSse' || (node.kind === 'httpClient' && !entry.viaHttpHook))) {
            countEventMarkers(entry, buf);
        }
        return;
    }
    if (entry.scanOut) entry.scanOut(buf);
}

function observeInbound(entry, chunk) {
    if (!chunk || !chunk.length) return;
    const buf = Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk);
    if (entry.mode === 'http') {
        // http.request owns its parser and exposes the upgrade head separately.
        if (entry.viaHttpHook) return;
        const consumed = detectUpgrade(entry, buf.subarray(0, Math.min(buf.length, 2048)).toString('latin1'));
        if (consumed >= 0 && consumed < buf.length) entry.scanIn(buf.subarray(consumed));
        return;
    }
    if (entry.replayedHeadBytes) {
        const skipped = Math.min(entry.replayedHeadBytes, buf.length);
        entry.replayedHeadBytes -= skipped;
        if (skipped < buf.length && entry.scanIn) entry.scanIn(buf.subarray(skipped));
        return;
    }
    if (entry.scanIn) entry.scanIn(buf);
}

/**
 * 只观察不改流：入站包 emit（加 'data' 监听会把暂停的流拽进 flowing 模式，可能把数据从
 * 真正的消费者手里抢走）；出站在 net.Socket.prototype._write/_writev 统一挂，
 * 不逐 socket 包 write —— http 客户端 upgrade 后会把 socket.write 换回原生实现，
 * 挂在实例上的包装会被直接冲掉。
 */
function instrumentSocket(socket, candidates, address) {
    if (!socket || (!candidates.length && !registryDir)) return null;
    const existing = entryBySocket.get(socket);
    if (existing) {
        if (address) existing.address = address;
        return existing;
    }
    try {
        Object.defineProperty(socket, INSTRUMENTED, { value: true, enumerable: false });
    } catch (_) {
        return;
    }

    const entry = {
        socket,
        node: address ? null : selectNode(candidates),
        candidates,
        address,
        pending: candidates.some(activeNode),
        pendingIn: 0,
        pendingOut: 0,
        mode: 'http',
        scanIn: null,
        scanOut: null,
        lastRead: 0,
        lastWritten: 0,
    };
    liveSockets.add(entry);
    entryBySocket.set(socket, entry);

    const origEmit = socket.emit;
    socket.emit = function (event, arg) {
        try {
            if (event === 'data') observeInbound(entry, arg);
            else if (event === 'error') countError(entry.node);
            else if (event === 'close') {
                sampleBytes(entry);
                liveSockets.delete(entry);
            }
        } catch (_) {
            /* 同上 */
        }
        return origEmit.apply(this, arguments);
    };
    // WS consumers often unshift the upgrade head back into the readable stream.
    // Those bytes were already scanned in req.upgrade; observe the replay without
    // consuming it or changing the socket's flowing state.
    const origUnshift = socket.unshift;
    socket.unshift = function (chunk) {
        if (entry.upgradeHead && Buffer.isBuffer(chunk) && chunk.equals(entry.upgradeHead)) {
            entry.replayedHeadBytes = (entry.replayedHeadBytes || 0) + chunk.length;
        }
        return origUnshift.apply(this, arguments);
    };
    return entry;
}

// 出站统一挂在 stream 内部写口：cork/uncork 批量、writev、以及 upgrade 后被换掉的
// socket.write 都会落到这里；TLSSocket 走同一条路，拿到的还是加密前的明文。
try {
    const origWrite = net.Socket.prototype._write;
    if (typeof origWrite === 'function') {
        net.Socket.prototype._write = function (data, encoding, cb) {
            try {
                const entry = entryBySocket.get(this);
                if (entry) observeOutbound(entry, data, encoding);
            } catch (_) { }
            return origWrite.apply(this, arguments);
        };
    }
    const origWritev = net.Socket.prototype._writev;
    if (typeof origWritev === 'function') {
        net.Socket.prototype._writev = function (chunks, cb) {
            try {
                const entry = entryBySocket.get(this);
                if (entry && Array.isArray(chunks)) {
                    for (const item of chunks) {
                        if (item) observeOutbound(entry, item.chunk, item.encoding);
                    }
                }
            } catch (_) { }
            return origWritev.apply(this, arguments);
        };
    }
} catch (_) { }

// --- 服务端：listen 时记端口，connection 时挂 socket -------------------------

function attachServer(server) {
    if (!server || server[INSTRUMENTED]) return;
    try {
        Object.defineProperty(server, INSTRUMENTED, { value: true, enumerable: false });
    } catch (_) {
        return;
    }

    // TLS server 的 'connection' 给的是密文 socket，'secureConnection' 才是明文；
    // 只挂一个，避免同一条连接被算两遍。
    const secure = typeof server.setSecureContext === 'function';
    server.on(secure ? 'secureConnection' : 'connection', (socket) => {
        try {
            instrumentSocket(socket, matchByPort(socket && socket.localPort));
        } catch (_) { }
    });

    server.on('request', (req) => {
        try {
            const candidates = matchByPort(req.socket && req.socket.localPort);
            const node = selectNode(candidates, null, req.headers['x-self-id']);
            if (!node) return;
            if (node.kind === 'httpSse' && req.method === 'GET') return; // GET 是订阅流，不是 action
            touch(node).actionsIn += 1;
        } catch (_) { }
    });
    server.prependListener('upgrade', (req, socket) => {
        try {
            const entry = instrumentSocket(socket, matchByPort(socket.localPort));
            if (entry) beginRequest(entry, matchByPort(socket.localPort), req.headers['x-self-id']);
        } catch (_) { }
    });
}

try {
    const origListen = net.Server.prototype.listen;
    net.Server.prototype.listen = function () {
        // 挂在 listen 而不是 createServer：此时业务 connectionListener 已注册，我们的
        // 观察者排在它后面，http 解析器先接管 socket
        try {
            attachServer(this);
        } catch (_) { }
        return origListen.apply(this, arguments);
    };
} catch (_) { }

// --- 客户端：connect 目标对 targetUrl ---------------------------------------

function connectTarget(args) {
    let a0 = args[0];
    // net.connect(...) 会把参数归一成 [options, cb] 数组再转调 Socket.prototype.connect
    if (Array.isArray(a0)) a0 = a0[0];
    if (a0 && typeof a0 === 'object') {
        if (a0.path && !a0.port) return null; // IPC / unix socket
        return { host: a0.host || a0.hostname, port: Number(a0.port) };
    }
    const port = Number(a0);
    if (!Number.isFinite(port) || port <= 0) return null;
    return { host: typeof args[1] === 'string' ? args[1] : '127.0.0.1', port };
}

try {
    const origConnect = net.Socket.prototype.connect;
    net.Socket.prototype.connect = function () {
        try {
            const target = connectTarget(arguments);
            if (target) {
                instrumentSocket(this, matchByHostPort(target.host, target.port), target);
            }
        } catch (_) { }
        return origConnect.apply(this, arguments);
    };
} catch (_) { }

// --- HTTP 客户端：请求体判事件，socket 归属交给记账 --------------------------

function wrapHttpModule(mod, isHttps) {
    const origRequest = mod.request;
    const origGet = mod.get;

    function wrapReq(args, orig) {
        const req = orig.apply(mod, args);
        try {
            let urlHint = '';
            const opts = args[0];
            if (typeof opts === 'string') urlHint = opts;
            else if (opts instanceof URL) urlHint = opts.href;
            else if (opts && typeof opts === 'object') {
                const proto = isHttps ? 'https:' : 'http:';
                const host = opts.hostname || opts.host || '127.0.0.1';
                const port = opts.port || (isHttps ? 443 : 80);
                const p = opts.path || '/';
                urlHint = `${proto}//${host}:${port}${p}`;
            }
            const target = parseTarget(urlHint);
            const candidates = target ? matchByHostPort(target.host, target.port, req.path) : [];
            if (!candidates.length && !registryDir) return req;
            let node = selectNode(candidates, null, req.getHeader('x-self-id'));
            let requestEntry = null;

            // 包 emit 而不是 on：node 客户端拿 req.listenerCount('upgrade') 决定要不要
            // 销毁一个意料之外的 101 连接，凭空加监听者会把该销毁的 socket 留下来。
            const origReqEmit = req.emit;
            req.emit = function (event, a, b, c) {
                try {
                    if (event === 'socket') {
                        const entry = instrumentSocket(a, candidates, target);
                        // 这条连接的事件由下面的 req.end 判定，socket 层别再扫一遍
                        if (entry) {
                            requestEntry = entry;
                            beginRequest(entry, candidates, req.getHeader('x-self-id'), req.path);
                            if (node) bindNode(entry, node);
                            entry.viaHttpHook = true;
                            entry.viaUndiciHook = false;
                        }
                    } else if (event === 'upgrade') {
                        // 客户端的 101 也走 C++ parser，socket 不发 'data'，只有这里拿得到切换时机
                        const entry = instrumentSocket(b, candidates, target);
                        if (entry) {
                            enterWsMode(entry);
                            if (c && c.length) {
                                entry.scanIn(c);
                                entry.upgradeHead = c;
                            }
                        }
                    }
                } catch (_) { }
                try { return origReqEmit.apply(this, arguments); }
                finally {
                    if (event === 'upgrade') {
                        const entry = entryBySocket.get(b);
                        if (entry) entry.upgradeHead = null;
                    }
                }
            };

            const chunks = [];
            let bodyLen = 0;
            const push = (chunk, enc) => {
                if (!chunk || typeof chunk === 'function') return;
                const buf = Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk, typeof enc === 'string' ? enc : 'utf8');
                bodyLen += buf.length;
                if (bodyLen <= MAX_SCAN_PAYLOAD) chunks.push(buf);
            };
            const origWrite = req.write;
            const origEnd = req.end;
            req.write = function (chunk, enc) {
                try {
                    push(chunk, enc);
                } catch (_) { }
                return origWrite.apply(this, arguments);
            };
            req.end = function (chunk, enc) {
                try {
                    push(chunk, enc);
                    // 空体请求（握手 / 探活）不是事件上报
                    if (chunks.length) {
                        const body = Buffer.concat(chunks);
                        node = selectNode(candidates, body, req.getHeader('x-self-id'));
                        if (requestEntry) bindNode(requestEntry, node);
                        countMessage(node, 'out', body);
                    }
                } catch (_) { }
                return origEnd.apply(this, arguments);
            };
        } catch (_) { }
        return req;
    }

    mod.request = function () {
        return wrapReq(arguments, origRequest);
    };
    mod.get = function () {
        return wrapReq(arguments, origGet);
    };
}

try {
    wrapHttpModule(http, false);
    wrapHttpModule(https, true);
} catch (_) { }

// Node fetch uses undici instead of http.request. Its core diagnostic channel
// supplies the request path before headers are written, including reused sockets.
try {
    diagnostics.channel('undici:client:sendHeaders').subscribe(({ request, socket }) => {
        try {
            const target = parseTarget(request.origin);
            if (!target) return;
            const candidates = matchByHostPort(target.host, target.port, request.path);
            let entry = entryBySocket.get(socket);
            if (!entry) entry = instrumentSocket(socket, candidates, target);
            if (entry) {
                beginRequest(entry, candidates, undefined, request.path);
                entry.viaHttpHook = false;
                entry.viaUndiciHook = true;
            }
        } catch (_) { }
    });
} catch (_) { }

// --- 快照 -------------------------------------------------------------------

function memorySnapshot() {
    try {
        const m = process.memoryUsage();
        return {
            rssBytes: m.rss,
            heapUsedBytes: m.heapUsed,
        };
    } catch (_) {
        return null;
    }
}

function writeContext(context) {
    try {
        const nodes = [];
        for (const node of context.nodes) {
            const c = ensureNode(node);
            nodes.push({
                name: c.name,
                kind: c.kind,
                events_out: c.eventsOut,
                actions_in: c.actionsIn,
                bytes_out: c.bytesOut,
                bytes_in: c.bytesIn,
                errors: c.errors,
                last_activity_at_ms: c.lastActivityAtMs || null,
            });
        }
        const payload = {
            collectedAtMs: Date.now(),
            memory: memorySnapshot(),
            nodes,
        };
        const output = context.statsOut;
        const dir = path.dirname(output);
        fs.mkdirSync(dir, { recursive: true });
        const tmp = output + '.tmp';
        fs.writeFileSync(tmp, JSON.stringify(payload));
        fs.renameSync(tmp, output);
        // 成功后清掉上次写失败痕迹
        try {
            fs.unlinkSync(output + '.err');
        } catch (_) { }
    } catch (err) {
        // 静默吞掉会导致 UI 永远「未注入」；落盘 .err 便于 SSH 排查
        try {
            const dir = path.dirname(context.statsOut);
            fs.mkdirSync(dir, { recursive: true });
            fs.writeFileSync(
                context.statsOut + '.err',
                String((err && err.stack) || err || 'writeSnapshot failed'),
                'utf8',
            );
        } catch (_) { }
    }
}

function writeSnapshot(force = true) {
    // Charge traffic collected before a configuration change to the old owner.
    sampleAllSockets();
    refreshRegistry();
    for (const entry of liveSockets) {
        entry.candidates = entry.address
            ? matchByHostPort(entry.address.host, entry.address.port, entry.requestPath)
            : matchByPort(entry.socket.localPort);
        const canWaitForIdentity = entry.candidates.some(activeNode)
            && (!entry.accountHint || !!selectNode(entry.candidates, null, entry.accountHint));
        const ownerRemoved = entry.node && (!activeNode(entry.node)
            || !entry.candidates.some((node) => node.key === entry.node.key));
        if (ownerRemoved || !canWaitForIdentity) {
            entry.node = null;
            entry.pendingIn = 0;
            entry.pendingOut = 0;
        }
        entry.pending = canWaitForIdentity;
    }
    const now = Date.now();
    for (const context of contexts.values()) {
        if (!context.enabled || (!force && context.lastWrite && now - context.lastWrite < context.intervalMs)) continue;
        writeContext(context);
        context.lastWrite = now;
    }
}

writeSnapshot();
const timer = setInterval(() => writeSnapshot(false), registryDir ? 1000 : intervalMs);
if (timer.unref) timer.unref();

module.exports = {
    disabled: false,
    writeSnapshot,
    counters,
};
