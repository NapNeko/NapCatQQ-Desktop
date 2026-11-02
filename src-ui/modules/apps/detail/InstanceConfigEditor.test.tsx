// @vitest-environment jsdom
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { AppConfigDocument, AppInstance } from '../../../core/ipc/types';
import { appFrameworkService } from '../../../core/services/app-framework.service';
import { pushInfoBar } from '../../../hooks/ui/globalInfoBarStore';
import { InstanceConfigEditor } from './InstanceConfigEditor';
import { AppInstancePageNext } from './AppInstancePage.next';

vi.mock('../../../core/services/app-framework.service', () => ({
    appFrameworkService: {
        listConfigDocuments: vi.fn(),
        readConfigText: vi.fn(),
        writeConfigText: vi.fn(),
    },
}));
vi.mock('../../../hooks/ui/globalInfoBarStore', () => ({ pushInfoBar: vi.fn() }));
vi.mock('../../../hooks/apps/useAppInstances', () => ({
    useAppInstances: () => ({ instances: [instance], isLoading: false }),
    useAppFrameworks: () => ({
        data: [{ id: instance.framework_id, has_webui: instance.framework_id === 'karin' }],
    }),
}));
vi.mock('../../../hooks/remote/useServerManager', () => ({
    useServerManager: () => ({ servers: [] }),
}));
vi.mock('../../../hooks/apps/useAppFrameworkVersions', () => ({
    useAppFrameworkVersions: () => ({ data: null }),
}));
vi.mock('../AppLinkDialog', () => ({ AppLinkDialog: () => null }));
vi.mock('../DeleteInstanceDialog', () => ({ DeleteInstanceDialog: () => null }));
vi.mock('./ReinstallDialog', () => ({ ReinstallDialog: () => null }));
vi.mock('./InstanceLogTab', () => ({ InstanceLogTab: () => null }));
vi.mock('./InstanceVersionTab', () => ({ InstanceVersionTab: () => null }));
vi.mock('./DetailHeader', () => ({ DetailHeader: () => null }));
vi.mock('../../../shared/ui', async (importOriginal) => {
    const actual = await importOriginal<typeof import('../../../shared/ui')>();
    return {
        ...actual,
        SyntaxTextEditor: ({
            value,
            onChange,
            disabled,
            'aria-label': label,
        }: {
            value: string;
            onChange: (value: string) => void;
            disabled: boolean;
            'aria-label': string;
        }) => (
            <textarea
                aria-label={label}
                value={value}
                onChange={(event) => onChange(event.target.value)}
                disabled={disabled}
            />
        ),
        Select: ({
            items,
            value,
            onValueChange,
            disabled,
        }: {
            items: { value: string; label: string }[];
            value: string;
            onValueChange: (value: string) => void;
            disabled: boolean;
        }) => (
            <select
                aria-label="配置文件"
                value={value}
                onChange={(event) => onValueChange(event.target.value)}
                disabled={disabled}
            >
                {items.map((item) => (
                    <option key={item.value} value={item.value}>
                        {item.label}
                    </option>
                ))}
            </select>
        ),
    };
});

let instance: AppInstance;
let client: QueryClient;
const documents: AppConfigDocument[] = [
    { id: 'env', label: '.env', rel_path: '.env', format: 'dot_env', hot_reload: false },
    {
        id: 'config',
        label: 'config.toml',
        rel_path: 'config.toml',
        format: 'toml',
        hot_reload: false,
    },
];

beforeEach(() => {
    vi.clearAllMocks();
    client = new QueryClient({
        defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
    });
    instance = {
        id: 'one',
        framework_id: 'nonebot2',
        display_name: 'bot',
        host_id: 'remote:test',
        placement: 'remote_native',
        install_dir: '/app',
        port: 13120,
        state: 'stopped',
        origin: 'imported',
        auto_start: false,
        install_renderer: false,
        created_at_ms: 1,
    };
    vi.mocked(appFrameworkService.listConfigDocuments).mockResolvedValue(documents);
    vi.mocked(appFrameworkService.readConfigText).mockImplementation(async (_id, docId) => ({
        doc_id: docId,
        text: docId === 'env' ? 'PORT=13120\n' : 'name = "bot"\n',
        revision: `base-${docId}`,
    }));
    vi.mocked(appFrameworkService.writeConfigText).mockImplementation(async (_id, docId, text) => ({
        doc_id: docId,
        text,
        revision: 'saved',
    }));
});
afterEach(() => {
    cleanup();
    client.clear();
});

function renderEditor() {
    return render(
        <QueryClientProvider client={client}>
            <InstanceConfigEditor instance={instance} />
        </QueryClientProvider>,
    );
}
const editor = () => screen.getByRole('textbox', { hidden: true }) as HTMLTextAreaElement;

describe('框架配置文件编辑', () => {
    it.each([
        { running: false, hot: true, note: undefined, expected: '下次启动实例时生效。' },
        { running: true, hot: false, note: undefined, expected: '重启实例后生效。' },
        {
            running: true,
            hot: true,
            note: undefined,
            expected: '支持自动热重载，生效情况请查看实例日志。',
        },
        { running: true, hot: true, note: '监听端口需重启。', expected: '监听端口需重启。' },
    ])('保存后 InfoBar 准确提示生效方式：$expected', async ({ running, hot, note, expected }) => {
        instance.state = running ? 'running' : 'stopped';
        vi.mocked(appFrameworkService.listConfigDocuments).mockResolvedValue([
            { ...documents[0], hot_reload: hot, reload_note: note },
        ]);
        renderEditor();
        await screen.findByRole('textbox');
        fireEvent.change(editor(), { target: { value: 'PORT=22222' } });
        await screen.findByText('未保存');
        expect(pushInfoBar).not.toHaveBeenCalled();
        fireEvent.click(screen.getByRole('button', { name: '保存' }));
        await waitFor(() =>
            expect(pushInfoBar).toHaveBeenCalledWith(
                expect.objectContaining({
                    title: '.env 已保存',
                    content: expected,
                    tone: 'success',
                }),
            ),
        );
    });
    it.each(['nonebot2', 'karin'])('%s 不论是否提供 WebUI 都默认进入编辑器', async (framework) => {
        instance.framework_id = framework;
        instance.state = 'running';
        render(
            <QueryClientProvider client={client}>
                <AppInstancePageNext instanceId="one" onBack={vi.fn()} />
            </QueryClientProvider>,
        );
        await screen.findByRole('textbox');
        expect(editor().value).toBe('PORT=13120\n');
        expect(screen.getByRole('tab', { name: '配置' }).getAttribute('data-state')).toBe('active');
    });

    it('切换文件和离开详情保留草稿，保存使用读入时的版本', async () => {
        const view = renderEditor();
        await screen.findByRole('textbox');
        fireEvent.change(editor(), { target: { value: 'PORT=23456\n' } });
        await screen.findByText('未保存');
        fireEvent.change(screen.getByRole('combobox'), { target: { value: 'config' } });
        await waitFor(() => expect(editor().value).toBe('name = "bot"\n'));
        fireEvent.change(screen.getByRole('combobox'), { target: { value: 'env' } });
        await waitFor(() => expect(editor().value).toBe('PORT=23456\n'));
        view.unmount();
        renderEditor();
        await waitFor(() => expect(editor().value).toBe('PORT=23456\n'));
        fireEvent.click(screen.getByRole('button', { name: '保存' }));
        await waitFor(() =>
            expect(appFrameworkService.writeConfigText).toHaveBeenCalledWith(
                'one',
                'env',
                'PORT=23456\n',
                'base-env',
            ),
        );
        await waitFor(() => expect(screen.queryByText('未保存')).toBeNull());
    });

    it('外部冲突保留草稿，只有明确覆盖才省略版本检查', async () => {
        vi.mocked(appFrameworkService.writeConfigText).mockRejectedValueOnce({
            kind: 'conflict',
            message: '外部修改',
            issues: [],
        });
        renderEditor();
        await screen.findByRole('textbox');
        fireEvent.change(editor(), { target: { value: 'PORT=22222' } });
        await screen.findByText('未保存');
        fireEvent.click(screen.getByRole('button', { name: '保存' }));
        const dialog = await screen.findByRole('dialog');
        expect(pushInfoBar).not.toHaveBeenCalled();
        expect(editor().value).toBe('PORT=22222');
        fireEvent.click(within(dialog).getByRole('button', { name: '覆盖保存' }));
        await waitFor(() =>
            expect(appFrameworkService.writeConfigText).toHaveBeenLastCalledWith(
                'one',
                'env',
                'PORT=22222',
                null,
            ),
        );
        await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
    });

    it('重新加载失败仍保留草稿，重试成功才替换', async () => {
        renderEditor();
        await screen.findByRole('textbox');
        fireEvent.change(editor(), { target: { value: 'PORT=22222' } });
        await screen.findByText('未保存');
        vi.mocked(appFrameworkService.readConfigText).mockRejectedValueOnce(new Error('SSH 断开'));
        fireEvent.click(screen.getByRole('button', { name: '重新加载' }));
        fireEvent.click(
            within(await screen.findByRole('dialog')).getByRole('button', { name: '重新加载' }),
        );
        await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
        expect(editor().value).toBe('PORT=22222');
        expect(screen.getByRole('alert').textContent).toContain('SSH 断开');
        fireEvent.click(screen.getByRole('button', { name: '重新加载' }));
        fireEvent.click(
            within(await screen.findByRole('dialog')).getByRole('button', { name: '重新加载' }),
        );
        await waitFor(() => expect(editor().value).toBe('PORT=13120\n'));
        expect(screen.queryByText('未保存')).toBeNull();
    });
});
