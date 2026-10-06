import React from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, renderHook, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';

const saveResponseFile = vi.fn();
const pushErrorBar = vi.fn();
const pushInfoBar = vi.fn();

vi.mock('../../core/services/onebot-debug.service', () => ({
    onebotDebugService: { saveResponseFile: (...args: unknown[]) => saveResponseFile(...args) },
}));
vi.mock('../ui/pushErrorBar', () => ({
    pushErrorBar: (...args: unknown[]) => pushErrorBar(...args),
}));
vi.mock('../ui/globalInfoBarStore', () => ({
    pushInfoBar: (...args: unknown[]) => pushInfoBar(...args),
}));

import { useSaveResponse } from './useSaveResponse';

function renderSave() {
    const client = new QueryClient({ defaultOptions: { mutations: { retry: false } } });
    const wrapper = ({ children }: { children: React.ReactNode }) => (
        <QueryClientProvider client={client}>{children}</QueryClientProvider>
    );
    return renderHook(() => useSaveResponse(), { wrapper });
}

beforeEach(() => {
    saveResponseFile.mockReset();
    pushErrorBar.mockReset();
    pushInfoBar.mockReset();
});
afterEach(cleanup);

describe('useSaveResponse', () => {
    it('按请求 id 另存，默认文件名带接口名；存好了给提示', async () => {
        saveResponseFile.mockResolvedValue(true);
        const { result } = renderSave();
        act(() => result.current.mutate({ requestId: 'req-1', action: 'get_group_member_list' }));
        await waitFor(() =>
            expect(pushInfoBar).toHaveBeenCalledWith(
                expect.objectContaining({ title: '完整回包已保存' }),
            ),
        );
        expect(saveResponseFile).toHaveBeenCalledWith(
            'req-1',
            expect.stringMatching(/^get_group_member_list-\d{8}-\d{6}\.json$/),
        );
        expect(pushErrorBar).not.toHaveBeenCalled();
    });

    it('用户在另存为对话框里取消：什么都不提示', async () => {
        saveResponseFile.mockResolvedValue(false);
        const { result } = renderSave();
        act(() => result.current.mutate({ requestId: 'req-1', action: 'x' }));
        await waitFor(() => expect(result.current.isSuccess).toBe(true));
        expect(pushInfoBar).not.toHaveBeenCalled();
        expect(pushErrorBar).not.toHaveBeenCalled();
    });

    it('后端已经不留这次的全文：弹错误条带上后端原话', async () => {
        saveResponseFile.mockRejectedValue('没有这次调用的完整回包：只保留最近 3 次被截断的回包');
        const { result } = renderSave();
        act(() => result.current.mutate({ requestId: 'old', action: 'x' }));
        await waitFor(() => expect(pushErrorBar).toHaveBeenCalledTimes(1));
        expect(pushErrorBar).toHaveBeenCalledWith(
            expect.objectContaining({
                key: 'debug-save-response',
                raw: '没有这次调用的完整回包：只保留最近 3 次被截断的回包',
            }),
        );
    });
});
