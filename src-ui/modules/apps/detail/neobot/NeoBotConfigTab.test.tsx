import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { beforeEach, expect, it, vi } from 'vitest';
import { NeoBotDraftContext } from '../../../../hooks/apps/neoBotDraftContext';
import { NeoBotConfigEditor } from './NeoBotConfigTab';

const { call, push } = vi.hoisted(() => ({ call: vi.fn(), push: vi.fn() }));
vi.mock('../../../../core/services/app-framework.service', () => ({
    appFrameworkService: { panelCall: call },
}));
vi.mock('../../../../hooks/ui/globalInfoBarStore', () => ({ pushInfoBar: push }));

const doc = {
    revision: 'content-hash-a',
    config: { retries: 3, options: { mode: 'auto' } },
    source: 'retries = 3',
    schema: [
        {
            name: 'retries',
            label: '重试次数',
            kind: 'scalar',
            type: 'int',
            value: 3,
            default: 2,
            min: 1,
            max: 10,
        },
        { name: 'options', label: '高级参数', kind: 'dict', value: { mode: 'auto' }, default: {} },
    ],
};

function mount() {
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    const drafts = { instanceId: 'one', values: new Map<string, unknown>() };
    const tree = (shown: boolean) => (
        <QueryClientProvider client={client}>
            <NeoBotDraftContext.Provider value={drafts}>
                {shown && (
                    <NeoBotConfigEditor
                        instanceId="one"
                        path="/api/config"
                        title="本体配置"
                        doc={doc}
                    />
                )}
            </NeoBotDraftContext.Provider>
        </QueryClientProvider>
    );
    return { ...render(tree(true)), tree };
}
beforeEach(() => {
    call.mockReset();
    push.mockReset();
});

it('冲突后保留草稿和原 revision，切换页面返回也可继续编辑', async () => {
    call.mockResolvedValue({
        kind: 'failed',
        status: 409,
        message: '配置已变化',
        data: { ok: false },
    });
    const view = mount();
    fireEvent.change(screen.getByLabelText('重试次数'), { target: { value: '6' } });
    fireEvent.click(screen.getByRole('button', { name: '保存并重载' }));
    await waitFor(() =>
        expect(call).toHaveBeenCalledWith(
            'one',
            'POST',
            '/api/config',
            expect.objectContaining({
                revision: 'content-hash-a',
                config: { retries: 6, options: { mode: 'auto' } },
                reload: true,
            }),
        ),
    );
    await waitFor(() => expect(screen.getByRole('button', { name: '保存并重载' })).toBeEnabled());
    expect(screen.getByLabelText('重试次数')).toHaveValue(6);
    view.rerender(view.tree(false));
    view.rerender(view.tree(true));
    expect(screen.getByLabelText('重试次数')).toHaveValue(6);
});

it('空数字、越界和无效 JSON 会阻止保存，撤销清理字段错误', () => {
    mount();
    const number = screen.getByLabelText('重试次数');
    fireEvent.change(number, { target: { value: '' } });
    expect(number).toHaveValue(null);
    expect(screen.getByRole('button', { name: '保存' })).toBeDisabled();
    fireEvent.change(number, { target: { value: '11' } });
    expect(screen.getByRole('button', { name: '保存' })).toBeDisabled();
    fireEvent.change(number, { target: { value: '4' } });
    fireEvent.change(screen.getByLabelText('高级参数'), { target: { value: '{broken' } });
    expect(screen.getByRole('button', { name: '保存' })).toBeDisabled();
    fireEvent.click(screen.getByRole('button', { name: '撤销草稿' }));
    expect(screen.getByLabelText('高级参数')).toHaveValue(
        JSON.stringify(doc.config.options, null, 2),
    );
    expect(screen.queryByText('JSON 格式不正确')).not.toBeInTheDocument();
    expect(call).not.toHaveBeenCalled();
});
