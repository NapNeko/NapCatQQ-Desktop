import type { ReactNode } from 'react';
import { afterAll, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import type { DebugActionSummary } from '../../../core/ipc/generated/debug/DebugActionSummary';
import type { DebugCatalog } from '../../../core/ipc/generated/debug/DebugCatalog';
import type { DebugTarget } from '../../../core/ipc/generated/debug/DebugTarget';

const service: Record<string, ReturnType<typeof vi.fn>> = {
    catalog: vi.fn(),
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
import { LeftColumn } from './LeftColumn';

// jsdom 里元素没有尺寸，虚拟列表会当视口高 0 一行不画：滚动容器给 600px，其余 30px。
// 按滚动容器的样式认而不是按 role：容器在加载完之前就挂着了（那时还没有 role），虚拟列表挂上时量的就是它
const offsetHeight = Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'offsetHeight');
const offsetWidth = Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'offsetWidth');
beforeAll(() => {
    Object.defineProperty(HTMLElement.prototype, 'offsetHeight', {
        configurable: true,
        get() {
            return (this as HTMLElement).classList.contains('overflow-y-auto') ? 600 : 30;
        },
    });
    Object.defineProperty(HTMLElement.prototype, 'offsetWidth', { configurable: true, get: () => 280 });
});
afterAll(() => {
    if (offsetHeight) Object.defineProperty(HTMLElement.prototype, 'offsetHeight', offsetHeight);
    if (offsetWidth) Object.defineProperty(HTMLElement.prototype, 'offsetWidth', offsetWidth);
});

const NC: DebugTarget = {
    bot_id: 'bot-nc',
    name: 'NapCat 测试号',
    qq_id: 1919810,
    backend: 'napcat',
    host: { kind: 'local' },
    running: true,
    online: true,
};

function action(name: string, patch: Partial<DebugActionSummary> = {}): DebugActionSummary {
    return {
        name,
        aliases: [],
        summary: `${name} 的简介`,
        category: 'group_info',
        safety: 'read_only',
        stream: false,
        supported: true,
        other_backend_present: true,
        param_diff: false,
        ...patch,
    };
}

function catalog(patch: Partial<DebugCatalog> = {}): DebugCatalog {
    return {
        backend: 'napcat',
        source: 'live',
        snapshot_version: '4.15.18',
        actions: [
            action('get_group_list', { summary: '获取群列表' }),
            action('get_group_info', { summary: '获取群信息' }),
            action('send_group_msg', { category: 'message', safety: 'side_effect', summary: '发送群消息' }),
            action('set_group_kick', { category: 'group_admin', safety: 'dangerous', summary: '踢出群成员' }),
            action('nc_only_thing', { category: 'extension', other_backend_present: false, param_diff: true }),
            action('upload_file_stream', { category: 'stream', stream: true }),
            action('mystery_action', { supported: false, category: 'extension' }),
        ],
        ...patch,
    };
}

function renderLeft(target: DebugTarget | null = NC) {
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    const wrapper = ({ children }: { children: ReactNode }) => (
        <QueryClientProvider client={client}>
            <TooltipProvider>{children}</TooltipProvider>
        </QueryClientProvider>
    );
    return render(<LeftColumn target={target} panel="catalog" />, { wrapper });
}

const rowNames = () =>
    screen
        .getAllByRole('treeitem')
        .filter((el) => el.getAttribute('aria-level') !== null && el.getAttribute('aria-expanded') === null)
        .map((el) => el.querySelector('.font-mono')?.textContent);

beforeEach(() => {
    service.catalog.mockReset();
    service.catalog.mockResolvedValue(catalog());
    preferencesStore.setMotionEnabled(false);
    debugWorkspaceStore._reset();
    _resetDebugCatalogForTests();
});

describe('接口目录', () => {
    it('按分类分组，不支持的收进底部默认折叠的一段；点开能看到', async () => {
        renderLeft();
        await screen.findByText('get_group_list');
        const tree = screen.getByRole('tree', { name: '接口目录' });
        expect(within(tree).getByText('消息')).toBeInTheDocument();
        expect(within(tree).getByText('群信息')).toBeInTheDocument();
        // 分类顺序：消息在群信息前面
        expect(rowNames().slice(0, 3)).toEqual(['send_group_msg', 'get_group_info', 'get_group_list']);

        const unsupported = screen.getByText('当前 Bot 不支持（1）').closest('[role="treeitem"]') as HTMLElement;
        expect(unsupported).toHaveAttribute('aria-expanded', 'false');
        expect(screen.queryByText('mystery_action')).not.toBeInTheDocument();
        fireEvent.click(unsupported);
        expect(await screen.findByText('mystery_action')).toBeInTheDocument();
        // 收回去，别影响后面的用例（折叠状态记在模块里）
        fireEvent.click(unsupported);
    });

    it('徽章：仅 NC、参数不同、流式', async () => {
        renderLeft();
        const row = (await screen.findByText('nc_only_thing')).closest('[role="treeitem"]') as HTMLElement;
        expect(within(row).getByText('仅 NC')).toBeInTheDocument();
        expect(within(row).getByText('参数不同')).toBeInTheDocument();
        const stream = screen.getByText('upload_file_stream').closest('[role="treeitem"]') as HTMLElement;
        expect(within(stream).getByText('流式')).toBeInTheDocument();
    });

    it('折叠分类后它的接口不再显示', async () => {
        renderLeft();
        await screen.findByText('get_group_list');
        const header = screen.getByText('群信息').closest('[role="treeitem"]') as HTMLElement;
        fireEvent.click(header);
        expect(header).toHaveAttribute('aria-expanded', 'false');
        expect(screen.queryByText('get_group_list')).not.toBeInTheDocument();
        fireEvent.click(header);
        expect(screen.getByText('get_group_list')).toBeInTheDocument();
    });

    it('搜索过滤结果，回车打开第一个；Ctrl+回车另开标签', async () => {
        const open = vi.spyOn(debugWorkspaceStore, 'openAction');
        const recent = vi.spyOn(debugWorkspaceStore, 'pushRecent');
        const user = userEvent.setup();
        renderLeft();
        await screen.findByText('get_group_list');

        const search = screen.getByRole('combobox', { name: '搜索接口' });
        await user.type(search, 'group_info');
        expect(rowNames()).toEqual(['get_group_info']);
        // 结果数显示在搜索框里
        expect(screen.getByText('1')).toBeInTheDocument();

        await user.keyboard('{Enter}');
        expect(open).toHaveBeenCalledWith('get_group_info', { newTab: false });
        expect(recent).toHaveBeenCalledWith('get_group_info');

        await user.keyboard('{Control>}{Enter}{/Control}');
        expect(open).toHaveBeenLastCalledWith('get_group_info', { newTab: true });

        await user.clear(search);
        expect(rowNames().length).toBeGreaterThan(3);
    });

    it('搜索框里 ↓ 移动高亮，回车打开高亮的那个；没有结果时给提示', async () => {
        const open = vi.spyOn(debugWorkspaceStore, 'openAction');
        const user = userEvent.setup();
        renderLeft();
        await screen.findByText('get_group_list');
        const search = screen.getByRole('combobox', { name: '搜索接口' });

        await user.type(search, 'get_group');
        expect(rowNames()).toEqual(['get_group_info', 'get_group_list']);
        await user.keyboard('{ArrowDown}{Enter}');
        expect(open).toHaveBeenCalledWith('get_group_list', { newTab: false });

        await user.clear(search);
        await user.type(search, 'zzz');
        expect(screen.getByText('没有匹配「zzz」的接口')).toBeInTheDocument();
        await user.keyboard('{Escape}');
        expect(search).toHaveValue('');
    });

    it('单击打开；Ctrl+单击、中键另开标签', async () => {
        const open = vi.spyOn(debugWorkspaceStore, 'openAction');
        renderLeft();
        const row = (await screen.findByText('send_group_msg')).closest('[role="treeitem"]') as HTMLElement;

        fireEvent.click(row);
        expect(open).toHaveBeenLastCalledWith('send_group_msg', { newTab: false });
        fireEvent.click(row, { ctrlKey: true });
        expect(open).toHaveBeenLastCalledWith('send_group_msg', { newTab: true });
        fireEvent(row, new MouseEvent('auxclick', { bubbles: true, button: 1 }));
        expect(open).toHaveBeenLastCalledWith('send_group_msg', { newTab: true });
        expect(open).toHaveBeenCalledTimes(3);
    });

    it('列表里用方向键移动、回车打开，← 回到所在分类', async () => {
        const open = vi.spyOn(debugWorkspaceStore, 'openAction');
        const user = userEvent.setup();
        renderLeft();
        await screen.findByText('get_group_list');
        const tree = screen.getByRole('tree', { name: '接口目录' });
        act(() => tree.focus());

        // 第一行是「消息」分类标题，第二行是 send_group_msg
        await user.keyboard('{ArrowDown}{ArrowDown}{Enter}');
        expect(open).toHaveBeenCalledWith('send_group_msg', { newTab: false });

        await user.keyboard('{ArrowLeft}');
        const active = document.getElementById(tree.getAttribute('aria-activedescendant') ?? '');
        expect(active).toHaveTextContent('消息');
    });

    it('按 / 聚焦搜索框（在输入框里不抢）', async () => {
        renderLeft();
        await screen.findByText('get_group_list');
        const search = screen.getByRole('combobox', { name: '搜索接口' });
        fireEvent.keyDown(document.body, { key: '/' });
        expect(search).toHaveFocus();
    });

    it('选了 Bot 且目录来自内置快照时显示提示条，可以点「重新获取」马上再问一次', async () => {
        service.catalog.mockResolvedValue(catalog({ source: 'snapshot' }));
        renderLeft({ ...NC, running: false });
        expect(await screen.findByText('按内置目录显示（4.15.18），可能和你的版本不同')).toBeInTheDocument();
        expect(service.catalog).toHaveBeenCalledTimes(1);

        // 「重新获取」：内部通道可能刚好就绪（比如登录完成但事件没落），不等定时重拉
        service.catalog.mockResolvedValue(catalog({ source: 'live' }));
        fireEvent.click(screen.getByRole('button', { name: '重新获取' }));
        await waitFor(() => expect(screen.queryByText(/按内置目录显示/)).not.toBeInTheDocument());
        expect(service.catalog).toHaveBeenCalledTimes(2);
    });

    it('在线目录不显示快照提示；当前标签打开的接口标出来', async () => {
        debugWorkspaceStore.openAction('get_group_info');
        renderLeft();
        const row = (await screen.findByText('get_group_info')).closest('[role="treeitem"]') as HTMLElement;
        expect(row).toHaveAttribute('aria-current', 'true');
        expect(screen.queryByText(/按内置目录显示/)).not.toBeInTheDocument();
    });

    it('读目录失败时给重试', async () => {
        service.catalog.mockRejectedValueOnce(new Error('连不上 WebUI'));
        renderLeft();
        expect(await screen.findByText('读不到接口目录')).toBeInTheDocument();
        fireEvent.click(screen.getByRole('button', { name: /重试/ }));
        await waitFor(() => expect(screen.getByText('get_group_list')).toBeInTheDocument());
    });

    it('行上右键菜单里的按键不会冒到列表上打开高亮的那一行', async () => {
        const open = vi.spyOn(debugWorkspaceStore, 'openAction');
        const user = userEvent.setup();
        renderLeft();
        await screen.findByText('get_group_list');
        const tree = screen.getByRole('tree', { name: '接口目录' });
        act(() => tree.focus());
        // 高亮落在 send_group_msg 上
        await user.keyboard('{ArrowDown}{ArrowDown}');
        fireEvent.contextMenu(screen.getByText('get_group_list').closest('[role="treeitem"]') as HTMLElement);
        const item = await screen.findByRole('menuitem', { name: '复制接口名' });
        fireEvent.keyDown(item, { key: 'Enter' });
        await new Promise((r) => setTimeout(r, 20));
        expect(open).not.toHaveBeenCalled();
    });
});
