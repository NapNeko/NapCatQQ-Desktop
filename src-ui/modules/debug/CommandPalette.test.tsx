import { useState, type ReactNode } from 'react';
import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import type { DebugActionSummary } from '../../core/ipc/generated/debug/DebugActionSummary';
import type { DebugCollections } from '../../core/ipc/generated/debug/DebugCollections';
import type { DebugTarget } from '../../core/ipc/generated/debug/DebugTarget';

const service: Record<string, ReturnType<typeof vi.fn>> = {
    catalog: vi.fn(),
    collections: vi.fn(),
    call: vi.fn(),
    workspace: vi.fn(),
    saveWorkspace: vi.fn(),
};

vi.mock('../../core/services/onebot-debug.service', () => ({
    onebotDebugService: new Proxy(
        {},
        {
            get:
                (_t, key: string) =>
                (...args: unknown[]) => {
                    const fn = service[key];
                    if (!fn) throw new Error(`没有模拟 service.${key}`);
                    return fn(...args);
                },
        },
    ),
}));
vi.mock('../../hooks/ui/pushErrorBar', () => ({ pushErrorBar: vi.fn() }));

import { TooltipProvider } from '../../shared/ui';
import { preferencesStore } from '../../core/domain/settings/preferencesStore';
import { debugWorkspaceStore, defaultWorkspace } from '../../hooks/debug/debugWorkspaceStore';
import { _resetDebugCatalogForTests } from '../../hooks/debug/useDebugCatalog';
import { CommandPalette, _resetCommandPaletteForTests } from './CommandPalette';

// 虚拟列表靠滚动容器的高度决定画几行；jsdom 里一律是 0，给个像样的值
const offsetHeight = Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'offsetHeight');
const offsetWidth = Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'offsetWidth');
beforeAll(() => {
    Object.defineProperty(HTMLElement.prototype, 'offsetHeight', {
        configurable: true,
        get() {
            return (this as HTMLElement).classList.contains('overflow-y-auto') ? 420 : 34;
        },
    });
    Object.defineProperty(HTMLElement.prototype, 'offsetWidth', {
        configurable: true,
        get: () => 640,
    });
});
afterAll(() => {
    if (offsetHeight) Object.defineProperty(HTMLElement.prototype, 'offsetHeight', offsetHeight);
    if (offsetWidth) Object.defineProperty(HTMLElement.prototype, 'offsetWidth', offsetWidth);
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

const action = (
    name: string,
    summary: string,
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
    action('delete_msg', '撤回消息', { safety: 'dangerous' }),
    action('get_group_album_list', '获取群相册列表', { other_backend_present: false }),
];

const COLLECTIONS: DebugCollections = {
    version: 1,
    folders: [],
    requests: [
        {
            id: 'req-hello',
            name: '测试群打招呼',
            folder_id: null,
            action: 'send_group_msg',
            params: { group_id: 100001, message: 'hi' },
            channel: { kind: 'internal' },
            note: null,
            order: 0,
            created_at_ms: 1,
            updated_at_ms: 1,
        },
    ],
};

function Harness({
    target = SL,
    onChange,
    outside,
}: {
    target?: DebugTarget | null;
    onChange?: (open: boolean) => void;
    /** 面板外面的东西：比如冒充新标签请求头的「接口名」输入框 */
    outside?: ReactNode;
}) {
    const [open, setOpen] = useState(true);
    return (
        <>
            <button type="button" onClick={() => setOpen(true)}>
                打开面板
            </button>
            {outside}
            <CommandPalette
                open={open}
                onOpenChange={(o) => {
                    onChange?.(o);
                    setOpen(o);
                }}
                target={target}
            />
        </>
    );
}

function renderPalette(
    props: {
        target?: DebugTarget | null;
        onChange?: (open: boolean) => void;
        outside?: ReactNode;
    } = {},
) {
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    const wrapper = ({ children }: { children: ReactNode }) => (
        <QueryClientProvider client={client}>
            <TooltipProvider>{children}</TooltipProvider>
        </QueryClientProvider>
    );
    return render(<Harness {...props} />, { wrapper });
}

const input = () => screen.getByRole('combobox', { name: '搜索接口' });
const option = (name: RegExp | string) => screen.getByRole('option', { name });
const selected = () =>
    screen.getAllByRole('option').find((o) => o.getAttribute('aria-selected') === 'true');

beforeEach(async () => {
    for (const fn of Object.values(service)) fn.mockReset();
    preferencesStore.setMotionEnabled(false);
    debugWorkspaceStore._reset();
    _resetDebugCatalogForTests();
    _resetCommandPaletteForTests();
    service.catalog.mockResolvedValue({
        backend: 'snowluma',
        source: 'live',
        snapshot_version: 't',
        actions: ACTIONS,
    });
    service.collections.mockResolvedValue(COLLECTIONS);
    service.workspace.mockResolvedValue({
        ...defaultWorkspace(),
        recent_actions: ['get_login_info'],
    });
    service.saveWorkspace.mockResolvedValue(undefined);
    await debugWorkspaceStore.load();
});

afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
});

describe('命令面板', () => {
    it('打开就能打字；没输入时先列最近用过、收藏，再是全部接口，行上有分级和标记', async () => {
        renderPalette();
        expect(await screen.findByRole('dialog', { name: '搜索接口' })).toBeInTheDocument();
        await waitFor(() => expect(input()).toHaveFocus());
        await screen.findByRole('option', { name: /^get_login_info/ });
        const names = screen.getAllByRole('option').map((o) => o.getAttribute('aria-label'));
        expect(names[0]).toBe('get_login_info，获取登录号信息，只读');
        expect(names[1]).toBe('打开收藏：测试群打招呼（send_group_msg），有副作用');
        expect(names).toContain('delete_msg，撤回消息，危险');
        // 第一条能选的默认高亮
        expect(selected()).toBe(option(/^get_login_info/));
        expect(input()).toHaveAttribute('aria-activedescendant', selected()?.id);
        expect(screen.getByText('仅 SL')).toBeInTheDocument();
        expect(screen.getByText('只打开，不会发送')).toBeInTheDocument();
    });

    it('输入筛选；回车在当前标签打开、记进最近用过、关掉面板，不发送', async () => {
        const user = userEvent.setup();
        const onChange = vi.fn();
        renderPalette({ onChange });
        const open = vi.spyOn(debugWorkspaceStore, 'openAction');
        await screen.findByRole('option', { name: /^get_login_info/ });
        await user.type(input(), '群列表');
        await waitFor(() => expect(screen.getAllByRole('option')).toHaveLength(1));
        await user.keyboard('{Enter}');
        expect(open).toHaveBeenCalledWith('get_group_list', { newTab: false });
        expect(debugWorkspaceStore.getSnapshot().ws.recent_actions[0]).toBe('get_group_list');
        expect(onChange).toHaveBeenCalledWith(false);
        expect(service.call).not.toHaveBeenCalled();
    });

    it('Ctrl+回车在新标签打开', async () => {
        const user = userEvent.setup();
        renderPalette();
        const open = vi.spyOn(debugWorkspaceStore, 'openAction');
        await screen.findByRole('option', { name: /^get_login_info/ });
        await user.type(input(), 'delete');
        await user.keyboard('{Control>}{Enter}{/Control}');
        expect(open).toHaveBeenCalledWith('delete_msg', { newTab: true });
        expect(service.call).not.toHaveBeenCalled();
    });

    it('↑↓ 跳过分段标题移动高亮，到头停住', async () => {
        const user = userEvent.setup();
        renderPalette();
        await screen.findByRole('option', { name: /^get_login_info/ });
        await user.keyboard('{ArrowUp}');
        expect(selected()).toBe(option(/^get_login_info/));
        await user.keyboard('{ArrowDown}');
        expect(selected()).toBe(option(/^打开收藏：测试群打招呼/));
        await user.keyboard('{ArrowDown}');
        // 越过「全部接口」标题，落到按名字排的第一个
        expect(selected()).toBe(option(/^delete_msg/));
        expect(input()).toHaveAttribute('aria-activedescendant', selected()?.id);
    });

    it('打开收藏：参数和指定的通道一起带进标签，不发送', async () => {
        const user = userEvent.setup();
        renderPalette();
        await screen.findByRole('option', { name: /^get_login_info/ });
        await user.type(input(), '打招呼');
        await waitFor(() => expect(selected()).toBe(option(/^打开收藏：测试群打招呼/)));
        await user.keyboard('{Enter}');
        const ws = debugWorkspaceStore.getSnapshot().ws;
        const tab = ws.tabs.find((t) => t.id === ws.active_tab);
        expect(tab).toMatchObject({ action: 'send_group_msg', channel: { kind: 'internal' } });
        expect(JSON.parse(tab?.params_text ?? '')).toEqual({ group_id: 100001, message: 'hi' });
        expect(service.call).not.toHaveBeenCalled();
    });

    it('鼠标移上去才换高亮（指针正好停着不算），点一下打开', async () => {
        const onChange = vi.fn();
        renderPalette({ onChange });
        const open = vi.spyOn(debugWorkspaceStore, 'openAction');
        await screen.findByRole('option', { name: /^get_login_info/ });
        fireEvent.mouseEnter(option(/^delete_msg/));
        expect(selected()).toBe(option(/^get_login_info/));
        fireEvent.mouseMove(option(/^delete_msg/));
        expect(selected()).toBe(option(/^delete_msg/));
        fireEvent.click(option(/^delete_msg/));
        expect(open).toHaveBeenCalledWith('delete_msg', { newTab: false });
        expect(onChange).toHaveBeenCalledWith(false);
    });

    it('目录外的接口名也能直接打开', async () => {
        const user = userEvent.setup();
        renderPalette();
        const open = vi.spyOn(debugWorkspaceStore, 'openAction');
        await screen.findByRole('option', { name: /^get_login_info/ });
        await user.type(input(), 'delete_msg_async');
        expect(selected()).toBe(option('打开目录外的接口 delete_msg_async'));
        await user.keyboard('{Enter}');
        expect(open).toHaveBeenCalledWith('delete_msg_async', { newTab: false });
    });

    it('什么都没搜到时给一句怎么搜；Esc 关掉；再打开时上次的字还在、整段选中', async () => {
        const user = userEvent.setup();
        const onChange = vi.fn();
        renderPalette({ onChange });
        await screen.findByRole('option', { name: /^get_login_info/ });
        await user.type(input(), '没有这个');
        expect(await screen.findByText('没有找到「没有这个」')).toBeInTheDocument();
        await user.keyboard('{Escape}');
        expect(onChange).toHaveBeenCalledWith(false);
        await waitFor(() =>
            expect(screen.queryByRole('combobox', { name: '搜索接口' })).not.toBeInTheDocument(),
        );

        fireEvent.click(screen.getByRole('button', { name: '打开面板' }));
        await waitFor(() => expect(input()).toHaveFocus());
        const el = input() as HTMLInputElement;
        expect(el.value).toBe('没有这个');
        expect([el.selectionStart, el.selectionEnd]).toEqual([0, el.value.length]);
    });

    it('目录读失败给重试；没选 Bot 时说明要先选', async () => {
        service.catalog.mockRejectedValueOnce(new Error('连不上 WebUI'));
        const user = userEvent.setup();
        renderPalette();
        expect(await screen.findByText('读不到接口目录')).toBeInTheDocument();
        await user.click(screen.getByRole('button', { name: '重试' }));
        expect(await screen.findByRole('option', { name: /^get_login_info/ })).toBeInTheDocument();
        cleanup();

        renderPalette({ target: null });
        expect(await screen.findByText('先在顶栏选一个 Bot')).toBeInTheDocument();
    });

    it('没选 Bot 时回车不打开任何东西：行算出来了，但列表没显示', async () => {
        const user = userEvent.setup();
        const onChange = vi.fn();
        renderPalette({ target: null, onChange });
        expect(await screen.findByText('先在顶栏选一个 Bot')).toBeInTheDocument();
        // 等收藏读到（收藏不依赖 Bot）：这正是 bug 的条件——收藏在 rows 里，但看不见
        await act(async () => {
            await new Promise((r) => setTimeout(r, 0));
        });
        const open = vi.spyOn(debugWorkspaceStore, 'openAction');
        await user.keyboard('{Enter}');
        await user.keyboard('{ArrowDown}{Enter}');
        expect(open).not.toHaveBeenCalled();
        expect(debugWorkspaceStore.getSnapshot().ws.tabs).toHaveLength(0);
        // 面板也没被关掉：列表没显示时，按键只拦默认行为
        expect(onChange).not.toHaveBeenCalledWith(false);
    });

    it('目录还在读（骨架）时回车不打开任何东西', async () => {
        service.catalog.mockReturnValueOnce(new Promise(() => {}));
        const user = userEvent.setup();
        renderPalette();
        await waitFor(() => expect(input()).toHaveFocus());
        // 等收藏读到：同 bug 条件，收藏对键盘可见、对用户不可见
        await act(async () => {
            await new Promise((r) => setTimeout(r, 0));
        });
        const open = vi.spyOn(debugWorkspaceStore, 'openAction');
        await user.keyboard('{Enter}');
        expect(open).not.toHaveBeenCalled();
        expect(debugWorkspaceStore.getSnapshot().ws.tabs).toHaveLength(0);
    });

    it('目录读到、列表真的显示后，回车照常打开第一条', async () => {
        const user = userEvent.setup();
        renderPalette();
        const open = vi.spyOn(debugWorkspaceStore, 'openAction');
        await screen.findByRole('option', { name: /^get_login_info/ });
        await user.keyboard('{Enter}');
        expect(open).toHaveBeenCalledWith('get_login_info', { newTab: false });
    });

    it('另开标签后关掉面板，焦点落在新标签的接口名输入框上', async () => {
        const user = userEvent.setup();
        // 中栏一次只挂当前标签；这个假输入框代表换过去的那个标签的请求头
        renderPalette({
            outside: (
                <input role="combobox" aria-label="接口名" data-testid="action-input" readOnly />
            ),
        });
        const actionInput = screen.getByTestId('action-input');
        const open = vi.spyOn(debugWorkspaceStore, 'openAction');
        await screen.findByRole('option', { name: /^get_login_info/ });
        await user.keyboard('{Control>}{Enter}{/Control}');
        expect(open).toHaveBeenCalledWith('get_login_info', { newTab: true });
        // Radix 的「还给打开面板前的元素」被拦下，焦点交给新标签的接口名
        await waitFor(() => expect(actionInput).toHaveFocus());
    });

    it('原地顶替当前标签时焦点照旧还给打开面板前的元素', async () => {
        const user = userEvent.setup();
        // 已有一个空白没动过的标签：回车会原地顶替它，不换标签，也不接管焦点
        debugWorkspaceStore.newTab();
        renderPalette({
            outside: (
                <input role="combobox" aria-label="接口名" data-testid="action-input" readOnly />
            ),
        });
        const actionInput = screen.getByTestId('action-input');
        const open = vi.spyOn(debugWorkspaceStore, 'openAction');
        await screen.findByRole('option', { name: /^get_login_info/ });
        await user.keyboard('{Enter}');
        expect(open).toHaveBeenCalledWith('get_login_info', { newTab: false });
        const ws = debugWorkspaceStore.getSnapshot().ws;
        expect(ws.tabs).toHaveLength(1);
        // 等关闭和可能的焦点接管都发生完再断言
        await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
        await act(async () => {
            await new Promise((r) => setTimeout(r, 50));
        });
        expect(actionInput).not.toHaveFocus();
    });
});
