// terminalStore 行为单测：只走 terminalStore / openTerminal / sessionTitle / isLive 这些公开 API。
//
// store 本身没有 _reset（生产不需要），所以每个用例 vi.resetModules + 重新 import，
// 连同 terminalPrefs 一起重建；localStorage 手动清掉，模拟「新窗口」。
// GROUPS_KEY 与源文件里的字面量一致：这是持久化格式本身，测试断言的就是这份契约。

import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { LocalShellKind } from '../../core/ipc/generated/domain/LocalShellKind';
import type { TerminalSessionInfo } from '../../core/ipc/generated/domain/TerminalSessionInfo';
import type { TerminalStatus } from '../../core/ipc/generated/domain/TerminalStatus';
import type { TerminalTarget } from '../../core/ipc/generated/domain/TerminalTarget';
import type { TerminalGroup } from './terminalStore';

const listMock = vi.fn();
const openMock = vi.fn();
const closeMock = vi.fn();
const restartMock = vi.fn();
const pushErrorBarMock = vi.fn();

vi.mock('../../core/services/terminal.service', () => ({
    terminalService: {
        list: (...args: unknown[]) => listMock(...args),
        open: (...args: unknown[]) => openMock(...args),
        close: (...args: unknown[]) => closeMock(...args),
        restart: (...args: unknown[]) => restartMock(...args),
    },
}));

vi.mock('../ui/pushErrorBar', () => ({
    pushErrorBar: (...args: unknown[]) => pushErrorBarMock(...args),
}));

const GROUPS_KEY = 'ncd.terminal.groups.v1';

type Persisted = { groups: TerminalGroup[]; activeGroup: string | null };

function persisted(): Persisted | null {
    const raw = window.localStorage.getItem(GROUPS_KEY);
    return raw ? (JSON.parse(raw) as Persisted) : null;
}

function seedSaved(groups: unknown[], activeGroup: string | null): void {
    window.localStorage.setItem(GROUPS_KEY, JSON.stringify({ groups, activeGroup }));
}

let seq = 0;
function info(over: Partial<TerminalSessionInfo> = {}): TerminalSessionInfo {
    return {
        id: `s${++seq}`,
        target: { kind: 'local' },
        title: '会话',
        host_id: 'local',
        host_label: '本机',
        host_os: 'windows',
        cwd: 'C:\\temp',
        shell: 'pwsh',
        status: { kind: 'running' },
        created_at_ms: 0,
        features: {
            files: true,
            stats: false,
            sudo_fill: false,
            shell_integration: true,
            external: true,
        },
        snippets: [],
        ...over,
    };
}

// 返回类型交给推断；store 没有 _reset，resetModules 之后必须动态 import 才能拿到
// 新模块代际，静态 import 永远绑第一份实例
async function freshStore() {
    vi.resetModules();
    return import('./terminalStore');
}

/** open/toggle 内部的 await 链全部落定 */
async function flush(): Promise<void> {
    for (let i = 0; i < 10; i++) await Promise.resolve();
}

beforeEach(() => {
    window.localStorage.clear();
    listMock.mockReset().mockResolvedValue([]);
    openMock.mockReset();
    closeMock.mockReset().mockResolvedValue(undefined);
    restartMock.mockReset();
    pushErrorBarMock.mockReset();
});

describe('bootstrap', () => {
    it('拉会话表并置 ready；重复调用不再拉第二次', async () => {
        listMock.mockResolvedValue([info({ id: 'a' }), info({ id: 'b' })]);
        const { terminalStore: store } = await freshStore();
        await store.bootstrap();
        const snap = store.getSnapshot();
        expect(snap.ready).toBe(true);
        expect(Object.keys(snap.sessions).sort()).toEqual(['a', 'b']);
        // localStorage 没东西时每个会话各一个标签，activeGroup 落最后一个
        expect(snap.groups.map((g) => g.panes)).toEqual([['a'], ['b']]);
        expect(snap.activeGroup).toBe(snap.groups[1].id);
        await store.bootstrap();
        expect(listMock).toHaveBeenCalledTimes(1);
    });

    it('后端拉取失败时只置 ready，不崩', async () => {
        listMock.mockRejectedValue(new Error('ipc down'));
        const { terminalStore: store } = await freshStore();
        await store.bootstrap();
        const snap = store.getSnapshot();
        expect(snap.ready).toBe(true);
        expect(snap.sessions).toEqual({});
        expect(snap.groups).toEqual([]);
        expect(snap.activeGroup).toBeNull();
    });
});

describe('localStorage 标签恢复', () => {
    it('恢复分组 / 分屏方向 / 比例 / 聚焦，比例越界夹到边界', async () => {
        seedSaved(
            [
                {
                    id: 'ga',
                    panes: ['a', 'b'],
                    split: 'column',
                    ratio: 0.7,
                    focused: 'b',
                },
                { id: 'gb', panes: ['c'], split: 'row', ratio: 5, focused: 'c' },
            ],
            'ga',
        );
        listMock.mockResolvedValue([info({ id: 'a' }), info({ id: 'b' }), info({ id: 'c' })]);
        const { terminalStore: store } = await freshStore();
        await store.bootstrap();
        const snap = store.getSnapshot();
        expect(snap.groups.map((g) => g.id)).toEqual(['ga', 'gb']);
        expect(snap.groups[0].split).toBe('column');
        expect(snap.groups[0].ratio).toBe(0.7);
        expect(snap.groups[0].focused).toBe('b');
        expect(snap.groups[1].ratio).toBe(0.85);
        expect(snap.activeGroup).toBe('ga');
    });

    it('越界会话 id 过滤掉；全灭的组整组丢掉；activeGroup 失效退到最后一组', async () => {
        seedSaved(
            [
                {
                    id: 'ga',
                    panes: ['ghost1', 'ghost2'],
                    split: 'row',
                    ratio: 0.5,
                    focused: 'ghost1',
                },
                {
                    id: 'gb',
                    panes: ['a', 'ghost3', 'b', 'c'],
                    split: 'row',
                    ratio: 0.5,
                    focused: 'ghost3',
                },
            ],
            'ga',
        );
        listMock.mockResolvedValue([info({ id: 'a' }), info({ id: 'b' }), info({ id: 'c' })]);
        const { terminalStore: store } = await freshStore();
        await store.bootstrap();
        const snap = store.getSnapshot();
        // gb 的 panes 只留活着的 a、b，且每组最多两块 → c 自己成组
        expect(snap.groups.map((g) => g.panes)).toEqual([['a', 'b'], ['c']]);
        // focused 指向越界 id 时退回第一块
        expect(snap.groups[0].focused).toBe('a');
        // saved activeGroup 'ga' 已被丢弃 → 退到最后一组
        expect(snap.activeGroup).toBe(snap.groups[1].id);
    });

    it('localStorage 是坏 JSON 时每个会话各成一个标签', async () => {
        window.localStorage.setItem(GROUPS_KEY, '{oops');
        listMock.mockResolvedValue([info({ id: 'a' }), info({ id: 'b' })]);
        const { terminalStore: store } = await freshStore();
        await store.bootstrap();
        const snap = store.getSnapshot();
        expect(snap.groups.map((g) => g.panes)).toEqual([['a'], ['b']]);
        expect(snap.activeGroup).toBe(snap.groups[1].id);
    });

    it('open 落盘 → 重建 store 后 bootstrap 原样读回布局', async () => {
        openMock
            .mockResolvedValueOnce(info({ id: 'a' }))
            .mockResolvedValueOnce(
                info({ id: 'b', target: { kind: 'bot', bot_id: '1', host_dir: false } }),
            );
        const first = await freshStore();
        await first.terminalStore.open({ kind: 'local' });
        await first.terminalStore.open({ kind: 'bot', bot_id: '1', host_dir: false });
        const saved = persisted();
        expect(saved?.groups.map((g) => g.panes)).toEqual([['a'], ['b']]);

        listMock.mockResolvedValue([info({ id: 'a' }), info({ id: 'b' })]);
        const second = await freshStore();
        await second.terminalStore.bootstrap();
        const snap = second.terminalStore.getSnapshot();
        expect(snap.groups.map((g) => ({ id: g.id, panes: g.panes }))).toEqual(
            saved?.groups.map((g) => ({ id: g.id, panes: g.panes })),
        );
        expect(snap.activeGroup).toBe(saved?.activeGroup);
    });
});

describe('open：复用 / 新开 / 分屏', () => {
    it('同一目标已活着就切过去，不再开后端会话', async () => {
        listMock.mockResolvedValue([info({ id: 'a' })]);
        const { terminalStore: store } = await freshStore();
        await store.bootstrap();
        const target: TerminalTarget = { kind: 'local' };
        const id = await store.open(target);
        expect(id).toBe('a');
        expect(openMock).not.toHaveBeenCalled();
        expect(store.getSnapshot().activeGroup).toBe(store.getSnapshot().groups[0].id);
    });

    it('shell 不同的本机会话不算同一个目标', async () => {
        listMock.mockResolvedValue([info({ id: 'a', shell: 'pwsh' })]);
        openMock.mockResolvedValue(info({ id: 'b', shell: 'cmd' }));
        const { terminalStore: store } = await freshStore();
        await store.bootstrap();
        const id = await store.open({ kind: 'local' }, { shell: 'cmd' as LocalShellKind });
        expect(id).toBe('b');
        expect(openMock).toHaveBeenCalledTimes(1);
    });

    it('forceNew 对活着的同目标也新开', async () => {
        listMock.mockResolvedValue([info({ id: 'a' })]);
        openMock.mockResolvedValue(info({ id: 'a2' }));
        const { terminalStore: store } = await freshStore();
        await store.bootstrap();
        const id = await store.open({ kind: 'local' }, { forceNew: true });
        expect(id).toBe('a2');
        expect(openMock).toHaveBeenCalledTimes(1);
        expect(store.getSnapshot().groups.map((g) => g.panes)).toEqual([['a'], ['a2']]);
    });

    it('splitFrom 并进源会话所在标签；满两块后落新标签', async () => {
        openMock
            .mockResolvedValueOnce(info({ id: 'a' }))
            .mockResolvedValueOnce(info({ id: 'b', target: { kind: 'server', server_id: 's1' } }))
            .mockResolvedValueOnce(info({ id: 'c', target: { kind: 'server', server_id: 's2' } }));
        const { terminalStore: store } = await freshStore();
        await store.open({ kind: 'local' });
        await store.open({ kind: 'server', server_id: 's1' }, { splitFrom: 'a', split: 'column' });
        const afterSplit = store.getSnapshot();
        expect(afterSplit.groups).toHaveLength(1);
        expect(afterSplit.groups[0].panes).toEqual(['a', 'b']);
        expect(afterSplit.groups[0].split).toBe('column');
        expect(afterSplit.groups[0].focused).toBe('b');
        await store.open({ kind: 'server', server_id: 's2' }, { splitFrom: 'a' });
        const afterThird = store.getSnapshot();
        expect(afterThird.groups.map((g) => g.panes)).toEqual([['a', 'b'], ['c']]);
    });

    it('后端开失败返回 null 并推错误条，busy 归位', async () => {
        openMock.mockRejectedValue('起不来');
        const { terminalStore: store } = await freshStore();
        const id = await store.open({ kind: 'local' });
        expect(id).toBeNull();
        expect(store.getSnapshot().busy).toBe(false);
        expect(pushErrorBarMock).toHaveBeenCalledWith(
            expect.objectContaining({ title: '终端没开起来' }),
        );
    });
});

describe('标签增删与顺序', () => {
    it('close 摘掉该 pane；空标签消失；activeGroup 挪到相邻标签', async () => {
        openMock
            .mockResolvedValueOnce(info({ id: 'a' }))
            .mockResolvedValueOnce(info({ id: 'b', target: { kind: 'server', server_id: 's1' } }))
            .mockResolvedValueOnce(info({ id: 'c', target: { kind: 'server', server_id: 's2' } }));
        const { terminalStore: store } = await freshStore();
        await store.open({ kind: 'local' });
        await store.open({ kind: 'server', server_id: 's1' });
        await store.open({ kind: 'server', server_id: 's2' });
        // 三个标签 g0=[a] g1=[b] g2=[c]，active 是 g2
        await store.close('c');
        const snap = store.getSnapshot();
        expect(snap.groups.map((g) => g.panes)).toEqual([['a'], ['b']]);
        // 原 active 在旧序里的下标 2 越界 → 夹到最后一个 [b]
        expect(snap.activeGroup).toBe(snap.groups[1].id);
        await store.close('a');
        expect(store.getSnapshot().groups.map((g) => g.panes)).toEqual([['b']]);
        expect(store.getSnapshot().activeGroup).toBe(store.getSnapshot().groups[0].id);
    });

    it('关掉两块标签里聚焦的那块，聚焦挪到剩下那块', async () => {
        openMock
            .mockResolvedValueOnce(info({ id: 'a' }))
            .mockResolvedValueOnce(info({ id: 'b', target: { kind: 'server', server_id: 's1' } }));
        const { terminalStore: store } = await freshStore();
        await store.open({ kind: 'local' });
        await store.open({ kind: 'server', server_id: 's1' }, { splitFrom: 'a' });
        await store.close('b');
        const snap = store.getSnapshot();
        expect(snap.groups).toHaveLength(1);
        expect(snap.groups[0].panes).toEqual(['a']);
        expect(snap.groups[0].focused).toBe('a');
    });

    it('最后一个标签关掉后面板收起、maximized 复位', async () => {
        openMock.mockResolvedValueOnce(info({ id: 'a' }));
        const { terminalStore: store } = await freshStore();
        await store.open({ kind: 'local' });
        store.setMaximized(true);
        await store.close('a');
        const snap = store.getSnapshot();
        expect(snap.groups).toEqual([]);
        expect(snap.activeGroup).toBeNull();
        expect(snap.open).toBe(false);
        expect(snap.maximized).toBe(false);
    });

    it('close / closeGroup 落盘：重建 store 后不再恢复已关标签', async () => {
        openMock
            .mockResolvedValueOnce(info({ id: 'a' }))
            .mockResolvedValueOnce(info({ id: 'b', target: { kind: 'server', server_id: 's1' } }));
        const first = await freshStore();
        const a = await first.terminalStore.open({ kind: 'local' });
        await first.terminalStore.open({ kind: 'server', server_id: 's1' }, { splitFrom: a! });
        await first.terminalStore.closeGroup(first.terminalStore.getSnapshot().groups[0].id);
        expect(persisted()?.groups).toEqual([]);
        expect(closeMock).toHaveBeenCalledTimes(2);
        listMock.mockResolvedValue([]);
        const second = await freshStore();
        await second.terminalStore.bootstrap();
        expect(second.terminalStore.getSnapshot().groups).toEqual([]);
    });

    it('moveGroup 调整顺序并持久化；越界下标不动', async () => {
        openMock
            .mockResolvedValueOnce(info({ id: 'a' }))
            .mockResolvedValueOnce(info({ id: 'b', target: { kind: 'server', server_id: 's1' } }));
        const { terminalStore: store } = await freshStore();
        await store.open({ kind: 'local' });
        await store.open({ kind: 'server', server_id: 's1' });
        const before = store.getSnapshot().groups.map((g) => g.id);
        store.moveGroup(1, 0);
        expect(store.getSnapshot().groups.map((g) => g.id)).toEqual([before[1], before[0]]);
        expect(persisted()?.groups.map((g) => g.id)).toEqual([before[1], before[0]]);
        store.moveGroup(5, 0);
        expect(store.getSnapshot().groups.map((g) => g.id)).toEqual([before[1], before[0]]);
    });
});

describe('面板与聚焦状态', () => {
    async function opened() {
        openMock
            .mockResolvedValueOnce(info({ id: 'a' }))
            .mockResolvedValueOnce(info({ id: 'b', target: { kind: 'server', server_id: 's1' } }));
        const mod = await freshStore();
        await mod.terminalStore.open({ kind: 'local' });
        await mod.terminalStore.open({ kind: 'server', server_id: 's1' }, { splitFrom: 'a' });
        return mod.terminalStore;
    }

    it('setOpen(true) 清掉当前标签的活动提示；isVisible 看面板 + 当前标签', async () => {
        const store = await opened();
        store.setOpen(false);
        store.setActivity('a', 'output');
        store.setActivity('b', 'ok');
        expect(store.isVisible('a')).toBe(false);
        store.setOpen(true);
        const snap = store.getSnapshot();
        // a 在当前标签 → 看过即清；b 同标签同样清
        expect(snap.sessions.a.activity).toBe('none');
        expect(snap.sessions.b.activity).toBe('none');
        expect(store.isVisible('a')).toBe(true);
        expect(store.isVisible('ghost')).toBe(false);
    });

    it('focusGroup 切标签并清该标签活动；越界 id 无操作', async () => {
        openMock
            .mockResolvedValueOnce(info({ id: 'a' }))
            .mockResolvedValueOnce(info({ id: 'b', target: { kind: 'server', server_id: 's1' } }));
        const { terminalStore: store } = await freshStore();
        await store.open({ kind: 'local' });
        await store.open({ kind: 'server', server_id: 's1' });
        store.setOpen(false);
        store.setActivity('a', 'fail');
        const groupIdB = store.getSnapshot().groups[1].id;
        store.focusGroup(groupIdB);
        const snap = store.getSnapshot();
        expect(snap.activeGroup).toBe(groupIdB);
        expect(snap.open).toBe(true);
        expect(snap.sessions.a.activity).toBe('fail');
        expect(() => store.focusGroup('g-nope')).not.toThrow();
        expect(store.getSnapshot().activeGroup).toBe(groupIdB);
    });

    it('toggle：没标签时直接开本机终端；有标签时开关面板', async () => {
        openMock.mockResolvedValueOnce(info({ id: 'a' }));
        const { terminalStore: store } = await freshStore();
        store.toggle();
        await flush();
        expect(openMock).toHaveBeenCalledWith(
            expect.objectContaining({ target: { kind: 'local' } }),
        );
        expect(store.getSnapshot().open).toBe(true);
        store.toggle();
        expect(store.getSnapshot().open).toBe(false);
        expect(openMock).toHaveBeenCalledTimes(1);
    });

    it('setMaximized(true) 强制展开；setOpen(false) 顺带收回最大化', async () => {
        const store = await opened();
        store.setOpen(false);
        store.setMaximized(true);
        expect(store.getSnapshot()).toMatchObject({ open: true, maximized: true });
        store.setOpen(false);
        expect(store.getSnapshot().maximized).toBe(false);
    });
});

describe('会话回报与越界容错', () => {
    async function oneSession() {
        openMock.mockResolvedValueOnce(info({ id: 'a', cwd: 'C:\\start' }));
        const mod = await freshStore();
        await mod.terminalStore.open({ kind: 'local' });
        return mod.terminalStore;
    }

    it('setActivity：ok/fail 不被 output 盖掉；同值重复调用不换引用', async () => {
        const store = await oneSession();
        store.setActivity('a', 'ok');
        store.setActivity('a', 'output');
        expect(store.getSnapshot().sessions.a.activity).toBe('ok');
        store.setActivity('a', 'fail');
        expect(store.getSnapshot().sessions.a.activity).toBe('fail');
        const before = store.getSnapshot();
        store.setActivity('a', 'fail');
        expect(store.getSnapshot()).toBe(before);
    });

    it('setStatus 写回 info 并复位 sudoPrompt；updateInfo 缺 cwd 时保留旧值', async () => {
        const store = await oneSession();
        store.setSudoPrompt('a', true);
        const exited: TerminalStatus = { kind: 'exited', code: 0 };
        store.setStatus('a', exited);
        let snap = store.getSnapshot();
        expect(snap.sessions.a.info.status).toEqual(exited);
        expect(snap.sessions.a.sudoPrompt).toBe(false);
        store.updateInfo('a', info({ id: 'a', cwd: undefined }));
        snap = store.getSnapshot();
        expect(snap.sessions.a.cwd).toBe('C:\\start');
        store.setCwd('a', 'D:\\other');
        expect(store.getSnapshot().sessions.a.cwd).toBe('D:\\other');
    });

    it('rename 去空白；空串与纯空白落回 null，sessionTitle 跟着回退', async () => {
        const { sessionTitle } = await freshStore();
        const store = await oneSession();
        store.rename('a', '  麦麦 shell  ');
        expect(store.getSnapshot().sessions.a.customTitle).toBe('麦麦 shell');
        expect(sessionTitle(store.getSnapshot().sessions.a)).toBe('麦麦 shell');
        store.rename('a', '   ');
        expect(store.getSnapshot().sessions.a.customTitle).toBeNull();
        expect(sessionTitle(store.getSnapshot().sessions.a)).toBe('会话');
    });

    it('setFilesOpen 同步成以后新开的默认；分出来的第二块不带文件栏', async () => {
        openMock.mockResolvedValueOnce(info({ id: 'a' }));
        const mod = await freshStore();
        // 动态 import 与 terminalStore 同代际，拿到的就是它内部用的那份 layout；
        // 静态 import 会绑到 resetModules 之前的旧代际，断言不到同一实例
        const { terminalLayout } = await import('./terminalPrefs');
        await mod.terminalStore.open({ kind: 'local' });
        mod.terminalStore.setFilesOpen('a', true);
        expect(mod.terminalStore.getSnapshot().sessions.a.filesOpen).toBe(true);
        expect(terminalLayout.get().filesOpen).toBe(true);
        openMock.mockResolvedValueOnce(
            info({ id: 'b', target: { kind: 'server', server_id: 's1' } }),
        );
        await mod.terminalStore.open({ kind: 'server', server_id: 's1' }, { splitFrom: 'a' });
        const snap = mod.terminalStore.getSnapshot();
        expect(snap.sessions.b.filesOpen).toBe(false);
    });

    it('所有按 id 修改的入口对越界 id 静默无操作', async () => {
        const store = await oneSession();
        const before = store.getSnapshot();
        store.rename('ghost', 'x');
        store.setCwd('ghost', 'x');
        store.setActivity('ghost', 'output');
        store.setProgress('ghost', { state: 1, value: 1 });
        store.setSudoPrompt('ghost', true);
        store.setStatus('ghost', { kind: 'running' });
        store.updateInfo('ghost', info({ id: 'ghost' }));
        store.focusPane('ghost');
        store.setRatio('g-ghost', 0.5);
        store.setSplit('g-ghost', 'column');
        await store.restart('ghost');
        await store.closeGroup('g-ghost');
        // restart 不查存在性，service 照样被调；只是 patch 不到会话、状态不变
        expect(store.getSnapshot()).toEqual(before);
        expect(closeMock).not.toHaveBeenCalled();
        expect(restartMock).toHaveBeenCalledWith('ghost');
    });

    it('setRatio 夹在 [0.15, 0.85]；setSplit 换向后两者都落盘', async () => {
        openMock
            .mockResolvedValueOnce(info({ id: 'a' }))
            .mockResolvedValueOnce(info({ id: 'b', target: { kind: 'server', server_id: 's1' } }));
        const { terminalStore: store } = await freshStore();
        await store.open({ kind: 'local' });
        const groupId = store.getSnapshot().groups[0].id;
        await store.open({ kind: 'server', server_id: 's1' }, { splitFrom: 'a' });
        store.setRatio(groupId, 3);
        expect(store.getSnapshot().groups[0].ratio).toBe(0.85);
        store.setRatio(groupId, -2);
        expect(store.getSnapshot().groups[0].ratio).toBe(0.15);
        store.setSplit(groupId, 'column');
        expect(store.getSnapshot().groups[0].split).toBe('column');
        expect(persisted()?.groups[0]).toMatchObject({ ratio: 0.15, split: 'column' });
    });
});

describe('纯导出', () => {
    it('isLive 只认 running / starting', async () => {
        const { isLive } = await freshStore();
        expect(isLive({ kind: 'starting' })).toBe(true);
        expect(isLive({ kind: 'running' })).toBe(true);
        expect(isLive({ kind: 'exited', code: 0 })).toBe(false);
        expect(isLive({ kind: 'disconnected', reason: 'r' })).toBe(false);
        expect(isLive({ kind: 'failed', message: 'm' })).toBe(false);
    });

    it('openTerminal 转发到 terminalStore.open', async () => {
        openMock.mockResolvedValueOnce(info({ id: 'a' }));
        const mod = await freshStore();
        const id = await mod.openTerminal({ kind: 'local' }, { shell: 'git_bash' });
        expect(id).toBe('a');
        expect(openMock).toHaveBeenCalledWith(
            expect.objectContaining({
                target: { kind: 'local' },
                shell: 'git_bash',
            }),
        );
    });

    it('closeAll 先 bootstrap 再逐个关', async () => {
        listMock.mockResolvedValue([info({ id: 'a' }), info({ id: 'b' })]);
        const { terminalStore: store } = await freshStore();
        await store.closeAll();
        expect(listMock).toHaveBeenCalledTimes(1);
        expect(closeMock.mock.calls.map((c) => c[0]).sort()).toEqual(['a', 'b']);
        expect(store.getSnapshot().sessions).toEqual({});
    });
});
