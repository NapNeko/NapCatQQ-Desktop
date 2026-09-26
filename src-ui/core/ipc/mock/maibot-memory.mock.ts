// 浏览器预览：麦麦的长期记忆。语义照后端 resources/memory：导入任务随时间推进、导完长出段落和关系；
// 查记忆按类型 / 关键词；删先预览后执行，最近删除能恢复；图谱按度数挑点。

import { makeAppConfigError } from '../../domain/apps/appConfigError';
import type {
    AppInstance,
    MaiBotLearningChat,
    MaiBotLocalTextFile,
    MaiBotMemoryCounts,
    MaiBotMemoryDeleteAction,
    MaiBotMemoryDeleteOp,
    MaiBotMemoryDeleteResult,
    MaiBotMemoryDeleteTarget,
    MaiBotMemoryGraph,
    MaiBotMemoryGraphHit,
    MaiBotMemoryImport,
    MaiBotMemoryImportSetup,
    MaiBotMemoryNodeDetail,
    MaiBotMemoryQuery,
    MaiBotMemoryRecord,
    MaiBotMemoryRecordDetail,
    MaiBotMemoryRecordKind,
    MaiBotMemoryRecordPage,
    MaiBotMemorySource,
    MaiBotMemoryStatus,
    MaiBotMemoryTask,
    MaiBotMemoryTaskAction,
    MaiBotMemoryTaskDetail,
} from '../types';
import { withMockDelay } from './bootstrap.mock';

const CHATS: MaiBotLearningChat[] = [
    { chat_id: 'c0a1', chat_name: '麦麦测试群', platform: 'qq', is_group: true },
    { chat_id: 'c0d4', chat_name: '摸鱼小分队', platform: 'qq', is_group: true },
    { chat_id: 'c0c3', chat_name: '小林的私聊', platform: 'qq', is_group: false },
];

// 来源 → 段落原文
const SEED_PARAGRAPHS: [string, string][] = [
    ['麦麦设定.md', '麦麦是一个普通的大学生，喜欢橘猫和奶茶，最讨厌数学课。'],
    ['麦麦设定.md', '麦麦平时在学校上课，周末最想吃的是火锅。'],
    ['群聊 · 摸鱼小分队', '小林说最近又在玩原神，抽卡又歪了。'],
    ['群聊 · 摸鱼小分队', '阿杰在群里弹了一段吉他，说自己加入摸鱼小分队快一年了。'],
    ['群聊 · 摸鱼小分队', '班长提醒大家摸鱼小分队周末聚餐，小林说要去吃火锅。'],
    ['聊天记录.txt', '猫猫头说家里的橘猫又胖了，最近还迷上了奶茶。'],
    ['聊天记录.txt', '老张在学校教数学，说期末要考函数。'],
    ['聊天记录.txt', '阿杰最近在玩星穹铁道，拉着小林一起。'],
];

// 主语、关系、宾语、出自第几段
const SEED_RELATIONS: [string, string, string, number][] = [
    ['麦麦', '喜欢', '橘猫', 0],
    ['麦麦', '喜欢', '奶茶', 0],
    ['麦麦', '讨厌', '数学', 0],
    ['麦麦', '在上', '学校', 1],
    ['麦麦', '周末想吃', '火锅', 1],
    ['小林', '在玩', '原神', 2],
    ['阿杰', '会弹', '吉他', 3],
    ['阿杰', '加入了', '摸鱼小分队', 3],
    ['班长', '管', '摸鱼小分队', 4],
    ['小林', '加入了', '摸鱼小分队', 4],
    ['小林', '周末去吃', '火锅', 4],
    ['猫猫头', '养了', '橘猫', 5],
    ['猫猫头', '喜欢', '奶茶', 5],
    ['老张', '教', '数学', 6],
    ['老张', '在', '学校', 6],
    ['阿杰', '在玩', '星穹铁道', 7],
    ['阿杰', '朋友', '小林', 7],
    ['麦麦', '朋友', '小林', 2],
];

type Paragraph = { id: string; source: string; content: string; created: number; deleted: boolean };
type Relation = { id: string; s: string; p: string; o: string; para: string; created: number; deleted: boolean };
type Fact = { id: string; key: string; value: string; created: number };
type Task = MaiBotMemoryTask & { names: string[]; texts: string[]; startedAt: number };
type Op = MaiBotMemoryDeleteOp & { paragraphIds: string[]; relationIds: string[] };

type Store = {
    paragraphs: Paragraph[];
    relations: Relation[];
    facts: Fact[];
    tasks: Task[];
    ops: Op[];
    seq: number;
};
const stores = new Map<string, Store>();
const now = () => Date.now() / 1000;
// 一个任务从排队到导完大约 8 秒，走查时来得及截到进度条
const TASK_SECONDS = 8;

function seed(): Store {
    const t = now();
    const paragraphs = SEED_PARAGRAPHS.map(([source, content], i) => ({
        id: `p${String(i + 1).padStart(2, '0')}`,
        source,
        content,
        created: t - 86400 * (8 - i),
        deleted: false,
    }));
    const relations = SEED_RELATIONS.map(([s, p, o, pi], i) => ({
        id: `r${String(i + 1).padStart(2, '0')}`,
        s,
        p,
        o,
        para: paragraphs[pi].id,
        created: paragraphs[pi].created + 60,
        deleted: false,
    }));
    const facts = [
        { id: 'f01', key: '麦麦的生日', value: '3 月 14 日', created: t - 86400 * 3 },
        { id: 'f02', key: '小林的昵称', value: '林林子', created: t - 86400 },
    ];
    const tasks: Task[] = [
        {
            id: 'task-seed-1', source: 'upload', status: 'done', progress: 1, total_chunks: 6, done_chunks: 6, failed_chunks: 0,
            file_count: 2, error: '', created_at: t - 86400 * 2, finished_at: t - 86400 * 2 + 40,
            names: ['麦麦设定.md', '聊天记录.txt'], texts: [], startedAt: 0,
        },
        {
            id: 'task-seed-2', source: 'paste', status: 'done_with_errors', progress: 1, total_chunks: 4, done_chunks: 3,
            failed_chunks: 1, file_count: 1, error: '1 块抽取超时', created_at: t - 3600 * 5, finished_at: t - 3600 * 5 + 30,
            names: ['群聊 · 摸鱼小分队'], texts: [], startedAt: 0,
        },
    ];
    return { paragraphs, relations, facts, tasks, ops: [], seq: 100 };
}

function store(inst: AppInstance): Store {
    if (inst.state !== 'running') throw makeAppConfigError('not_running', '启动麦麦后才能用');
    let s = stores.get(inst.id);
    if (!s) {
        s = seed();
        stores.set(inst.id, s);
    }
    advance(s);
    return s;
}

/** 按时间推进进行中的任务；导完的把内容切成段落、随手抽几条关系 */
function advance(s: Store) {
    const t = now();
    for (const task of s.tasks) {
        if (task.status !== 'queued' && task.status !== 'running' && task.status !== 'preparing') continue;
        const progress = Math.min(1, (t - task.startedAt) / TASK_SECONDS);
        task.progress = progress;
        task.status = progress <= 0.05 ? 'queued' : progress < 1 ? 'running' : 'done';
        task.done_chunks = Math.round(task.total_chunks * progress);
        if (progress >= 1) {
            task.finished_at = t;
            task.texts.forEach((text, i) => {
                const source = task.names[i] ?? task.names[0];
                for (const line of text.split(/\n+/).map((l) => l.trim()).filter(Boolean).slice(0, 6)) {
                    const id = `p${s.seq++}`;
                    s.paragraphs.push({ id, source, content: line, created: t, deleted: false });
                    // 「A 喜欢 B」这种句式抽一条关系，够预览用
                    const m = /^(.{1,6}?)(喜欢|讨厌|认识|养了|在玩)(.{1,8}?)[。！，,.!]?$/.exec(line);
                    if (m) s.relations.push({ id: `r${s.seq++}`, s: m[1], p: m[2], o: m[3], para: id, created: t, deleted: false });
                }
            });
        }
    }
}

const RECORD_KINDS: MaiBotMemoryRecordKind[] = ['paragraph', 'entity', 'relation', 'fact'];
const trim = (text: string, n: number) => (text.length > n ? `${text.slice(0, n)}…` : text);

function entities(s: Store): { name: string; mentions: number; created: number }[] {
    const map = new Map<string, { name: string; mentions: number; created: number }>();
    for (const r of s.relations.filter((x) => !x.deleted)) {
        for (const name of [r.s, r.o]) {
            const e = map.get(name) ?? { name, mentions: 0, created: r.created };
            e.mentions += 1;
            e.created = Math.min(e.created, r.created);
            map.set(name, e);
        }
    }
    return [...map.values()].sort((a, b) => b.mentions - a.mentions);
}

const entityId = (name: string) => `e-${encodeURIComponent(name)}`;
const entityName = (id: string) => decodeURIComponent(id.replace(/^e-/, ''));

function paragraphRecord(p: Paragraph): MaiBotMemoryRecord {
    return {
        kind: 'paragraph', id: p.id, title: trim(p.content, 60), summary: p.content, source: p.source,
        status: p.deleted ? 'deleted' : 'active', active: !p.deleted, created_at: p.created, knowledge_type: 'mixed',
    };
}

function relationRecord(r: Relation): MaiBotMemoryRecord {
    return {
        kind: 'relation', id: r.id, title: `${r.s} ${r.p} ${r.o}`, summary: '置信度 0.86', source: r.para,
        status: r.deleted ? 'inactive' : 'active', active: !r.deleted, created_at: r.created, confidence: 0.86,
    };
}

function entityRecord(e: { name: string; mentions: number; created: number }): MaiBotMemoryRecord {
    return {
        kind: 'entity', id: entityId(e.name), title: e.name, summary: `由 ${e.mentions} 条有效段落支撑`, source: '',
        status: 'active', active: true, created_at: e.created, mentions: e.mentions,
    };
}

function factRecord(f: Fact): MaiBotMemoryRecord {
    return { kind: 'fact', id: f.id, title: `${f.key}: ${f.value}`, summary: 'person', source: '', status: 'active', active: true, created_at: f.created };
}

function allRecords(s: Store, includeInactive: boolean): MaiBotMemoryRecord[] {
    return [
        ...s.paragraphs.filter((p) => includeInactive || !p.deleted).map(paragraphRecord),
        ...entities(s).map(entityRecord),
        ...s.relations.filter((r) => includeInactive || !r.deleted).map(relationRecord),
        ...s.facts.map(factRecord),
    ].sort((a, b) => (b.created_at ?? 0) - (a.created_at ?? 0));
}

/** 删一批会连带删到什么：段落连着它抽出来的关系，实体连着提到它的关系 */
function reach(s: Store, t: MaiBotMemoryDeleteTarget): { paragraphs: Paragraph[]; relations: Relation[] } {
    const live = s.paragraphs.filter((p) => !p.deleted);
    const paragraphs =
        t.kind === 'paragraph' ? live.filter((p) => t.ids.includes(p.id)) : t.kind === 'source' ? live.filter((p) => t.ids.includes(p.source)) : [];
    const names = t.kind === 'entity' ? t.ids.map(entityName) : [];
    const relations = s.relations.filter(
        (r) =>
            !r.deleted &&
            ((t.kind === 'relation' && t.ids.includes(r.id)) ||
                paragraphs.some((p) => p.id === r.para) ||
                names.includes(r.s) ||
                names.includes(r.o)),
    );
    return { paragraphs, relations };
}

const counts = (p: number, r: number, e = 0, src = 0): MaiBotMemoryCounts => ({ paragraphs: p, relations: r, entities: e, sources: src });
const strip = ({ names: _n, texts: _t, startedAt: _s, ...task }: Task): MaiBotMemoryTask => task;
const isText = (p: string) => /\.(txt|md|json)$/i.test(p);
const baseName = (p: string) => p.split(/[\\/]/).pop() ?? p;

export const mockMaiBotMemory = {
    status(inst: AppInstance): Promise<MaiBotMemoryStatus> {
        store(inst);
        return withMockDelay({ state: 'ready', message: '', notes: [] });
    },

    importSetup(inst: AppInstance): Promise<MaiBotMemoryImportSetup> {
        store(inst);
        return withMockDelay({ limits: { max_file_mb: 20, max_files: 200, max_paste_chars: 200_000, poll_ms: 1000 }, chats: CHATS });
    },

    importMemory(inst: AppInstance, req: MaiBotMemoryImport): Promise<MaiBotMemoryTask> {
        const s = store(inst);
        const t = now();
        const names = req.op === 'paste' ? [req.name.trim() || `粘贴 ${new Date().toLocaleTimeString('zh-CN')}`] : req.paths.map(baseName);
        const texts = req.op === 'paste' ? [req.content] : names.map((n) => `${n.replace(/\.\w+$/, '')}里说麦麦喜欢小狗。`);
        const task: Task = {
            id: `task-${s.seq++}`, source: req.op === 'paste' ? 'paste' : 'upload', status: 'queued', progress: 0,
            total_chunks: Math.max(2, texts.join('').length % 7), done_chunks: 0, failed_chunks: 0, file_count: names.length,
            error: '', created_at: t, names, texts, startedAt: t,
        };
        s.tasks.unshift(task);
        return withMockDelay(strip(task));
    },

    tasks(inst: AppInstance): Promise<MaiBotMemoryTask[]> {
        return withMockDelay(store(inst).tasks.map(strip));
    },

    task(inst: AppInstance, id: string): Promise<MaiBotMemoryTaskDetail> {
        const task = store(inst).tasks.find((x) => x.id === id);
        if (!task) return Promise.reject(makeAppConfigError('invalid', '这个导入任务没了（麦麦重启过就会清空）'));
        const per = Math.max(1, Math.round(task.total_chunks / task.names.length));
        return withMockDelay({
            task: strip(task),
            files: task.names.map((name, i) => ({
                name, status: task.status, progress: task.progress, total_chunks: per, done_chunks: Math.round(per * task.progress),
                failed_chunks: i === 0 ? task.failed_chunks : 0, error: i === 0 ? task.error : '', warnings: [],
            })),
        });
    },

    taskAction(inst: AppInstance, a: MaiBotMemoryTaskAction): Promise<MaiBotMemoryTask> {
        const s = store(inst);
        const task = s.tasks.find((x) => x.id === a.id);
        if (!task) return Promise.reject(makeAppConfigError('invalid', '这个导入任务没了（麦麦重启过就会清空）'));
        if (a.op === 'cancel') {
            task.status = 'cancelled';
            task.finished_at = now();
            return withMockDelay(strip(task));
        }
        const t = now();
        const retry: Task = { ...task, id: `task-${s.seq++}`, status: 'queued', progress: 0, done_chunks: 0, failed_chunks: 0, error: '', created_at: t, finished_at: undefined, startedAt: t };
        s.tasks.unshift(retry);
        return withMockDelay(strip(retry));
    },

    records(inst: AppInstance, q: MaiBotMemoryQuery): Promise<MaiBotMemoryRecordPage> {
        const kw = q.search.trim().toLowerCase();
        const kinds = q.kinds.length ? q.kinds : RECORD_KINDS;
        const items = allRecords(store(inst), q.include_inactive)
            .filter((r) => kinds.includes(r.kind) && (!kw || `${r.title} ${r.summary} ${r.source}`.toLowerCase().includes(kw)))
            .slice(0, q.limit);
        const n = (k: MaiBotMemoryRecordKind) => items.filter((r) => r.kind === k).length;
        return withMockDelay({ items, counts: { paragraph: n('paragraph'), entity: n('entity'), relation: n('relation'), fact: n('fact') } });
    },

    record(inst: AppInstance, kind: MaiBotMemoryRecordKind, id: string): Promise<MaiBotMemoryRecordDetail> {
        const s = store(inst);
        const record = allRecords(s, true).find((r) => r.kind === kind && r.id === id);
        if (!record) return Promise.reject(makeAppConfigError('invalid', '这条记忆没找到'));
        const rels =
            kind === 'relation' ? s.relations.filter((r) => r.id === id)
            : kind === 'paragraph' ? s.relations.filter((r) => r.para === id)
            : kind === 'entity' ? s.relations.filter((r) => r.s === entityName(id) || r.o === entityName(id))
            : [];
        const paras = s.paragraphs.filter((p) => rels.some((r) => r.para === p.id) || (kind === 'paragraph' && p.id === id));
        const names = new Set(rels.flatMap((r) => [r.s, r.o]));
        return withMockDelay({
            record,
            paragraphs: paras.filter((p) => p.id !== id).map(paragraphRecord),
            entities: entities(s).filter((e) => names.has(e.name) && entityId(e.name) !== id).map(entityRecord),
            relations: rels.filter((r) => r.id !== id).map(relationRecord),
            facts: [],
        });
    },

    sources(inst: AppInstance): Promise<MaiBotMemorySource[]> {
        const map = new Map<string, MaiBotMemorySource>();
        for (const p of store(inst).paragraphs.filter((x) => !x.deleted)) {
            const src = map.get(p.source) ?? { source: p.source, paragraphs: 0, last_updated: p.created };
            src.paragraphs += 1;
            src.last_updated = Math.max(src.last_updated ?? 0, p.created);
            map.set(p.source, src);
        }
        return withMockDelay([...map.values()].sort((a, b) => (b.last_updated ?? 0) - (a.last_updated ?? 0)));
    },

    deleteAction(inst: AppInstance, a: MaiBotMemoryDeleteAction): Promise<MaiBotMemoryDeleteResult> {
        const s = store(inst);
        if (a.op === 'restore') {
            const op = s.ops.find((o) => o.id === a.operation_id);
            if (!op || op.status === 'restored') return Promise.reject(makeAppConfigError('invalid', '这次删除已经恢复过了'));
            s.paragraphs.forEach((p) => op.paragraphIds.includes(p.id) && (p.deleted = false));
            s.relations.forEach((r) => op.relationIds.includes(r.id) && (r.deleted = false));
            op.status = 'restored';
            op.restored_at = now();
            return withMockDelay({ counts: counts(0, 0), samples: [], operation_id: op.id, message: '恢复了' });
        }
        const hit = reach(s, a.target);
        const c = counts(hit.paragraphs.length, hit.relations.length, a.target.kind === 'entity' ? a.target.ids.length : 0, a.target.kind === 'source' ? a.target.ids.length : 0);
        if (hit.paragraphs.length + hit.relations.length === 0) return Promise.reject(makeAppConfigError('invalid', '未命中可删除内容'));
        if (a.op === 'preview') {
            const samples = [
                ...hit.paragraphs.map((p) => ({ kind: 'paragraph', label: p.source, preview: trim(p.content, 80) })),
                ...hit.relations.map((r) => ({ kind: 'relation', label: `${r.s} ${r.p} ${r.o}`, preview: `${r.s} ${r.p} ${r.o}` })),
            ];
            return withMockDelay({ counts: c, samples, operation_id: '', message: '' });
        }
        hit.paragraphs.forEach((p) => (p.deleted = true));
        hit.relations.forEach((r) => (r.deleted = true));
        const op: Op = {
            id: `op-${s.seq++}`, mode: a.target.kind, status: 'executed', reason: 'desktop', created_at: now(), counts: c,
            paragraphIds: hit.paragraphs.map((p) => p.id), relationIds: hit.relations.map((r) => r.id),
        };
        s.ops.unshift(op);
        const parts = [c.paragraphs && `${c.paragraphs} 段`, c.relations && `${c.relations} 条关系`].filter(Boolean);
        return withMockDelay({ counts: c, samples: [], operation_id: op.id, message: `删掉了 ${parts.join('、')}` });
    },

    deleteOps(inst: AppInstance): Promise<MaiBotMemoryDeleteOp[]> {
        return withMockDelay(store(inst).ops.map(({ paragraphIds: _p, relationIds: _r, ...op }) => op));
    },

    graph(inst: AppInstance, maxNodes: number): Promise<MaiBotMemoryGraph> {
        const s = store(inst);
        const live = s.relations.filter((r) => !r.deleted);
        const pairs = new Map<string, { source: string; target: string; label: string[]; relations: number }>();
        for (const r of live) {
            const key = `${r.s}\u0000${r.o}`;
            const e = pairs.get(key) ?? { source: r.s, target: r.o, label: [], relations: 0 };
            e.label.push(r.p);
            e.relations += 1;
            pairs.set(key, e);
        }
        const edges = [...pairs.values()].map((e) => ({ source: e.source, target: e.target, label: e.label.join('、'), relations: e.relations, evidence: e.relations }));
        const degree = new Map<string, number>();
        for (const e of edges) for (const n of [e.source, e.target]) degree.set(n, (degree.get(n) ?? 0) + 1);
        const nodes = [...degree.entries()]
            .map(([id, d]) => ({ id, degree: d }))
            .sort((a, b) => b.degree - a.degree || a.id.localeCompare(b.id))
            .slice(0, maxNodes);
        const kept = new Set(nodes.map((n) => n.id));
        return withMockDelay({
            nodes,
            edges: edges.filter((e) => kept.has(e.source) && kept.has(e.target)),
            total_nodes: degree.size,
            total_edges: edges.length,
        });
    },

    graphNode(inst: AppInstance, id: string): Promise<MaiBotMemoryNodeDetail> {
        const s = store(inst);
        const rels = s.relations.filter((r) => !r.deleted && (r.s === id || r.o === id));
        const paras = s.paragraphs.filter((p) => !p.deleted && rels.some((r) => r.para === p.id));
        return withMockDelay({
            id,
            hash: entityId(id),
            mentions: paras.length,
            relations: rels.map((r) => ({ hash: r.id, subject: r.s, predicate: r.p, object: r.o, confidence: 0.86, paragraphs: 1 })),
            paragraphs: paras.map((p) => ({ hash: p.id, preview: p.content, source: p.source, created_at: p.created })),
        });
    },

    graphSearch(inst: AppInstance, q: string): Promise<MaiBotMemoryGraphHit[]> {
        const kw = q.trim();
        if (!kw) return withMockDelay([]);
        const s = store(inst);
        const hits: MaiBotMemoryGraphHit[] = [
            ...entities(s).filter((e) => e.name.includes(kw)).map((e) => ({ kind: 'entity', title: e.name, node: e.name })),
            ...s.relations.filter((r) => !r.deleted && r.p.includes(kw)).map((r) => ({ kind: 'relation', title: `${r.s} ${r.p} ${r.o}`, node: r.s })),
        ];
        return withMockDelay(hits.slice(0, 20));
    },

    pickFiles(): Promise<string[]> {
        return withMockDelay([
            'C:/Users/me/Documents/麦麦的新设定.md',
            'C:/Users/me/Documents/上周群聊.txt',
            'C:/Users/me/Documents/旧笔记.txt',
            'C:/Users/me/Documents/截图.png',
        ]);
    },

    localTexts(paths: string[]): Promise<MaiBotLocalTextFile[]> {
        return withMockDelay(
            paths.map((path, i) => {
                const name = baseName(path);
                if (!isText(path)) return { path, name, size: 48_213, problem: '只收 txt / md / json' };
                if (name.includes('旧')) return { path, name, size: 9_120, problem: '不是 UTF-8 编码，另存为 UTF-8 再导' };
                return { path, name, size: 3_400 + i * 5_120 };
            }),
        );
    },
};
