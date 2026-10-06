// 浏览器预览：麦麦学到的行为。语义照后端 resources/behavior：只读；分页、搜索、
// 按聊天 / 在用停用 / 怎么学到的筛、四种排序；详情带观察和反馈记录，新的在前。

import { makeAppConfigError } from '../../domain/apps/appConfigError';
import type {
    AppInstance,
    MaiBotBehavior,
    MaiBotBehaviorActor,
    MaiBotBehaviorChat,
    MaiBotBehaviorDetail,
    MaiBotBehaviorEvidence,
    MaiBotBehaviorFeedback,
    MaiBotBehaviorOverview,
    MaiBotBehaviorPage,
    MaiBotBehaviorQuery,
    MaiBotBehaviorTag,
    MaiBotBehaviorTagKind,
} from '../types';
import { withMockDelay } from './bootstrap.mock';

const CHATS: Omit<MaiBotBehaviorChat, 'count'>[] = [
    { chat_id: 'c0a1', chat_name: '麦麦测试群', is_group: true },
    { chat_id: 'c0b2', chat_name: '原神交流群', is_group: true },
    { chat_id: 'c0c3', chat_name: '小林的私聊', is_group: false },
    { chat_id: '__global__', chat_name: '全局行为', is_group: false },
];

type Seed = {
    chat: number;
    scene: [MaiBotBehaviorTagKind, string, number][];
    actor: MaiBotBehaviorActor;
    action: string;
    outcome: string;
    /** 见过、用过、成、败、分 */
    stats: [number, number, number, number, number];
    enabled?: boolean;
};

const SEEDS: Seed[] = [
    {
        chat: 1,
        scene: [
            ['domain', '游戏 / 抽卡', 0.6],
            ['attitude', '吐槽', 0.25],
            ['need', '共鸣', 0.15],
        ],
        actor: 'maibot',
        action: '接一句「又歪了是吧，懂的都懂」',
        outcome: '对方回了个哈哈，接着聊了下去',
        stats: [4, 6, 4, 1, 3.2],
    },
    {
        chat: 0,
        scene: [
            ['need', '安慰', 0.5],
            ['attitude', '低落', 0.3],
            ['domain', '考试', 0.2],
        ],
        actor: 'others',
        action: '先问一句「怎么了」，再说「抱抱」',
        outcome: '对方说了原因，情绪好了一些',
        stats: [3, 2, 2, 0, 2.4],
    },
    {
        chat: 0,
        scene: [
            ['domain', '深夜 / 熬夜', 0.5],
            ['attitude', '闲聊', 0.5],
        ],
        actor: 'maibot',
        action: '催一句「都几点了还不睡」',
        outcome: '对方说马上睡，之后没再回',
        stats: [2, 3, 1, 1, 0.4],
    },
    {
        chat: 1,
        scene: [
            ['need', '求推荐', 0.6],
            ['domain', '游戏', 0.4],
        ],
        actor: 'others',
        action: '直接列两三个游戏，每个带一句理由',
        outcome: '提问的人挑了一个去试',
        stats: [5, 1, 1, 0, 1.8],
    },
    {
        chat: 0,
        scene: [
            ['attitude', '阴阳怪气', 0.6],
            ['domain', '争论', 0.4],
        ],
        actor: 'maibot',
        action: '顺着说「你说得对」',
        outcome: '对方没再追着吵',
        stats: [2, 4, 2, 2, -0.8],
    },
    {
        chat: 0,
        scene: [
            ['domain', '报错 / 技术问题', 0.7],
            ['need', '求助', 0.3],
        ],
        actor: 'group',
        action: '大家先让提问的人把报错截图发出来',
        outcome: '问题很快被定位了',
        stats: [6, 0, 0, 0, 0.6],
    },
    {
        chat: 0,
        scene: [
            ['attitude', '冷场', 0.7],
            ['domain', '闲聊', 0.3],
        ],
        actor: 'maibot',
        action: '抛个新话题「你们午饭吃的啥」',
        outcome: '没人接，又冷场了',
        stats: [1, 5, 0, 4, -6],
        enabled: false,
    },
    {
        chat: 1,
        scene: [
            ['need', '被夸', 0.5],
            ['attitude', '开心', 0.5],
        ],
        actor: 'maibot',
        action: '回「嘿嘿 被发现了」',
        outcome: '对方回了个表情',
        stats: [3, 7, 5, 0, 5.5],
    },
    {
        chat: 0,
        scene: [
            ['domain', '天气', 0.8],
            ['attitude', '抱怨', 0.2],
        ],
        actor: 'others',
        action: '接一句「今天是真的热，空调续命」',
        outcome: '好几个人跟着聊起来',
        stats: [2, 0, 0, 0, 0.2],
    },
    {
        chat: 1,
        scene: [
            ['need', '求资源', 0.6],
            ['attitude', '着急', 0.4],
        ],
        actor: 'maibot',
        action: '说「我这边没有，群里谁有可以发一下」',
        outcome: '有人把链接发出来了',
        stats: [1, 2, 1, 0, 1.2],
    },
    {
        chat: 0,
        scene: [
            ['domain', '生日', 0.7],
            ['attitude', '开心', 0.3],
        ],
        actor: 'group',
        action: '大家一起刷「生日快乐」',
        outcome: '寿星很开心，发了红包',
        stats: [3, 1, 1, 0, 2.0],
    },
    {
        chat: 1,
        scene: [
            ['attitude', '被 @ 但没话说', 0.6],
            ['need', '回应', 0.4],
        ],
        actor: 'maibot',
        action: '只回一个「？」',
        outcome: '对方觉得被敷衍，有点不高兴',
        stats: [1, 3, 0, 3, -4.6],
        enabled: false,
    },
    {
        chat: 0,
        scene: [
            ['domain', '作业', 0.6],
            ['attitude', '吐槽', 0.4],
        ],
        actor: 'maibot',
        action: '回「同款，一起摆烂」',
        outcome: '对方笑着说那先玩一会',
        stats: [2, 3, 2, 0, 2.6],
    },
    {
        chat: 0,
        scene: [
            ['domain', '猫 / 宠物', 0.7],
            ['attitude', '可爱', 0.3],
        ],
        actor: 'others',
        action: '问是谁家的猫、几岁了',
        outcome: '发图的人又发了好几张',
        stats: [4, 2, 2, 0, 3.0],
    },
    {
        chat: 1,
        scene: [
            ['need', '分享好消息', 0.5],
            ['attitude', '开心', 0.5],
        ],
        actor: 'maibot',
        action: '说「恭喜恭喜！请客吗」',
        outcome: '对方说下次一定',
        stats: [2, 2, 1, 0, 1.0],
    },
    {
        chat: 3,
        scene: [
            ['domain', '问身份', 0.7],
            ['attitude', '好奇', 0.3],
        ],
        actor: 'maibot',
        action: '说「我只是一个普通的大学生啦」，顺手转开话题',
        outcome: '对方没再追问',
        stats: [3, 6, 4, 1, 4.1],
    },
    {
        chat: 2,
        scene: [
            ['need', '倾诉', 0.6],
            ['attitude', '低落', 0.4],
        ],
        actor: 'maibot',
        action: '多问一句「后来呢」，让对方接着说',
        outcome: '对方说了很多，最后说好受多了',
        stats: [2, 3, 3, 0, 4.4],
    },
    {
        chat: 2,
        scene: [
            ['domain', '追番', 0.6],
            ['attitude', '兴奋', 0.4],
        ],
        actor: 'others',
        action: '安利番的时候先说最喜欢哪一集',
        outcome: '话题聊得更具体了',
        stats: [1, 0, 0, 0, 0],
    },
];

const WINS = ['对方接着聊下去了', '对方回了哈哈', '气氛轻松了不少'];
const LOSSES = ['没人接话', '对方有点不高兴', '对方说别敷衍'];

type Store = { items: MaiBotBehavior[] };
const stores = new Map<string, Store>();

function store(inst: AppInstance): Store {
    if (inst.state !== 'running') throw makeAppConfigError('not_running', '启动麦麦后才能用');
    let s = stores.get(inst.id);
    if (!s) {
        const now = Date.now() / 1000;
        const items = SEEDS.map((seed, i): MaiBotBehavior => {
            const [seen, used, succeeded, failed, score] = seed.stats;
            const chat = CHATS[seed.chat];
            const scene: MaiBotBehaviorTag[] = seed.scene.map(([kind, label, weight]) => ({
                kind,
                label,
                weight,
            }));
            const active = now - (i * 7 + 2) * 3600 * 3;
            return {
                id: 300 + i,
                chat_id: chat.chat_id,
                chat_name: chat.chat_name,
                scene,
                actor: seed.actor,
                self_reflection: seed.actor === 'maibot',
                action: seed.action,
                outcome: seed.outcome,
                seen,
                used,
                succeeded,
                failed,
                score,
                enabled: seed.enabled ?? true,
                active_at: active,
                last_feedback_at: used > 0 ? active - 1800 : undefined,
            };
        });
        s = { items };
        stores.set(inst.id, s);
    }
    return s;
}

const SORT_KEY: Record<MaiBotBehaviorQuery['sort'], (b: MaiBotBehavior) => number> = {
    recent: (b) => b.active_at ?? 0,
    score: (b) => b.score,
    seen: (b) => b.seen,
    used: (b) => b.used,
};

function feedbackOf(b: MaiBotBehavior): MaiBotBehaviorFeedback[] {
    const base = b.last_feedback_at ?? b.active_at ?? Date.now() / 1000;
    // 旧的在前排好，最后整体倒过来：和后端一样新的在前
    const events: MaiBotBehaviorFeedback[] = [];
    for (let k = 0; k < Math.min(b.succeeded, 4); k++) {
        events.push({
            kind: 'success',
            delta: 1,
            reason: WINS[k % WINS.length],
            outcome: b.outcome,
        });
    }
    for (let k = 0; k < Math.min(b.failed, 3); k++) {
        events.push({
            kind: 'failure',
            delta: -1.2,
            reason: LOSSES[k % LOSSES.length],
            outcome: '没聊开',
        });
    }
    if (b.used > b.succeeded + b.failed) {
        events.push({
            kind: 'partial',
            delta: 0.4,
            reason: '对方回了，但没聊开',
            outcome: '回了一句就没下文了',
        });
    }
    if (b.used > 0 && b.succeeded === 0) {
        events.push({
            kind: 'decay',
            delta: -0.25,
            reason: '被选择后长期没有成功反馈，降低后续抽样权重。',
            outcome: '',
        });
    }
    if (!b.enabled) {
        events.push({
            kind: 'disabled',
            delta: 0,
            reason: '长期缺少有效强化或负反馈过多，暂时停止作为行为表现候选。',
            outcome: '',
        });
    }
    return events
        .map((e, k) => ({ ...e, at: base - (events.length - 1 - k) * 86400 * 1.5 }))
        .reverse();
}

function evidenceOf(b: MaiBotBehavior): MaiBotBehaviorEvidence[] {
    const base = b.active_at ?? Date.now() / 1000;
    return Array.from({ length: Math.min(b.seen, 4) }, (_, k) => ({
        action: b.action,
        outcome: b.outcome,
        actor: b.actor,
        messages: 2 + ((b.id + k) % 4),
        at: base - k * 86400 * 1.3,
    }));
}

export const mockMaiBotBehaviors = {
    list(inst: AppInstance, q: MaiBotBehaviorQuery): Promise<MaiBotBehaviorPage> {
        const s = store(inst);
        const kw = q.search.trim().toLowerCase();
        const rows = s.items
            .filter(
                (b) =>
                    (!q.chat_id || b.chat_id === q.chat_id) &&
                    (q.filter === 'all' || (q.filter === 'enabled') === b.enabled) &&
                    (q.origin === 'all' ||
                        (q.origin === 'self_reflection') === b.self_reflection) &&
                    (!kw ||
                        [b.action, b.outcome, b.chat_name, ...b.scene.map((t) => t.label)].some(
                            (v) => v.toLowerCase().includes(kw),
                        )),
            )
            .sort((a, b) => SORT_KEY[q.sort](b) - SORT_KEY[q.sort](a));
        const start = (Math.max(1, q.page) - 1) * q.page_size;
        return withMockDelay({ total: rows.length, items: rows.slice(start, start + q.page_size) });
    },

    overview(inst: AppInstance): Promise<MaiBotBehaviorOverview> {
        const s = store(inst);
        const latest = (id: string) =>
            Math.max(0, ...s.items.filter((b) => b.chat_id === id).map((b) => b.active_at ?? 0));
        const chats = CHATS.map((c) => ({
            ...c,
            count: s.items.filter((b) => b.chat_id === c.chat_id).length,
        }))
            .filter((c) => c.count > 0)
            .sort((a, b) => latest(b.chat_id) - latest(a.chat_id));
        const enabled = s.items.filter((b) => b.enabled).length;
        return withMockDelay({
            chats,
            total: s.items.length,
            enabled,
            disabled: s.items.length - enabled,
        });
    },

    detail(inst: AppInstance, id: number): Promise<MaiBotBehaviorDetail> {
        const item = store(inst).items.find((b) => b.id === id);
        if (!item)
            throw makeAppConfigError('other', '麦麦 WebUI 返回 404 Not Found：行为经验路径不存在');
        return withMockDelay({ item, evidence: evidenceOf(item), feedback: feedbackOf(item) });
    },
};
