// ncd-ob11-stats: Desktop-owned OneBot traffic probe (preload via require).
// Counts frames/bytes per configured network node; writes net-stats.json atomically.
// Does not log message bodies. Controlled by env:
//   NCD_METRICS_ENABLED=1
//   NCD_METRICS_OUT=<abs path>
//   NCD_METRICS_INTERVAL_MS=3000
//   NCD_METRICS_NODES_JSON=<json array> OR NCD_METRICS_NODES_PATH=<file>

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

/** @type {{ name: string, kind: string, listenPort?: number, targetUrl?: string }[]} */
let nodeMap = [];
/** 上次成功加载 nodes 文件的 mtime，用于热更新（NC/SL 连接配置可热改） */
let nodesMapMtimeMs = 0;

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

function applyNodeMap(next) {
    if (!Array.isArray(next)) return;
    nodeMap = next;
    for (const n of nodeMap) {
        if (n && n.name) ensureNode(String(n.name), n.kind || 'unknown');
    }
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
    for (const n of nodeMap) {
        if (n.listenPort && Number(n.listenPort) === Number(port)) return n;
    }
    return null;
}

function matchByUrl(url) {
    if (!url) return null;
    const s = String(url);
    for (const n of nodeMap) {
        if (n.targetUrl && s.indexOf(String(n.targetUrl)) !== -1) return n;
    }
    // host:port fallback
    try {
        const u = new URL(s);
        const port = u.port || (u.protocol === 'https:' ? '443' : '80');
        for (const n of nodeMap) {
            if (!n.targetUrl) continue;
            if (String(n.targetUrl).indexOf(u.hostname) !== -1 && String(n.targetUrl).indexOf(port) !== -1) {
                return n;
            }
        }
    } catch (_) { }
    return null;
}

function bumpOut(node, bytes, isEvent) {
    if (!node) return;
    const c = ensureNode(node.name, node.kind);
    if (isEvent) c.eventsOut += 1;
    c.bytesOut += bytes || 0;
    c.lastActivityAtMs = Date.now();
}

function bumpIn(node, bytes, isAction) {
    if (!node) return;
    const c = ensureNode(node.name, node.kind);
    if (isAction) c.actionsIn += 1;
    c.bytesIn += bytes || 0;
    c.lastActivityAtMs = Date.now();
}

function classifyPayload(text) {
    if (!text || typeof text !== 'string') return { event: false, action: false };
    const t = text.trim();
    if (!t) return { event: false, action: false };
    try {
        const j = JSON.parse(t);
        if (j && typeof j === 'object') {
            if (j.post_type || j.message_type || j.notice_type || j.request_type || j.meta_event_type) {
                return { event: true, action: false };
            }
            if (j.action) return { event: false, action: true };
        }
    } catch (_) {
        if (t.indexOf('"post_type"') !== -1) return { event: true, action: false };
        if (t.indexOf('"action"') !== -1) return { event: false, action: true };
    }
    return { event: false, action: false };
}

function byteLen(data) {
    if (data == null) return 0;
    if (Buffer.isBuffer(data)) return data.length;
    if (typeof data === 'string') return Buffer.byteLength(data);
    try {
        return Buffer.byteLength(String(data));
    } catch (_) {
        return 0;
    }
}

// --- http(s).request / get ---
function wrapHttpModule(mod, isHttps) {
    const origRequest = mod.request;
    const origGet = mod.get;

    function wrapReq(args, orig) {
        const req = orig.apply(mod, args);
        try {
            let urlHint = '';
            const opts = args[0];
            if (typeof opts === 'string') urlHint = opts;
            else if (opts && typeof opts === 'object') {
                const proto = isHttps ? 'https:' : 'http:';
                const host = opts.hostname || opts.host || '127.0.0.1';
                const port = opts.port || (isHttps ? 443 : 80);
                const p = opts.path || '/';
                urlHint = `${proto}//${host}:${port}${p}`;
            }
            const node = matchByUrl(urlHint);
            const chunks = [];
            const origWrite = req.write;
            const origEnd = req.end;
            req.write = function (chunk, enc, cb) {
                if (chunk) chunks.push(Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk, enc));
                return origWrite.apply(this, arguments);
            };
            req.end = function (chunk, enc, cb) {
                if (chunk && typeof chunk !== 'function') {
                    chunks.push(Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk, enc));
                }
                const body = Buffer.concat(chunks.length ? chunks : [Buffer.alloc(0)]);
                const text = body.toString('utf8');
                const cls = classifyPayload(text);
                bumpOut(node, body.length, cls.event || !cls.action);
                if (cls.action) bumpOut(node, 0, false);
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

// --- net.Server listen port tracking for inbound ---
const serverPorts = new WeakMap();
try {
    const origListen = net.Server.prototype.listen;
    net.Server.prototype.listen = function () {
        const server = this;
        server.once('listening', () => {
            try {
                const addr = server.address();
                if (addr && typeof addr === 'object' && addr.port) {
                    serverPorts.set(server, addr.port);
                }
            } catch (_) { }
        });
        return origListen.apply(this, arguments);
    };
} catch (_) { }

// Best-effort ws hook if 'ws' is loadable later
function tryHookWs() {
    try {
        const wsPath = require.resolve('ws');
        const ws = require(wsPath);
        if (!ws || !ws.WebSocket) return;
        const Orig = ws.WebSocket;
        function Wrapped(url, protocols, options) {
            const socket = new Orig(url, protocols, options);
            const node = matchByUrl(String(url || ''));
            const origSend = socket.send.bind(socket);
            socket.send = function (data, opts, cb) {
                const n = byteLen(data);
                const text = Buffer.isBuffer(data) ? data.toString('utf8') : String(data || '');
                const cls = classifyPayload(text);
                bumpOut(node, n, cls.event || !cls.action);
                return origSend(data, opts, cb);
            };
            socket.on('message', (data) => {
                const n = byteLen(data);
                const text = Buffer.isBuffer(data) ? data.toString('utf8') : String(data || '');
                const cls = classifyPayload(text);
                bumpIn(node, n, cls.action || !cls.event);
            });
            return socket;
        }
        Wrapped.prototype = Orig.prototype;
        Object.assign(Wrapped, Orig);
        ws.WebSocket = Wrapped;
        if (ws.default) ws.default = Wrapped;
    } catch (_) { }
}

tryHookWs();
setTimeout(tryHookWs, 2000);

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
        const nodes = [];
        for (const c of counters.values()) {
            nodes.push({
                name: c.name,
                kind: c.kind,
                eventsOut: c.eventsOut,
                actionsIn: c.actionsIn,
                bytesOut: c.bytesOut,
                bytesIn: c.bytesIn,
                errors: c.errors,
                lastActivityAtMs: c.lastActivityAtMs || null,
            });
        }
        // Map probe camelCase to domain serde (snake via Desktop parser also accepts both)
        const payload = {
            collectedAtMs: Date.now(),
            memory: memorySnapshot(),
            nodes: nodes.map((n) => ({
                name: n.name,
                kind: n.kind,
                events_out: n.eventsOut,
                actions_in: n.actionsIn,
                bytes_out: n.bytesOut,
                bytes_in: n.bytesIn,
                errors: n.errors,
                last_activity_at_ms: n.lastActivityAtMs,
            })),
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
