// 浏览器预览：麦麦学到的表达方式和黑话。语义照后端 resources/{expression,jargon}：
// 分页、搜索、按聊天、筛选；精选 / 取消精选；黑话固定含义 = pinned。

import { makeAppConfigError } from '../../domain/apps/appConfigError';
import type {
    AppInstance,
    MaiBotExpression,
    MaiBotExpressionAction,
    MaiBotExpressionOverview,
    MaiBotExpressionPage,
    MaiBotExpressionQuery,
    MaiBotJargon,
    MaiBotJargonAction,
    MaiBotJargonOverview,
    MaiBotJargonPage,
    MaiBotJargonQuery,
    MaiBotLearningChat,
    MaiBotResourceDone,
} from '../types';
import { withMockDelay } from './bootstrap.mock';

const CHATS: MaiBotLearningChat[] = [
    { chat_id: 'c0a1', chat_name: '麦麦测试群', platform: 'qq', is_group: true },
    { chat_id: 'c0b2', chat_name: '原神交流群', platform: 'qq', is_group: true },
    { chat_id: 'c0c3', chat_name: '小林的私聊', platform: 'qq', is_group: false },
    { chat_id: 'c0d4', chat_name: '摸鱼小分队', platform: 'qq', is_group: true },
];

const PAIRS: [string, string][] = [
    ['有人夸你', '嘿嘿 被发现了'],
    ['被问到不会的问题', '这个我真不太懂，要不问问群里大佬'],
    ['群里在聊游戏抽卡', '又歪了是吧，懂的都懂'],
    ['有人说自己很累', '抱抱，先去睡一觉吧'],
    ['别人发了一张猫图', '好可爱！是谁家的小猫'],
    ['被人阴阳怪气', '你说得对'],
    ['有人问麦麦是不是机器人', '我只是一个普通的大学生啦'],
    ['深夜还有人在聊天', '都几点了还不睡'],
    ['有人分享好消息', '恭喜恭喜！请客吗'],
    ['话题冷场了', '那我去写作业了'],
    ['有人问推荐什么游戏', '最近在玩星穹铁道，还挺好玩'],
    ['被@但不知道说什么', '？'],
    ['有人吐槽作业多', '同款，一起摆烂'],
    ['别人在讨论天气', '今天是真的热'],
];

type Store = { expressions: MaiBotExpression[]; jargons: MaiBotJargon[]; nextId: number };
const stores = new Map<string, Store>();

function store(inst: AppInstance): Store {
    if (inst.state !== 'running') throw makeAppConfigError('not_running', '启动麦麦后才能用');
    let s = stores.get(inst.id);
    if (!s) {
        const now = Date.now() / 1000;
        const expressions = PAIRS.flatMap(([situation, style], i) =>
            CHATS.slice(0, i % 3 === 0 ? 3 : 2).map((chat, j) => ({
                id: 100 + i * 4 + j,
                situation,
                style: j === 0 ? style : `${style}~`,
                chat_id: chat.chat_id,
                chat_name: chat.chat_name,
                curated: (i + j) % 3 === 0,
                last_active: now - (i * 4 + j) * 3600 * 5,
                created: now - (i * 4 + j) * 86400,
            })),
        );
        const jargons: MaiBotJargon[] = [
            ['yyds', '永远的神，夸某样东西特别好'],
            ['绝绝子', '好到极点，也常被反讽着用'],
            ['歪了', '抽卡没抽到想要的限定角色'],
            ['保底', '抽卡抽到一定次数必出高星'],
            ['摆烂', '放弃努力，随它去'],
            ['xswl', '笑死我了'],
            ['发电', '为某人或某事热情输出'],
            ['大佬', ''],
            ['水群', '在群里闲聊刷消息'],
            ['老铁', '关系好的朋友'],
            ['破防', '心理防线被击穿'],
            ['原石', '原神里的抽卡货币'],
        ].map(([content, meaning], i) => ({
            id: 500 + i,
            content: content!,
            meaning: meaning!,
            chat_ids: [CHATS[i % 3]!.chat_id],
            chat_names: [CHATS[i % 3]!.chat_name],
            count: 3 + ((i * 7) % 40),
            is_jargon: !!meaning && i % 5 !== 4,
            is_global: i === 0 || i === 4,
            pinned: i === 2 || i === 11,
            complete: i === 0,
        }));
        s = { expressions, jargons, nextId: 1000 };
        stores.set(inst.id, s);
    }
    return s;
}

function paginate<T>(rows: T[], page: number, size: number): { total: number; items: T[] } {
    const start = (Math.max(1, page) - 1) * size;
    return { total: rows.length, items: rows.slice(start, start + size) };
}

const chatName = (id: string) => CHATS.find((c) => c.chat_id === id)?.chat_name ?? id;
const done = (affected: number, message: string): Promise<MaiBotResourceDone> =>
    withMockDelay({ affected, message });

export const mockMaiBotLearning = {
    expressions(inst: AppInstance, q: MaiBotExpressionQuery): Promise<MaiBotExpressionPage> {
        const s = store(inst);
        const kw = q.search.trim().toLowerCase();
        const rows = s.expressions
            .filter((e) => !q.chat_id || e.chat_id === q.chat_id)
            .filter((e) =>
                q.filter === 'curated' ? e.curated : q.filter === 'uncurated' ? !e.curated : true,
            )
            .filter((e) => !kw || `${e.situation}${e.style}`.toLowerCase().includes(kw))
            .sort((a, b) => b.last_active - a.last_active);
        return withMockDelay(paginate(rows, q.page, q.page_size));
    },

    expressionOverview(inst: AppInstance): Promise<MaiBotExpressionOverview> {
        const s = store(inst);
        const week = Date.now() / 1000 - 7 * 86400;
        return withMockDelay({
            chats: CHATS,
            used_chat_ids: [...new Set(s.expressions.map((e) => e.chat_id))],
            total: s.expressions.length,
            curated: s.expressions.filter((e) => e.curated).length,
            uncurated: s.expressions.filter((e) => !e.curated).length,
            recent_7days: s.expressions.filter((e) => (e.created ?? 0) > week).length,
        });
    },

    expressionAction(inst: AppInstance, a: MaiBotExpressionAction): Promise<MaiBotResourceDone> {
        const s = store(inst);
        const now = Date.now() / 1000;
        switch (a.op) {
            case 'create':
                s.expressions.unshift({
                    id: s.nextId++,
                    situation: a.situation.trim(),
                    style: a.style.trim(),
                    chat_id: a.chat_id,
                    chat_name: chatName(a.chat_id),
                    curated: false,
                    last_active: now,
                    created: now,
                });
                return done(1, '加好了');
            case 'update':
                s.expressions = s.expressions.map((e) =>
                    e.id === a.id
                        ? {
                              ...e,
                              situation: a.situation.trim(),
                              style: a.style.trim(),
                              ...(a.chat_id
                                  ? { chat_id: a.chat_id, chat_name: chatName(a.chat_id) }
                                  : {}),
                              last_active: now,
                          }
                        : e,
                );
                return done(1, '改好了');
            case 'curate':
                s.expressions = s.expressions.map((e) =>
                    a.ids.includes(e.id) ? { ...e, curated: a.curated, last_active: now } : e,
                );
                return done(a.ids.length, a.curated ? '已精选' : '已取消精选');
            case 'delete':
                s.expressions = s.expressions.filter((e) => !a.ids.includes(e.id));
                return done(a.ids.length, '删掉了');
        }
    },

    jargons(inst: AppInstance, q: MaiBotJargonQuery): Promise<MaiBotJargonPage> {
        const s = store(inst);
        const kw = q.search.trim().toLowerCase();
        const rows = s.jargons
            .filter((j) => !q.chat_id || j.chat_ids.includes(q.chat_id))
            .filter((j) => {
                switch (q.filter) {
                    case 'confirmed':
                        return j.is_jargon && !!j.meaning;
                    case 'not_jargon':
                        return !(j.is_jargon && j.meaning);
                    case 'pinned':
                        return j.pinned;
                    case 'global':
                        return j.is_global;
                    default:
                        return true;
                }
            })
            .filter((j) => !kw || j.content.toLowerCase().includes(kw))
            .sort((a, b) => b.count - a.count || b.id - a.id);
        return withMockDelay(paginate(rows, q.page, q.page_size));
    },

    jargonOverview(inst: AppInstance): Promise<MaiBotJargonOverview> {
        const s = store(inst);
        return withMockDelay({
            chats: CHATS,
            used_chat_ids: [...new Set(s.jargons.flatMap((j) => j.chat_ids))],
            total: s.jargons.length,
            confirmed: s.jargons.filter((j) => j.is_jargon && j.meaning).length,
            pinned: s.jargons.filter((j) => j.pinned).length,
            global: s.jargons.filter((j) => j.is_global).length,
        });
    },

    jargonAction(inst: AppInstance, a: MaiBotJargonAction): Promise<MaiBotResourceDone> {
        const s = store(inst);
        switch (a.op) {
            case 'create':
                if (!a.chat_ids.length)
                    return Promise.reject(makeAppConfigError('invalid', '至少挑一个聊天'));
                s.jargons.unshift({
                    id: s.nextId++,
                    content: a.content.trim(),
                    meaning: a.meaning.trim(),
                    chat_ids: a.chat_ids,
                    chat_names: a.chat_ids.map(chatName),
                    count: 0,
                    is_jargon: !!a.meaning.trim(),
                    is_global: a.is_global,
                    pinned: true,
                    complete: false,
                });
                return done(1, '加好了');
            case 'update':
                s.jargons = s.jargons.map((j) =>
                    j.id === a.id
                        ? {
                              ...j,
                              content: a.content.trim(),
                              meaning: a.meaning.trim(),
                              is_global: a.is_global,
                              is_jargon: a.is_jargon,
                              pinned: a.pinned,
                              ...(a.chat_ids
                                  ? { chat_ids: a.chat_ids, chat_names: a.chat_ids.map(chatName) }
                                  : {}),
                          }
                        : j,
                );
                return done(1, '改好了');
            case 'set_jargon':
                s.jargons = s.jargons.map((j) =>
                    a.ids.includes(j.id) ? { ...j, is_jargon: a.is_jargon } : j,
                );
                return done(a.ids.length, a.is_jargon ? '标成黑话了' : '标成不是黑话了');
            case 'delete':
                s.jargons = s.jargons.filter((j) => !a.ids.includes(j.id));
                return done(a.ids.length, '删掉了');
        }
    },
};
