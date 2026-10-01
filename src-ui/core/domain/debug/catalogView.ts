// 接口目录的展示：左栏分类树，和 Ctrl+K 命令面板的搜索排序。

import type { DebugActionCategory } from '../../ipc/generated/debug/DebugActionCategory';
import type { DebugActionSummary } from '../../ipc/generated/debug/DebugActionSummary';

// 少一个分类前端就编译报错，Rust 侧加了分类也一样
export const CATEGORY_LABEL: { [K in DebugActionCategory]: string } = {
    message: '消息',
    group_info: '群信息',
    group_admin: '群管理',
    friend: '好友',
    file: '文件',
    request: '请求处理',
    account: '账号与状态',
    face: '表情',
    stream: '流式',
    extension: '扩展',
};

/** 左栏里分类的先后：先日常收发消息，再群、好友，最后是杂项和扩展 */
export const CATEGORY_ORDER: DebugActionCategory[] = [
    'message',
    'group_info',
    'group_admin',
    'friend',
    'file',
    'request',
    'account',
    'face',
    'stream',
    'extension',
];

/** 按名字排；下划线开头的（`_del_group_notice` 一类内部接口）沉到最后，不抢常用接口的位置 */
function compareByName(a: DebugActionSummary, b: DebugActionSummary): number {
    const ua = a.name.startsWith('_') ? 1 : 0;
    const ub = b.name.startsWith('_') ? 1 : 0;
    if (ua !== ub) return ua - ub;
    return a.name < b.name ? -1 : a.name > b.name ? 1 : 0;
}

/**
 * 按分类分组；当前 Bot 不支持的接口单独收进 unsupported（界面上折叠在底部），不占分类里的位置。
 * 空分类不出现。
 */
export function groupActions(list: DebugActionSummary[]): {
    groups: Array<{ category: DebugActionCategory; label: string; actions: DebugActionSummary[] }>;
    unsupported: DebugActionSummary[];
} {
    const byCategory = new Map<DebugActionCategory, DebugActionSummary[]>();
    const unsupported: DebugActionSummary[] = [];
    for (const action of list) {
        if (!action.supported) {
            unsupported.push(action);
            continue;
        }
        const bucket = byCategory.get(action.category);
        if (bucket) bucket.push(action);
        else byCategory.set(action.category, [action]);
    }
    const groups = CATEGORY_ORDER.flatMap((category) => {
        const actions = byCategory.get(category);
        if (!actions || actions.length === 0) return [];
        return [{ category, label: CATEGORY_LABEL[category], actions: actions.sort(compareByName) }];
    });
    return { groups, unsupported: unsupported.sort(compareByName) };
}

/**
 * 目录列表里的主名：上游简介混着参数和实现注释（「（id 或 message_id）」「（未实现）」「；传 out_format…」），
 * 单行列表装不下、看着乱。主名只取第一个括注 / 分号前的主干，完整简介在行的悬停提示和文档页里。
 * 主干空了（整个简介就是一条注释）或没有简介时退回字段名。
 */
export function catalogRowLabel(action: Pick<DebugActionSummary, 'name' | 'summary'>): string {
    const head = action.summary.split(/[（(；;]/, 1)[0].trim();
    return head === '' ? action.name : head;
}

function scoreOf(action: DebugActionSummary, q: string): number {
    const name = action.name.toLowerCase();
    if (name === q) return 100;
    if (name.startsWith(q)) return 80;
    if (action.aliases.some((a) => a.toLowerCase() === q)) return 75;
    if (name.includes(q)) return 60;
    if (action.summary.toLowerCase().includes(q)) return 40;
    return 0;
}

/**
 * 命令面板搜索。得分：名字完全一致 100、名字前缀 80、别名完全一致 75、名字包含 60、简介包含 40，
 * 最近用过的再加 10，同分时最近用的靠前，再同分按名字。
 * 空查询没有得分可比：最近用过的排最前（新的在前），其余按名字。
 * `recent` 里最新的在最前面。
 */
export function searchActions(list: DebugActionSummary[], query: string, recent: string[]): DebugActionSummary[] {
    const q = query.trim().toLowerCase();
    const recency = new Map<string, number>();
    recent.forEach((name, i) => {
        if (!recency.has(name)) recency.set(name, i);
    });
    const rank = (a: DebugActionSummary) => recency.get(a.name) ?? Number.POSITIVE_INFINITY;

    if (q === '') {
        return [...list].sort((a, b) => {
            const ra = rank(a);
            const rb = rank(b);
            if (ra !== rb) return ra - rb;
            return compareByName(a, b);
        });
    }

    const scored: Array<{ action: DebugActionSummary; score: number }> = [];
    for (const action of list) {
        const base = scoreOf(action, q);
        if (base === 0) continue;
        scored.push({ action, score: base + (recency.has(action.name) ? 10 : 0) });
    }
    scored.sort((a, b) => {
        if (a.score !== b.score) return b.score - a.score;
        const ra = rank(a.action);
        const rb = rank(b.action);
        if (ra !== rb) return ra - rb;
        return compareByName(a.action, b.action);
    });
    return scored.map((s) => s.action);
}

/** NapCat 的 `xxx_async` / `xxx_rate_limited` 变体对应的原接口名；不是变体就原样返回 */
export function baseActionName(action: string): string {
    return action.trim().replace(/_(?:async|rate_limited)$/, '');
}

/**
 * 目录里这一行：先按名字 / 别名找；找不到时去掉 `_async`、`_rate_limited` 这类后缀再找一次，
 * NapCat 的这些变体目录里不单列，但副作用和原接口一样（危险的照样要确认）。
 * `summaryFrom` 是按哪个原接口认的；直接找到时是 null。
 * 查分级的地方（中栏发送、收藏 ▶）都用这一个，别各写各的。
 */
export function lookupSummary(
    actions: readonly DebugActionSummary[] | undefined,
    action: string,
): { summary: DebugActionSummary | null; summaryFrom: string | null } {
    if (!action || !actions) return { summary: null, summaryFrom: null };
    const find = (name: string) => actions.find((a) => a.name === name || a.aliases.includes(name)) ?? null;
    const direct = find(action);
    if (direct) return { summary: direct, summaryFrom: null };
    const base = baseActionName(action);
    const viaBase = base !== action ? find(base) : null;
    return viaBase ? { summary: viaBase, summaryFrom: viaBase.name } : { summary: null, summaryFrom: null };
}
