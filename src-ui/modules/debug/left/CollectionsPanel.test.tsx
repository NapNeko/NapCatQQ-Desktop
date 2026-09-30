import type { ReactNode } from 'react';
import { beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import type { DebugCallRequest } from '../../../core/ipc/generated/debug/DebugCallRequest';
import type { DebugCatalog } from '../../../core/ipc/generated/debug/DebugCatalog';
import type { DebugCollections } from '../../../core/ipc/generated/debug/DebugCollections';
import type { DebugSavedRequest } from '../../../core/ipc/generated/debug/DebugSavedRequest';
import type { DebugTarget } from '../../../core/ipc/generated/debug/DebugTarget';

const service: Record<string, ReturnType<typeof vi.fn>> = {
    collections: vi.fn(),
    saveCollections: vi.fn(),
    catalog: vi.fn(),
    call: vi.fn(),
};

vi.mock('../../../core/services/onebot-debug.service', () => ({
    onebotDebugService: new Proxy(
        {},
        {
            get: (_t, key: string) => (...args: unknown[]) => {
                const fn = service[key];
                if (!fn) throw new Error(`没有模拟 service.${key}`);
                return fn(...args);
            },
        },
    ),
}));
vi.mock('../../../hooks/ui/pushErrorBar', () => ({ pushErrorBar: vi.fn() }));

import { TooltipProvider } from '../../../shared/ui';
import { preferencesStore } from '../../../hooks/preferences/preferencesStore';
import { debugWorkspaceStore } from '../../../hooks/debug/debugWorkspaceStore';
import { _resetDebugCatalogForTests } from '../../../hooks/debug/useDebugCatalog';
import { _resetDangerSkipsForTests, dangerConfirmSkipped } from '../DangerConfirmDialog';
import { LeftColumn } from './LeftColumn';

beforeAll(() => {
    Element.prototype.hasPointerCapture ??= () => false;
    Element.prototype.releasePointerCapture ??= () => {};
    Element.prototype.setPointerCapture ??= () => {};
});

const SL: DebugTarget = {
    bot_id: 'bot-sl',
    name: '小雪',
    qq_id: 2854196310,
    backend: 'snowluma',
    host: { kind: 'local' },
    running: true,
    online: true,
};

function saved(id: string, folder: string | null, order: number, patch: Partial<DebugSavedRequest> = {}): DebugSavedRequest {
    return {
        id,
        name: `收藏 ${id}`,
        folder_id: folder,
        action: 'get_login_info',
        params: {},
        channel: null,
        note: null,
        order,
        created_at_ms: 1000 + order,
        updated_at_ms: 1000 + order,
        ...patch,
    };
}

const SEED: DebugCollections = {
    version: 1,
    folders: [{ id: 'f1', name: '常用', order: 0 }],
    requests: [
        saved('a', 'f1', 0, { name: '看看登录号' }),
        saved('b', 'f1', 1, { name: '测试群打招呼', action: 'send_group_msg', params: { group_id: 1, message: 'hi' } }),
        saved('r', null, 0, { name: '群列表', action: 'get_group_list', channel: { kind: 'internal' } }),
        saved('k', null, 1, { name: '踢人', action: 'set_group_kick', params: { group_id: 1, user_id: 2 } }),
    ],
};

const CATALOG: DebugCatalog = {
    backend: 'snowluma',
    source: 'live',
    snapshot_version: '0.9.0',
    actions: (
        [
            ['get_login_info', 'read_only'],
            ['send_group_msg', 'side_effect'],
            ['get_group_list', 'read_only'],
            ['set_group_kick', 'dangerous'],
        ] as const
    ).map(([name, safety]) => ({
        name,
        aliases: [],
        summary: '',
        category: 'message' as const,
        safety,
        stream: false,
        supported: true,
        other_backend_present: true,
        param_diff: false,
    })),
};

let disk: DebugCollections = SEED;

function renderCollections(target: DebugTarget | null = SL) {
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    const wrapper = ({ children }: { children: ReactNode }) => (
        <QueryClientProvider client={client}>
            <TooltipProvider>{children}</TooltipProvider>
        </QueryClientProvider>
    );
    return render(<LeftColumn target={target} panel="collections" />, { wrapper });
}

const lastSaved = (): DebugCollections => service.saveCollections.mock.calls.at(-1)?.[0] as DebugCollections;
const rowOf = (name: string) => screen.getByText(name).closest('[role="treeitem"]') as HTMLElement;

beforeEach(() => {
    for (const fn of Object.values(service)) fn.mockReset();
    preferencesStore.setMotionEnabled(false);
    debugWorkspaceStore._reset();
    _resetDebugCatalogForTests();
    _resetDangerSkipsForTests();
    disk = SEED;
    service.collections.mockImplementation(async () => disk);
    service.saveCollections.mockImplementation(async (next: DebugCollections) => {
        disk = next;
    });
    service.catalog.mockResolvedValue(CATALOG);
    service.call.mockImplementation(async (req: DebugCallRequest) => ({
        request_id: req.request_id,
        result: {
            kind: 'ok',
            outcome: {
                ok: true,
                status: 'ok',
                retcode: 0,
                data: null,
                message: '',
                wording: '',
                raw: {},
                elapsed_ms: 5,
                channel: { kind: 'internal' },
                size_bytes: 2,
                truncated: false,
            },
        },
    }));
});

describe('收藏', () => {
    it('文件夹在前、根目录请求在后；文件夹可以收起', async () => {
        renderCollections();
        await screen.findByText('看看登录号');
        const names = screen.getAllByRole('treeitem').map((el) => el.getAttribute('aria-label'));
        expect(names).toEqual(['文件夹 常用，2 个请求', '看看登录号（get_login_info）', '测试群打招呼（send_group_msg）', '群列表（get_group_list）', '踢人（set_group_kick）']);
        fireEvent.click(rowOf('常用'));
        expect(screen.queryByText('看看登录号')).not.toBeInTheDocument();
        fireEvent.click(rowOf('常用'));
        expect(screen.getByText('看看登录号')).toBeInTheDocument();
    });

    it('单击打开：参数写进标签，指定了通道的一起带上；Ctrl+单击另开', async () => {
        const open = vi.spyOn(debugWorkspaceStore, 'openAction');
        const setChannel = vi.spyOn(debugWorkspaceStore, 'setTabChannel');
        renderCollections();
        fireEvent.click(await screen.findByText('群列表'));
        expect(open).toHaveBeenCalledWith('get_group_list', { newTab: false, paramsText: '{}' });
        expect(setChannel).toHaveBeenCalledWith(open.mock.results[0]?.value, { kind: 'internal' });

        fireEvent.click(screen.getByText('测试群打招呼'), { ctrlKey: true });
        expect(open).toHaveBeenLastCalledWith('send_group_msg', {
            newTab: true,
            paramsText: '{\n  "group_id": 1,\n  "message": "hi"\n}',
        });
    });

    it('▶ 在当前 Bot 上发：打开成标签再发，走 Bot 选的调用通道', async () => {
        const open = vi.spyOn(debugWorkspaceStore, 'openAction');
        const user = userEvent.setup();
        renderCollections();
        await screen.findByText('看看登录号');
        await waitFor(() => expect(service.catalog).toHaveBeenCalled());
        await user.click(within(rowOf('看看登录号')).getByRole('button', { name: '在当前 Bot 上发送' }));
        await waitFor(() => expect(service.call).toHaveBeenCalledTimes(1));
        expect(service.call.mock.calls[0]?.[0]).toMatchObject({
            bot_id: 'bot-sl',
            action: 'get_login_info',
            channel: { kind: 'auto' },
            params: {},
            origin: 'editor',
        });
        const tabId = open.mock.results[0]?.value as string;
        await waitFor(() => expect(debugWorkspaceStore.getRun(tabId)?.last?.action).toBe('get_login_info'));
    });

    it('危险接口先确认再发', async () => {
        const user = userEvent.setup();
        renderCollections();
        await screen.findByText('踢人');
        await waitFor(() => expect(service.catalog).toHaveBeenCalled());
        await user.click(within(rowOf('踢人')).getByRole('button', { name: '在当前 Bot 上发送' }));
        expect(await screen.findByRole('heading', { name: '确认调用 set_group_kick？' })).toBeInTheDocument();
        expect(screen.getByText(/会把 2 移出群 1/)).toBeInTheDocument();
        expect(screen.getByText('这是收藏「踢人」，会发给 小雪。')).toBeInTheDocument();
        expect(service.call).not.toHaveBeenCalled();
        fireEvent.click(screen.getByRole('button', { name: '确认调用' }));
        await waitFor(() => expect(service.call).toHaveBeenCalledTimes(1));
    });

    it('一键发送用的是中栏同一个确认框：勾了「本次不再询问」之后，两边都不再问', async () => {
        const user = userEvent.setup();
        renderCollections();
        await screen.findByText('踢人');
        await waitFor(() => expect(service.catalog).toHaveBeenCalled());
        await user.click(within(rowOf('踢人')).getByRole('button', { name: '在当前 Bot 上发送' }));
        await screen.findByRole('heading', { name: '确认调用 set_group_kick？' });
        // Radix 模态期间 body 是 pointer-events: none，user-event 会拒绝点；直接派发点击
        fireEvent.click(screen.getByRole('checkbox', { name: /本次不再询问/ }));
        fireEvent.click(screen.getByRole('button', { name: '确认调用' }));
        await waitFor(() => expect(service.call).toHaveBeenCalledTimes(1));
        expect(dangerConfirmSkipped('bot-sl', 'set_group_kick')).toBe(true);

        await user.click(within(rowOf('踢人')).getByRole('button', { name: '在当前 Bot 上发送' }));
        await waitFor(() => expect(service.call).toHaveBeenCalledTimes(2));
        expect(screen.queryByRole('heading', { name: '确认调用 set_group_kick？' })).not.toBeInTheDocument();
    });

    it('Bot 没在运行时发送按钮禁用', async () => {
        renderCollections({ ...SL, running: false });
        await screen.findByText('看看登录号');
        expect(within(rowOf('看看登录号')).getByRole('button', { name: '在当前 Bot 上发送' })).toBeDisabled();
    });

    it('行内改名：回车保存，Esc 放弃', async () => {
        const user = userEvent.setup();
        renderCollections();
        await screen.findByText('群列表');
        await user.click(within(rowOf('群列表')).getByRole('button', { name: '重命名' }));
        const input = screen.getByRole('textbox', { name: '收藏名' });
        expect(input).toHaveFocus();
        await user.clear(input);
        await user.type(input, '所有群{Enter}');
        await waitFor(() => expect(service.saveCollections).toHaveBeenCalledTimes(1));
        expect(lastSaved().requests.find((r) => r.id === 'r')?.name).toBe('所有群');
        expect(await screen.findByText('所有群')).toBeInTheDocument();

        await user.click(within(rowOf('所有群')).getByRole('button', { name: '重命名' }));
        await user.type(screen.getByRole('textbox', { name: '收藏名' }), 'xxx{Escape}');
        expect(screen.getByText('所有群')).toBeInTheDocument();
        expect(service.saveCollections).toHaveBeenCalledTimes(1);
    });

    it('删除前确认；删文件夹会说明里面有几个请求', async () => {
        const user = userEvent.setup();
        renderCollections();
        await screen.findByText('群列表');
        await user.click(within(rowOf('群列表')).getByRole('button', { name: '删除' }));
        expect(await screen.findByRole('heading', { name: '删除这个收藏？' })).toBeInTheDocument();
        fireEvent.click(screen.getByRole('button', { name: '删除' }));
        await waitFor(() => expect(service.saveCollections).toHaveBeenCalledTimes(1));
        expect(lastSaved().requests.map((r) => r.id)).toEqual(['a', 'b', 'k']);

        await waitFor(() => expect(screen.queryByRole('heading', { name: '删除这个收藏？' })).not.toBeInTheDocument());
        await user.click(within(rowOf('常用')).getByRole('button', { name: '删除文件夹' }));
        expect(await screen.findByText(/和里面的 2 个请求会一起删掉/)).toBeInTheDocument();
    });

    it('新建文件夹后直接进入改名', async () => {
        const user = userEvent.setup();
        renderCollections();
        await screen.findByText('群列表');
        await user.click(screen.getByRole('button', { name: '新建文件夹' }));
        await waitFor(() => expect(service.saveCollections).toHaveBeenCalledTimes(1));
        expect(lastSaved().folders.map((f) => f.name)).toEqual(['常用', '新建文件夹']);
        const input = await screen.findByRole('textbox', { name: '文件夹名' });
        await user.clear(input);
        await user.type(input, '群管理{Enter}');
        await waitFor(() => expect(lastSaved().folders.map((f) => f.name)).toEqual(['常用', '群管理']));
    });

    it('键盘：↓ 移到下一行，Alt+↓ 把请求往下挪一格', async () => {
        const user = userEvent.setup();
        renderCollections();
        await screen.findByText('看看登录号');
        act(() => rowOf('看看登录号').focus());
        await user.keyboard('{ArrowDown}');
        expect(rowOf('测试群打招呼')).toHaveFocus();
        await user.keyboard('{ArrowUp}{Alt>}{ArrowDown}{/Alt}');
        await waitFor(() => expect(service.saveCollections).toHaveBeenCalledTimes(1));
        const f1 = lastSaved()
            .requests.filter((r) => r.folder_id === 'f1')
            .sort((x, y) => x.order - y.order)
            .map((r) => r.id);
        expect(f1).toEqual(['b', 'a']);
    });

    it('右键「移到」把请求放进文件夹', async () => {
        renderCollections();
        await screen.findByText('群列表');
        fireEvent.contextMenu(rowOf('群列表'));
        fireEvent.pointerDown(await screen.findByRole('menuitem', { name: /移到/ }), { pointerType: 'mouse' });
        fireEvent.keyDown(screen.getByRole('menuitem', { name: /移到/ }), { key: 'ArrowRight' });
        fireEvent.click(await screen.findByRole('menuitem', { name: '常用' }));
        await waitFor(() => expect(service.saveCollections).toHaveBeenCalledTimes(1));
        expect(lastSaved().requests.find((r) => r.id === 'r')).toMatchObject({ folder_id: 'f1', order: 2 });
    });

    it('没有收藏时给出说明和导入入口', async () => {
        disk = { version: 1, folders: [], requests: [] };
        renderCollections();
        expect(await screen.findByText('还没有收藏')).toBeInTheDocument();
        // 标题行的图标按钮和空状态里的大按钮都能导入
        expect(screen.getAllByRole('button', { name: /导入收藏/ })).toHaveLength(2);
        expect(screen.getByRole('button', { name: '导出收藏' })).toBeDisabled();
    });

    it('右键菜单里点重命名：改名框不会一闪就没，回车保存后焦点回到行上', async () => {
        const user = userEvent.setup();
        renderCollections();
        await screen.findByText('群列表');
        // 真实的右键先按下鼠标，行会拿到焦点；菜单关上时 Radix 要把焦点还给它
        act(() => rowOf('群列表').focus());
        fireEvent.contextMenu(rowOf('群列表'));
        fireEvent.click(await screen.findByRole('menuitem', { name: '重命名' }));
        const input = await screen.findByRole('textbox', { name: '收藏名' });
        await waitFor(() => expect(input).toHaveFocus());
        // 菜单关上后 Radix 还焦点的那一下（下一个任务里）不能把改名框顶掉
        await new Promise((r) => setTimeout(r, 50));
        expect(screen.getByRole('textbox', { name: '收藏名' })).toHaveFocus();
        expect(service.saveCollections).not.toHaveBeenCalled();

        await user.clear(input);
        await user.type(input, '所有群{Enter}');
        await waitFor(() => expect(lastSaved().requests.find((r) => r.id === 'r')?.name).toBe('所有群'));
        await waitFor(() => expect(rowOf('所有群')).toHaveFocus());
    });

    it('键盘 Delete 删掉一行后焦点落到下一行', async () => {
        renderCollections();
        await screen.findByText('看看登录号');
        act(() => rowOf('看看登录号').focus());
        fireEvent.keyDown(rowOf('看看登录号'), { key: 'Delete' });
        expect(await screen.findByRole('heading', { name: '删除这个收藏？' })).toBeInTheDocument();
        fireEvent.click(screen.getByRole('button', { name: '删除' }));
        await waitFor(() => expect(service.saveCollections).toHaveBeenCalledTimes(1));
        await waitFor(() => expect(rowOf('测试群打招呼')).toHaveFocus());
    });

    it('悬停按钮不进 Tab 顺序（键盘走行上的快捷键）', async () => {
        renderCollections();
        await screen.findByText('看看登录号');
        for (const name of ['在当前 Bot 上发送', '打开', '重命名', '删除']) {
            expect(within(rowOf('看看登录号')).getByRole('button', { name })).toHaveAttribute('tabindex', '-1');
        }
    });

    it('目录读失败时一键发送先确认，确认后才发', async () => {
        service.catalog.mockRejectedValue(new Error('连不上 WebUI'));
        const user = userEvent.setup();
        renderCollections();
        await screen.findByText('看看登录号');
        await waitFor(() => expect(service.catalog).toHaveBeenCalled());
        const send = within(rowOf('看看登录号')).getByRole('button', { name: '在当前 Bot 上发送' });
        await waitFor(() => expect(send).not.toBeDisabled());
        await user.click(send);
        expect(await screen.findByRole('heading', { name: '确认调用 get_login_info？' })).toBeInTheDocument();
        expect(screen.getByText(/接口目录没读出来/)).toBeInTheDocument();
        expect(service.call).not.toHaveBeenCalled();
        fireEvent.click(screen.getByRole('button', { name: '确认调用' }));
        await waitFor(() => expect(service.call).toHaveBeenCalledTimes(1));
    });

    it('目录里没有的接口一键发送也先确认', async () => {
        disk = { ...SEED, requests: [...SEED.requests, saved('w', null, 2, { name: '神秘接口', action: 'weird_action' })] };
        const user = userEvent.setup();
        renderCollections();
        await screen.findByText('神秘接口');
        await waitFor(() => expect(service.catalog).toHaveBeenCalled());
        const send = within(rowOf('神秘接口')).getByRole('button', { name: '在当前 Bot 上发送' });
        await waitFor(() => expect(send).not.toBeDisabled());
        await user.click(send);
        expect(await screen.findByText(/当前 Bot 的接口目录里没有这个接口/)).toBeInTheDocument();
        expect(service.call).not.toHaveBeenCalled();
    });

    it('目录还在读时发送按钮禁用', async () => {
        service.catalog.mockReturnValue(new Promise(() => {}));
        renderCollections();
        await screen.findByText('看看登录号');
        expect(within(rowOf('看看登录号')).getByRole('button', { name: '在当前 Bot 上发送' })).toBeDisabled();
    });

    it('拖动中按 Esc 放弃：不保存；手还按着和刚松手时补来的点击都不打开请求', async () => {
        const open = vi.spyOn(debugWorkspaceStore, 'openAction');
        renderCollections();
        await screen.findByText('群列表');
        const row = rowOf('群列表');
        fireEvent.pointerDown(row, { button: 0, pointerId: 1, clientX: 20, clientY: 20 });
        fireEvent.pointerMove(row, { pointerId: 1, clientX: 20, clientY: 60 });
        // 拖起来了：跟着指针的那块也写着名字
        expect(screen.getAllByText('群列表')).toHaveLength(2);

        fireEvent.keyDown(window, { key: 'Escape' });
        expect(screen.getAllByText('群列表')).toHaveLength(1);
        fireEvent.click(row);
        expect(open).not.toHaveBeenCalled();

        fireEvent.pointerUp(window, { pointerId: 1 });
        fireEvent.click(row);
        expect(open).not.toHaveBeenCalled();
        expect(service.saveCollections).not.toHaveBeenCalled();

        await new Promise((r) => setTimeout(r, 300));
        fireEvent.click(row);
        expect(open).toHaveBeenCalledTimes(1);
    });

    it('Esc 放弃后等来的是 pointercancel（触屏、被系统手势打断）也算松手，之后的点击照常', async () => {
        const open = vi.spyOn(debugWorkspaceStore, 'openAction');
        renderCollections();
        await screen.findByText('群列表');
        const row = rowOf('群列表');
        fireEvent.pointerDown(row, { button: 0, pointerId: 1, clientX: 20, clientY: 20 });
        fireEvent.pointerMove(row, { pointerId: 1, clientX: 20, clientY: 60 });
        fireEvent.keyDown(window, { key: 'Escape' });
        fireEvent.pointerCancel(window, { pointerId: 1 });
        fireEvent.click(row);
        expect(open).not.toHaveBeenCalled();
        // 松手后的 250ms 过了就能点；只等 2 秒兜底的话这里还点不开
        await new Promise((r) => setTimeout(r, 300));
        fireEvent.click(row);
        expect(open).toHaveBeenCalledTimes(1);
    });

    it('指针捕获被抢走时当作放弃，不落到半路的位置', async () => {
        renderCollections();
        await screen.findByText('群列表');
        const row = rowOf('群列表');
        fireEvent.pointerDown(row, { button: 0, pointerId: 1, clientX: 20, clientY: 20 });
        fireEvent.pointerMove(row, { pointerId: 1, clientX: 20, clientY: 80 });
        expect(screen.getAllByText('群列表')).toHaveLength(2);
        fireEvent.lostPointerCapture(screen.getByRole('tree', { name: '收藏' }), { pointerId: 1 });
        expect(screen.getAllByText('群列表')).toHaveLength(1);
        await new Promise((r) => setTimeout(r, 20));
        expect(service.saveCollections).not.toHaveBeenCalled();
    });
});
