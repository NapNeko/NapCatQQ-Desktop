import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { DebugRequestDraft } from '../../core/ipc/generated/debug/DebugRequestDraft';
import type { DebugWorkspace } from '../../core/ipc/generated/debug/DebugWorkspace';
import { UNSET_COLUMN_WIDTH } from '../../core/domain/debug/workbenchLayout';

const workspaceMock = vi.fn();
const saveWorkspaceMock = vi.fn();
const pushErrorBar = vi.fn();

vi.mock('../../core/services/onebot-debug.service', () => ({
    onebotDebugService: {
        workspace: (...args: unknown[]) => workspaceMock(...args),
        saveWorkspace: (...args: unknown[]) => saveWorkspaceMock(...args),
    },
}));

vi.mock('../ui/pushErrorBar', () => ({
    pushErrorBar: (...args: unknown[]) => pushErrorBar(...args),
}));

import { debugWorkspaceStore as store, defaultWorkspace, flushWorkspace } from './debugWorkspaceStore';

const tab = (id: string, action = 'get_login_info', text = '{}'): DebugRequestDraft => ({
    id,
    action,
    params_text: text,
    timeout_ms: null,
    channel: null,
});

function stored(patch: Partial<DebugWorkspace>): DebugWorkspace {
    return { ...defaultWorkspace(), ...patch };
}

async function loadWith(ws: DebugWorkspace = defaultWorkspace()): Promise<void> {
    workspaceMock.mockResolvedValue(ws);
    await store.load();
}

const ws = () => store.getSnapshot().ws;

beforeEach(() => {
    workspaceMock.mockReset();
    saveWorkspaceMock.mockReset();
    saveWorkspaceMock.mockResolvedValue(undefined);
    pushErrorBar.mockReset();
    store._reset();
});

afterEach(() => {
    vi.useRealTimers();
    store._reset();
});

describe('载入', () => {
    it('只读一次，落盘的 active_tab 失效时回退到第一个标签', async () => {
        workspaceMock.mockResolvedValue(stored({ tabs: [tab('a'), tab('b')], active_tab: 'gone' }));

        await Promise.all([store.load(), store.load()]);
        await store.load();

        expect(workspaceMock).toHaveBeenCalledTimes(1);
        expect(store.getSnapshot().loaded).toBe(true);
        expect(ws().active_tab).toBe('a');
    });

    it('读失败：用默认值继续，弹错误条', async () => {
        workspaceMock.mockRejectedValue('磁盘坏了');

        await store.load();

        expect(store.getSnapshot().loaded).toBe(true);
        expect(ws()).toEqual(defaultWorkspace());
        expect(pushErrorBar).toHaveBeenCalledWith(expect.objectContaining({ key: 'debug-workspace-load' }));
    });

    it('读盘失败后不会拿默认值覆盖磁盘：保存前补读一次，仍读不到就不写，改动留在内存里', async () => {
        vi.useFakeTimers();
        workspaceMock.mockRejectedValue('读不出来');
        await store.load();
        expect(workspaceMock).toHaveBeenCalledTimes(1);

        store.selectBot('bot-1');
        await vi.advanceTimersByTimeAsync(500);
        expect(workspaceMock).toHaveBeenCalledTimes(2);
        expect(saveWorkspaceMock).not.toHaveBeenCalled();
        expect(ws().selected_bot).toBe('bot-1');

        store.selectBot('bot-2');
        await vi.advanceTimersByTimeAsync(500);
        expect(workspaceMock).toHaveBeenCalledTimes(3);
        expect(saveWorkspaceMock).not.toHaveBeenCalled();
        // 只在第一次失败时弹一次条，补读失败不重复打扰
        expect(pushErrorBar).toHaveBeenCalledTimes(1);
        expect(pushErrorBar).toHaveBeenCalledWith(expect.objectContaining({ key: 'debug-workspace-load' }));
        expect(ws().selected_bot).toBe('bot-2');
    });

    it('补读成功：内存里做过的改动套在读到的工作区上，再写盘', async () => {
        vi.useFakeTimers();
        workspaceMock.mockRejectedValueOnce('读不出来');
        await store.load();
        store.selectBot('bot-9');
        store.pushRecent('get_login_info');

        workspaceMock.mockResolvedValue(
            stored({ tabs: [tab('a', 'x', '{"k":1}')], active_tab: 'a', selected_bot: 'bot-1', recent_actions: ['old'] }),
        );
        await vi.advanceTimersByTimeAsync(500);

        expect(saveWorkspaceMock).toHaveBeenCalledTimes(1);
        const written = saveWorkspaceMock.mock.calls[0][0] as DebugWorkspace;
        // 磁盘上原有的标签还在，内存里的选择和最近使用叠在上面
        expect(written.tabs.map((t) => t.id)).toEqual(['a']);
        expect(written.selected_bot).toBe('bot-9');
        expect(written.recent_actions).toEqual(['get_login_info', 'old']);
        expect(ws()).toEqual(written);

        // 读通之后就是正常的防抖写盘，不再补读
        store.selectBot('bot-10');
        await vi.advanceTimersByTimeAsync(500);
        expect(workspaceMock).toHaveBeenCalledTimes(2);
        expect(saveWorkspaceMock).toHaveBeenCalledTimes(2);
    });

    it('载入前的改动 + 读盘失败：改动不丢，补读成功后一起套上去', async () => {
        vi.useFakeTimers();
        workspaceMock.mockRejectedValueOnce('读不出来');
        const loading = store.load();
        store.selectBot('bot-9');
        await loading;
        expect(ws().selected_bot).toBe('bot-9');

        workspaceMock.mockResolvedValue(stored({ selected_bot: 'bot-1', recent_actions: ['old'] }));
        store.pushRecent('get_login_info');
        await vi.advanceTimersByTimeAsync(500);

        const written = saveWorkspaceMock.mock.calls[0][0] as DebugWorkspace;
        expect(written.selected_bot).toBe('bot-9');
        expect(written.recent_actions).toEqual(['get_login_info', 'old']);
    });

    it('载入完成前的改动不丢：载入后套在读到的工作区上', async () => {
        let resolveLoad!: (v: DebugWorkspace) => void;
        workspaceMock.mockReturnValue(new Promise<DebugWorkspace>((r) => (resolveLoad = r)));
        const loading = store.load();

        store.selectBot('bot-9');
        expect(ws().selected_bot).toBe('bot-9');

        resolveLoad(stored({ tabs: [tab('a')], active_tab: 'a', selected_bot: 'bot-1' }));
        await loading;

        expect(ws().selected_bot).toBe('bot-9');
        expect(ws().tabs.map((t) => t.id)).toEqual(['a']);
    });
});

describe('打开动作', () => {
    it('没有标签时新开一个并激活', async () => {
        await loadWith();

        const id = store.openAction('get_group_list', { paramsText: '{"no_cache": false}' });

        expect(ws().tabs).toEqual([
            { id, action: 'get_group_list', params_text: '{"no_cache": false}', timeout_ms: null, channel: null },
        ]);
        expect(ws().active_tab).toBe(id);
    });

    it('当前标签文本没被改过（还是打开时的样子）就直接顶替，不另开', async () => {
        await loadWith();
        const first = store.openAction('get_group_list', { paramsText: '{"no_cache": false}' });

        const second = store.openAction('get_friend_list', { paramsText: '{}' });

        expect(second).toBe(first);
        expect(ws().tabs).toHaveLength(1);
        expect(ws().tabs[0].action).toBe('get_friend_list');
    });

    it('当前标签文本是 {} 也算没动过', async () => {
        await loadWith(stored({ tabs: [tab('a', 'get_login_info', '{}')], active_tab: 'a' }));

        const id = store.openAction('get_group_list');

        expect(id).toBe('a');
        expect(ws().tabs).toHaveLength(1);
        expect(ws().tabs[0].action).toBe('get_group_list');
    });

    it('用户改过文本的标签不顶替：另开新标签并激活', async () => {
        await loadWith();
        const first = store.openAction('send_msg', { paramsText: '{"message": ""}' });
        store.setParamsText(first, '{"message": "hi"}');

        const second = store.openAction('get_group_list');

        expect(second).not.toBe(first);
        expect(ws().tabs.map((t) => t.action)).toEqual(['send_msg', 'get_group_list']);
        expect(ws().tabs[0].params_text).toBe('{"message": "hi"}');
        expect(ws().active_tab).toBe(second);
    });

    it('newTab 选项强制另开，哪怕当前标签没动过', async () => {
        await loadWith();
        const first = store.openAction('get_group_list');

        const second = store.openAction('get_friend_list', { newTab: true });

        expect(second).not.toBe(first);
        expect(ws().tabs).toHaveLength(2);
    });

    it('编辑器按说明填的初始文本（initial）不算用户改过', async () => {
        await loadWith();
        const id = store.openAction('send_msg'); // 说明还没到，先是 {}
        store.setParamsText(id, '{"message": []}', { initial: true });

        store.openAction('get_group_list');

        expect(ws().tabs).toHaveLength(1);
        expect(ws().tabs[0].action).toBe('get_group_list');
    });

    it('顶替时上一个动作的结果被清掉', async () => {
        await loadWith();
        const id = store.openAction('get_group_list');
        store.setRun(id, { inflight: { requestId: 'r1', startedAt: 1 } });

        store.openAction('get_friend_list');

        expect(store.getRun(id)).toBeUndefined();
    });

    it('newTab 建空白标签并激活', async () => {
        await loadWith();
        const id = store.newTab();
        expect(ws().tabs).toEqual([{ id, action: '', params_text: '{}', timeout_ms: null, channel: null }]);
        expect(ws().active_tab).toBe(id);
    });
});

describe('原地换动作（setTabAction）', () => {
    it('参数、通道、超时留着，结果丢掉；留下的参数算用户写的，之后从目录点接口另开标签', async () => {
        await loadWith();
        const id = store.openAction('get_group_info', { paramsText: '{"group_id": 1}' });
        store.setTimeout(id, 5000);
        store.setTabChannel(id, { kind: 'internal' });
        store.setRun(id, { inflight: { requestId: 'r1', startedAt: 1 } });

        store.setTabAction(id, 'get_group_member_list');

        expect(ws().tabs).toEqual([
            { id, action: 'get_group_member_list', params_text: '{"group_id": 1}', timeout_ms: 5000, channel: { kind: 'internal' } },
        ]);
        expect(store.getRun(id)).toBeUndefined();
        const next = store.openAction('get_login_info');
        expect(next).not.toBe(id);
        expect(ws().tabs).toHaveLength(2);
    });

    it('给了 paramsText 就换上；同一个动作同样的文本不算改动', async () => {
        await loadWith();
        const id = store.openAction('get_group_info');
        store.setTabAction(id, 'get_msg', { paramsText: '{"message_id": 7}' });
        expect(ws().tabs[0]).toMatchObject({ action: 'get_msg', params_text: '{"message_id": 7}' });

        const before = store.getSnapshot();
        store.setTabAction(id, 'get_msg');
        expect(store.getSnapshot()).toBe(before);
        store.setTabAction('gone', 'get_msg');
        expect(store.getSnapshot()).toBe(before);
    });
});

describe('关闭与恢复', () => {
    it('关当前标签：进最近关闭，激活右邻，没有右邻就左邻', async () => {
        await loadWith(
            stored({
                tabs: [tab('a', 'x', '{"k":1}'), tab('b', 'y', '{"k":2}'), tab('c', 'z', '{"k":3}')],
                active_tab: 'b',
            }),
        );

        store.closeTab('b');
        expect(ws().active_tab).toBe('c');
        expect(ws().closed_tabs.map((t) => t.id)).toEqual(['b']);

        store.closeTab('c');
        expect(ws().active_tab).toBe('a');

        store.closeTab('a');
        expect(ws().active_tab).toBeNull();
        expect(ws().closed_tabs.map((t) => t.id)).toEqual(['a', 'c', 'b']);
    });

    it('关的不是当前标签时当前标签不变；空白标签不占最近关闭', async () => {
        await loadWith(stored({ tabs: [tab('a', 'x', '{"k":1}'), tab('blank', '', '{}')], active_tab: 'a' }));

        store.closeTab('blank');

        expect(ws().active_tab).toBe('a');
        expect(ws().closed_tabs).toEqual([]);
    });

    it('最近关闭只留 10 个', async () => {
        const many = Array.from({ length: 12 }, (_, i) => tab(`t${i}`, 'x', `{"i":${i}}`));
        await loadWith(stored({ tabs: many, active_tab: 't0' }));

        for (const t of many) store.closeTab(t.id);

        expect(ws().closed_tabs).toHaveLength(10);
        expect(ws().closed_tabs[0].id).toBe('t11');
    });

    it('reopenClosed 恢复最近关的那个并激活；没有可恢复的返回 null', async () => {
        await loadWith(stored({ tabs: [tab('a', 'x', '{"k":1}')], active_tab: 'a' }));
        store.closeTab('a');
        expect(ws().tabs).toEqual([]);

        expect(store.reopenClosed()).toBe('a');
        expect(ws().tabs.map((t) => t.id)).toEqual(['a']);
        expect(ws().active_tab).toBe('a');
        expect(ws().closed_tabs).toEqual([]);
        expect(store.reopenClosed()).toBeNull();
    });

    it('恢复出来的 id 撞上现有标签时换新 id', async () => {
        await loadWith(
            stored({
                tabs: [tab('a', 'x', '{"k":1}')],
                active_tab: 'a',
                closed_tabs: [tab('a', 'old', '{"k":0}')],
            }),
        );

        const id = store.reopenClosed();

        expect(id).not.toBe('a');
        expect(ws().tabs.map((t) => t.action)).toEqual(['x', 'old']);
        expect(ws().active_tab).toBe(id);
    });

    it('关掉标签时它的调用记录一并清掉', async () => {
        await loadWith(stored({ tabs: [tab('a', 'x', '{"k":1}')], active_tab: 'a' }));
        store.setRun('a', { inflight: { requestId: 'r', startedAt: 1 } });

        store.closeTab('a');

        expect(store.getRun('a')).toBeUndefined();
    });
});

describe('其它字段', () => {
    it('recent_actions 新的在前、去重、最多 20 个', async () => {
        await loadWith();
        for (let i = 0; i < 25; i += 1) store.pushRecent(`a${i}`);
        store.pushRecent('a10');

        expect(ws().recent_actions).toHaveLength(20);
        expect(ws().recent_actions.slice(0, 3)).toEqual(['a10', 'a24', 'a23']);
        expect(new Set(ws().recent_actions).size).toBe(20);
    });

    it('通道选择、布局、超时、标签通道', async () => {
        await loadWith(stored({ tabs: [tab('a')], active_tab: 'a' }));

        store.setChannelChoice('bot-1', { call: { kind: 'internal' }, events: { kind: 'auto' } });
        store.setLayout({ left_width: 300, right_view: 'list' });
        store.setTimeout('a', 5000);
        store.setTabChannel('a', { kind: 'http', name: 'main' });

        expect(ws().channel_choice['bot-1']).toEqual({ call: { kind: 'internal' }, events: { kind: 'auto' } });
        expect(ws().layout).toMatchObject({ left_width: 300, right_view: 'list', right_width: UNSET_COLUMN_WIDTH });
        expect(ws().tabs[0]).toMatchObject({ timeout_ms: 5000, channel: { kind: 'http', name: 'main' } });
    });

    it('没有实际变化的写入不换引用、不触发保存', async () => {
        vi.useFakeTimers();
        await loadWith(stored({ tabs: [tab('a')], active_tab: 'a', selected_bot: 'bot-1' }));
        const before = store.getSnapshot();

        store.selectBot('bot-1');
        store.setActive('a');
        store.setLayout({ left_width: UNSET_COLUMN_WIDTH });
        store.setTimeout('a', null);
        await vi.advanceTimersByTimeAsync(1000);

        expect(store.getSnapshot()).toBe(before);
        expect(saveWorkspaceMock).not.toHaveBeenCalled();
    });

    it('setRun 不落盘', async () => {
        vi.useFakeTimers();
        await loadWith();

        store.setRun('a', { inflight: { requestId: 'r', startedAt: 1 } });
        await vi.advanceTimersByTimeAsync(1000);

        expect(store.getRun('a')?.inflight?.requestId).toBe('r');
        expect(saveWorkspaceMock).not.toHaveBeenCalled();
    });
});

describe('防抖写盘', () => {
    it('500ms 内的连续改动只写一次，写的是最新的', async () => {
        vi.useFakeTimers();
        await loadWith();

        store.selectBot('bot-1');
        await vi.advanceTimersByTimeAsync(300);
        store.selectBot('bot-2');
        await vi.advanceTimersByTimeAsync(300);
        expect(saveWorkspaceMock).not.toHaveBeenCalled();

        await vi.advanceTimersByTimeAsync(200);
        expect(saveWorkspaceMock).toHaveBeenCalledTimes(1);
        expect(saveWorkspaceMock.mock.calls[0][0]).toMatchObject({ selected_bot: 'bot-2' });
    });

    it('flushWorkspace 立刻写，之后防抖到点不再重复写', async () => {
        vi.useFakeTimers();
        await loadWith();
        store.selectBot('bot-1');

        await flushWorkspace();
        expect(saveWorkspaceMock).toHaveBeenCalledTimes(1);

        await vi.advanceTimersByTimeAsync(1000);
        expect(saveWorkspaceMock).toHaveBeenCalledTimes(1);
    });

    it('没有待写的改动时 flush 什么也不做', async () => {
        await loadWith();
        await flushWorkspace();
        expect(saveWorkspaceMock).not.toHaveBeenCalled();
    });

    it('窗口藏起来（visibilitychange → hidden）时立刻写', async () => {
        vi.useFakeTimers();
        await loadWith();
        store.selectBot('bot-1');

        const spy = vi.spyOn(document, 'visibilityState', 'get').mockReturnValue('hidden');
        document.dispatchEvent(new Event('visibilitychange'));
        await vi.advanceTimersByTimeAsync(0);
        spy.mockRestore();

        expect(saveWorkspaceMock).toHaveBeenCalledTimes(1);
    });

    it('写的途中又有改动：等这一轮写完后补写，且两次写不交叠', async () => {
        vi.useFakeTimers();
        await loadWith();
        let release!: () => void;
        saveWorkspaceMock.mockImplementationOnce(() => new Promise<void>((r) => (release = r)));
        store.selectBot('bot-1');
        const firstFlush = flushWorkspace();

        store.selectBot('bot-2');
        const secondFlush = flushWorkspace();
        expect(saveWorkspaceMock).toHaveBeenCalledTimes(1);

        release();
        await Promise.all([firstFlush, secondFlush]);

        expect(saveWorkspaceMock).toHaveBeenCalledTimes(2);
        expect(saveWorkspaceMock.mock.calls[1][0]).toMatchObject({ selected_bot: 'bot-2' });
    });

    it('写失败弹错误条，连续失败只弹一次；成功一次后再失败才会再弹', async () => {
        vi.useFakeTimers();
        await loadWith();
        saveWorkspaceMock.mockRejectedValue('写不进去');

        store.selectBot('bot-1');
        await flushWorkspace();
        store.selectBot('bot-2');
        await flushWorkspace();
        expect(pushErrorBar).toHaveBeenCalledTimes(1);
        expect(pushErrorBar).toHaveBeenCalledWith(expect.objectContaining({ key: 'debug-workspace-save' }));

        saveWorkspaceMock.mockResolvedValueOnce(undefined);
        store.selectBot('bot-3');
        await flushWorkspace();
        saveWorkspaceMock.mockRejectedValue('又写不进去');
        store.selectBot('bot-4');
        await flushWorkspace();
        expect(pushErrorBar).toHaveBeenCalledTimes(2);
    });
});
