import { describe, expect, it } from 'vitest';
import type { DebugActionSummary } from '../../ipc/generated/debug/DebugActionSummary';
import type { DebugCollections } from '../../ipc/generated/debug/DebugCollections';
import type { DebugSavedRequest } from '../../ipc/generated/debug/DebugSavedRequest';
import {
    PALETTE_SAVED_LIMIT,
    buildPaletteRows,
    firstSelectable,
    isSelectable,
    stepSelectable,
    type PaletteRow,
} from './palette';

const action = (
    name: string,
    summary = '',
    patch: Partial<DebugActionSummary> = {},
): DebugActionSummary => ({
    name,
    aliases: [],
    summary,
    category: 'message',
    safety: 'read_only',
    stream: false,
    supported: true,
    other_backend_present: true,
    param_diff: false,
    ...patch,
});

const ACTIONS = [
    action('send_group_msg', '发送群消息', { safety: 'side_effect' }),
    action('get_group_list', '获取群列表'),
    action('get_login_info', '获取登录号信息'),
    action('delete_msg', '撤回消息', { safety: 'dangerous', aliases: ['recall_msg'] }),
];

const saved = (
    id: string,
    name: string,
    actionName: string,
    folder: string | null = null,
    order = 0,
): DebugSavedRequest => ({
    id,
    name,
    folder_id: folder,
    action: actionName,
    params: {},
    channel: null,
    note: null,
    order,
    created_at_ms: order,
    updated_at_ms: order,
});

const COLLECTIONS: DebugCollections = {
    version: 1,
    folders: [{ id: 'f1', name: '常用', order: 0 }],
    requests: [
        saved('b', '测试群打招呼', 'send_group_msg', 'f1', 0),
        saved('a', '看看登录号', 'get_login_info', null, 0),
    ],
};

const summarize = (rows: PaletteRow[]) =>
    rows.map((r) =>
        r.kind === 'header'
            ? `# ${r.label}`
            : r.kind === 'action'
              ? `${r.action.name}${r.recent ? ' *' : ''}`
              : r.kind === 'saved'
                ? `☆ ${r.request.name}${r.folderName ? ` @${r.folderName}` : ''}`
                : r.kind === 'free'
                  ? `→ ${r.name}`
                  : `… ${r.text}`,
    );

describe('buildPaletteRows', () => {
    it('没输入：最近用过、收藏（按文件夹顺序）、全部接口（不重复列最近用过的）', () => {
        const rows = buildPaletteRows({
            actions: ACTIONS,
            collections: COLLECTIONS,
            query: '',
            recent: ['get_login_info', 'gone_action'],
        });
        expect(summarize(rows)).toEqual([
            '# 最近用过',
            'get_login_info *',
            '# 收藏 · 2',
            '☆ 测试群打招呼 @常用',
            '☆ 看看登录号',
            '# 全部接口 · 4',
            'delete_msg',
            'get_group_list',
            'send_group_msg',
        ]);
        const hello = rows.find((r) => r.kind === 'saved' && r.request.id === 'b');
        expect(hello).toMatchObject({ safety: 'side_effect' });
    });

    it('没输入时收藏太多只列前几个，提示输入缩小范围', () => {
        const many: DebugCollections = {
            version: 1,
            folders: [],
            requests: Array.from({ length: PALETTE_SAVED_LIMIT + 3 }, (_, i) =>
                saved(`r${i}`, `收藏 ${i}`, 'get_group_list', null, i),
            ),
        };
        const rows = buildPaletteRows({
            actions: ACTIONS,
            collections: many,
            query: '',
            recent: [],
        });
        expect(rows.filter((r) => r.kind === 'saved')).toHaveLength(PALETTE_SAVED_LIMIT);
        expect(rows.find((r) => r.kind === 'more')).toMatchObject({
            text: '还有 3 个收藏，输入名字缩小范围',
        });
    });

    it('输入之后：接口按得分排，收藏按名字 / 接口名筛；最近用过的标出来', () => {
        const rows = buildPaletteRows({
            actions: ACTIONS,
            collections: COLLECTIONS,
            query: 'group',
            recent: ['send_group_msg'],
        });
        expect(summarize(rows)).toEqual([
            '# 接口 · 2',
            'send_group_msg *',
            'get_group_list',
            '# 收藏 · 1',
            '☆ 测试群打招呼 @常用',
        ]);
        expect(
            summarize(
                buildPaletteRows({
                    actions: ACTIONS,
                    collections: COLLECTIONS,
                    query: '登录',
                    recent: [],
                }),
            ),
        ).toEqual(['# 接口 · 1', 'get_login_info', '# 收藏 · 1', '☆ 看看登录号']);
    });

    it('像接口名、目录里又没有的词：给「打开目录外的接口」；没搜到时它排第一，搜到了排在接口最后', () => {
        expect(
            summarize(
                buildPaletteRows({
                    actions: ACTIONS,
                    collections: null,
                    query: 'delete_msg_async',
                    recent: [],
                }),
            ),
        ).toEqual(['# 接口', '→ delete_msg_async']);
        expect(
            summarize(
                buildPaletteRows({
                    actions: ACTIONS,
                    collections: null,
                    query: 'get_group',
                    recent: [],
                }),
            ),
        ).toEqual(['# 接口 · 1', 'get_group_list', '→ get_group']);
        // 名字或别名完全一样就不给
        expect(
            buildPaletteRows({
                actions: ACTIONS,
                collections: null,
                query: 'recall_msg',
                recent: [],
            }).some((r) => r.kind === 'free'),
        ).toBe(false);
        expect(
            buildPaletteRows({
                actions: ACTIONS,
                collections: null,
                query: 'get_login_info',
                recent: [],
            }).some((r) => r.kind === 'free'),
        ).toBe(false);
        // 不像接口名（中文、带空格）的不给
        expect(
            buildPaletteRows({
                actions: ACTIONS,
                collections: null,
                query: '没有 这个',
                recent: [],
            }),
        ).toEqual([]);
        expect(
            buildPaletteRows({
                actions: [],
                collections: null,
                query: '.ocr_image',
                recent: [],
            }).some((r) => r.kind === 'free'),
        ).toBe(true);
    });

    it('收藏还没读到时不出收藏那一段', () => {
        const rows = buildPaletteRows({
            actions: ACTIONS,
            collections: null,
            query: '',
            recent: [],
        });
        expect(rows.some((r) => r.kind === 'saved')).toBe(false);
    });
});

describe('选择', () => {
    const rows = buildPaletteRows({
        actions: ACTIONS,
        collections: COLLECTIONS,
        query: '',
        recent: ['get_login_info'],
    });

    it('跳过标题行和提示行；到头停住不绕回', () => {
        const first = firstSelectable(rows);
        expect(isSelectable(rows[first])).toBe(true);
        expect(first).toBe(1);
        const next = stepSelectable(rows, first, 1);
        expect(rows[next]?.kind).toBe('saved');
        expect(stepSelectable(rows, first, -1)).toBe(first);
        const last = rows.length - 1;
        expect(stepSelectable(rows, last, 1)).toBe(last);
        expect(firstSelectable([])).toBe(-1);
    });
});
