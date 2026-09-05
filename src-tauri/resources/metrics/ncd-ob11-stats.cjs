// ncd-ob11-stats: Desktop-owned OneBot traffic probe (preload via require).
// Counts frames/bytes per configured network node; writes net-stats.json atomically.
// Does not log message bodies. Controlled by env:
//   NCD_METRICS_ENABLED=1
//   NCD_METRICS_OUT=<abs path>
//   NCD_METRICS_INTERVAL_MS=3000
//   NCD_METRICS_NODES_JSON=<json array> OR NCD_METRICS_NODES_PATH=<file>
//
// 只挂 node 核心模块（net / http / https）：NapCat 把 `ws` 打进 bundle、SnowLuma 自带
// `@snowluma/websocket`，任何针对 ws 包的 hook 都拿不到正反向连接。核心模块是两边共同
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

if (process.env.NCD_METRICS_ENABLED !== '1') {
    module.exports = { disabled: true };
    return;
}

const outPath = process.env.NCD_METRICS_OUT;
if (!outPath) {
    module.exports = { disabled: true, reason: 'missing NCD_METRICS_OUT' };
    return;
}

const intervalMs = Math.max(
    1000,
    Math.min(30000, parseInt(process.env.NCD_METRICS_INTERVAL_MS || '3000', 10) || 3000),
);

/** 单帧超过这个大小只记条数不看正文，避免为统计缓冲大图片 */
const MAX_SCAN_PAYLOAD = 64 * 1024;
/** 半包缓冲上限；超了说明解析已经跟丢，放弃该 socket 的帧解析（字节仍算） */
const MAX_FRAME_BUFFER = 512 * 1024;
const WS_OPCODES = new Set([0x0, 0x1, 0x2, 0x8, 0x9, 0xa]);

/** @type {{ name: string, kind: string, listenPort?: number, targetUrl?: string }[]} */
let nodeMap = [];
/** 上次成功加载 nodes 文件的 mtime，用于热更新（NC/SL 连接配置可热改） */
let nodesMapMtimeMs = 0;
/** listenPort -> node */
let portIndex = new Map();
/** 客户端节点解析结果：{ node, host, port, path } */
let targetIndex = [];

/** @type {Map<string, any>} */
const counters = new Map();

function ensureNode(name, kind) {
    let c = counters.get(name);
    if (!c) {
        c = {
            name,
            kind: kind || 'unknown',
            eventsOut: 0,
            actionsIn: 0,
            bytesOut: 0,
            bytesIn: 0,
            errors: 0,
            lastActivityAtMs: 0,
        };
        counters.set(name, c);
    } else if (kind && c.kind === 'unknown') {
        c.kind = kind;
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

function applyNodeMap(next) {
    if (!Array.isArray(next)) return;
    nodeMap = next;
    const ports = new Map();
    const targets = [];
    for (const n of nodeMap) {
        if (!n || !n.name) continue;
        const kind = n.kind || 'unknown';
        ensureNode(String(n.name), kind);
        const listenPort = Number(n.listenPort || n.listen_port || 0);
        if (listenPort > 0) ports.set(listenPort, { name: String(n.name), kind });
        const targetUrl = n.targetUrl || n.target_url;
        if (targetUrl) {
            const t = parseTarget(targetUrl);
            if (t) targets.push({ name: String(n.name), kind, ...t });
        }
    }
    portIndex = ports;
    targetIndex = targets;
}

function loadNodeMapFromEnv(force) {
    try {
        if (process.env.NCD_METRICS_NODES_JSON) {
            if (force || nodeMap.length === 0) {
                applyNodeMap(JSON.parse(process.env.NCD_METRICS_NODES_JSON));
            }
            return;
        }
        const p = process.env.NCD_METRICS_NODES_PATH;
        if (!p) return;
        let st;
        try {
            st = fs.statSync(p);
        } catch (_) {
            return;
        }
        const mtime = st.mtimeMs || 0;
        if (!force && mtime && mtime === nodesMapMtimeMs) return;
        applyNodeMap(JSON.parse(fs.readFileSync(p, 'utf8')));
        nodesMapMtimeMs = mtime;
    } catch (_) {
        /* 保持上一份 map */
    }
}

loadNodeMapFromEnv(true);

function matchByPort(port) {
    if (!port) return null;
    return portIndex.get(Number(port)) || null;
}

function matchByHostPort(host, port, hintPath) {
    if (!port) return null;
    const h = normalizeHost(host);
    const p = Number(port);
    let loose = null;
    for (const t of targetIndex) {
        if (t.port !== p) continue;
        if (t.host !== h) continue;
        if (hintPath && t.path && t.path !== '/' && String(hintPath).startsWith(t.path)) return t;
        if (!loose) loose = t;
    }
    return loose;
}

function matchByUrl(url) {
    const t = parseTarget(url);
    if (!t) return null;
    return matchByHostPort(t.host, t.port, t.path);
}

function touch(node) {
    const c = ensureNode(node.name, node.kind);
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
    if (!node) return;
    const cls = classifyPayload(payload);
    const c = touch(node);
    if (dir === 'out') {
        if (cls === 'event' || cls === 'unknown') c.eventsOut += 1;
    } else if (cls === 'action' || cls === 'unknown') {
        c.actionsIn += 1;
    }
}

function countError(node) {
    if (!node) return;
    const c = ensureNode(node.name, node.kind);
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
    const c = ensureNode(entry.node.name, entry.node.kind);
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
    entry.scanIn = createFrameScanner((payload) => countMessage(entry.node, 'in', payload));
    entry.scanOut = createFrameScanner((payload) => countMessage(entry.node, 'out', payload));
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
function countEventMarkers(node, buf) {
    let idx = buf.indexOf(EVENT_MARKER);
    while (idx !== -1) {
        touch(node).eventsOut += 1;
        idx = buf.indexOf(EVENT_MARKER, idx + EVENT_MARKER.length);
    }
}

function observeOutbound(entry, chunk, encoding) {
    if (!chunk) return;
    const buf = Buffer.isBuffer(chunk)
        ? chunk
        : (typeof chunk === 'string' ? Buffer.from(chunk, typeof encoding === 'string' ? encoding : 'utf8') : null);
    if (!buf || !buf.length) return;

    if (entry.mode === 'http') {
        const consumed = detectUpgrade(entry, buf.subarray(0, Math.min(buf.length, 2048)).toString('latin1'));
        if (consumed >= 0) {
            if (consumed < buf.length) entry.scanOut(buf.subarray(consumed));
            return;
        }
        // SSE 没有帧头可数；SnowLuma 的 HTTP 上报走全局 fetch(undici)，不经 http.request
        // 钩子。两者都只能在字节流上扫事件标记。
        if (entry.node.kind === 'httpSse' || (entry.node.kind === 'httpClient' && !entry.viaHttpHook)) {
            countEventMarkers(entry.node, buf);
        }
        return;
    }
    if (entry.scanOut) entry.scanOut(buf);
}

function observeInbound(entry, chunk) {
    if (!chunk || !chunk.length) return;
    const buf = Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk);
    if (entry.mode === 'http') {
        const consumed = detectUpgrade(entry, buf.subarray(0, Math.min(buf.length, 2048)).toString('latin1'));
        if (consumed >= 0 && consumed < buf.length) entry.scanIn(buf.subarray(consumed));
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
function instrumentSocket(socket, node) {
    if (!socket || !node || socket[INSTRUMENTED]) return;
    try {
        Object.defineProperty(socket, INSTRUMENTED, { value: true, enumerable: false });
    } catch (_) {
        return;
    }

    const entry = {
        socket,
        node,
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
            else if (event === 'error') countError(node);
            else if (event === 'close') {
                sampleBytes(entry);
                liveSockets.delete(entry);
            }
        } catch (_) {
            /* 同上 */
        }
        return origEmit.apply(this, arguments);
    };
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
            const node = matchByPort(socket && socket.localPort);
            if (node) instrumentSocket(socket, node);
        } catch (_) { }
    });

    server.on('request', (req) => {
        try {
            const node = matchByPort(req.socket && req.socket.localPort);
            if (!node) return;
            if (node.kind === 'httpSse' && req.method === 'GET') return; // GET 是订阅流，不是 action
            touch(node).actionsIn += 1;
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
                const node = matchByHostPort(target.host, target.port);
                if (node) instrumentSocket(this, node);
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
            const node = matchByUrl(urlHint);
            if (!node) return req;

            // 包 emit 而不是 on：node 客户端拿 req.listenerCount('upgrade') 决定要不要
            // 销毁一个意料之外的 101 连接，凭空加监听者会把该销毁的 socket 留下来。
            const origReqEmit = req.emit;
            req.emit = function (event, a, b, c) {
                try {
                    if (event === 'socket') {
                        instrumentSocket(a, node);
                        // 这条连接的事件由下面的 req.end 判定，socket 层别再扫一遍
                        const entry = entryBySocket.get(a);
                        if (entry) entry.viaHttpHook = true;
                    } else if (event === 'upgrade') {
                        // 客户端的 101 也走 C++ parser，socket 不发 'data'，只有这里拿得到切换时机
                        instrumentSocket(b, node);
                        const entry = entryBySocket.get(b);
                        if (entry) {
                            enterWsMode(entry);
                            if (c && c.length) entry.scanIn(c);
                        }
                    }
                } catch (_) { }
                return origReqEmit.apply(this, arguments);
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
                    if (chunks.length) countMessage(node, 'out', Buffer.concat(chunks));
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

function writeSnapshot() {
    try {
        // 连接配置热更新：Desktop 改 nodes.json 后无需重启 Bot
        loadNodeMapFromEnv(false);
        sampleAllSockets();
        const nodes = [];
        for (const c of counters.values()) {
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
        const dir = path.dirname(outPath);
        fs.mkdirSync(dir, { recursive: true });
        const tmp = outPath + '.tmp';
        fs.writeFileSync(tmp, JSON.stringify(payload));
        fs.renameSync(tmp, outPath);
        // 成功后清掉上次写失败痕迹
        try {
            fs.unlinkSync(outPath + '.err');
        } catch (_) { }
    } catch (err) {
        // 静默吞掉会导致 UI 永远「未注入」；落盘 .err 便于 SSH 排查
        try {
            const dir = path.dirname(outPath);
            fs.mkdirSync(dir, { recursive: true });
            fs.writeFileSync(
                outPath + '.err',
                String((err && err.stack) || err || 'writeSnapshot failed'),
                'utf8',
            );
        } catch (_) { }
    }
}

writeSnapshot();
const timer = setInterval(writeSnapshot, intervalMs);
if (timer.unref) timer.unref();

module.exports = {
    disabled: false,
    writeSnapshot,
    counters,
};
