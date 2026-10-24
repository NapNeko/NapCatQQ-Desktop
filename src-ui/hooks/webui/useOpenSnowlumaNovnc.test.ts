// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, renderHook } from '@testing-library/react';
import { useOpenSnowlumaNovnc } from './useOpenSnowlumaNovnc';

const mocks = vi.hoisted(() => ({
    openNovnc: vi.fn(),
    openExternalUrl: vi.fn(),
    writeText: vi.fn(),
    pushInfoBar: vi.fn(),
    browserWriteText: vi.fn(),
}));
vi.mock('../../core/services/desktop.service', () => ({
    snowlumaService: { openNovnc: mocks.openNovnc },
}));
vi.mock('../../core/services/clipboard.service', () => ({
    clipboardService: { writeText: mocks.writeText },
}));
vi.mock('../../core/ipc/transport', () => ({ openExternalUrl: mocks.openExternalUrl }));
vi.mock('../ui/globalInfoBarStore', () => ({ pushInfoBar: mocks.pushInfoBar }));

const ENDPOINT = {
    url: 'http://127.0.0.1:47081/vnc.html?password=fixture-vnc-password',
    password: 'fixture-vnc-password',
};

beforeEach(() => {
    vi.resetAllMocks();
    vi.stubGlobal('navigator', { clipboard: { writeText: mocks.browserWriteText } });
    mocks.openNovnc.mockResolvedValue(ENDPOINT);
    mocks.writeText.mockResolvedValue(undefined);
    mocks.openExternalUrl.mockResolvedValue(undefined);
});

afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
});

describe('useOpenSnowlumaNovnc', () => {
    it('使用原生剪贴板服务后打开 QQ 界面，不触发 Webview 剪贴板权限', async () => {
        const { result } = renderHook(useOpenSnowlumaNovnc);

        await result.current('one');

        expect(mocks.openNovnc).toHaveBeenCalledWith('one');
        expect(mocks.writeText).toHaveBeenCalledWith(ENDPOINT.password);
        expect(mocks.browserWriteText).not.toHaveBeenCalled();
        expect(mocks.openExternalUrl).toHaveBeenCalledWith(ENDPOINT.url);
        expect(mocks.pushInfoBar).toHaveBeenCalledWith(
            expect.objectContaining({ tone: 'success', title: 'VNC 密码已复制' }),
        );
        expect(JSON.stringify(mocks.pushInfoBar.mock.calls)).not.toContain(ENDPOINT.password);
    });

    it('复制失败仍打开 QQ 界面，提示和日志都不包含密码或原始异常', async () => {
        mocks.writeText.mockRejectedValue(new Error(`clipboard denied: ${ENDPOINT.password}`));
        const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
        const error = vi.spyOn(console, 'error').mockImplementation(() => {});
        const { result } = renderHook(useOpenSnowlumaNovnc);

        await result.current('one');

        expect(mocks.openExternalUrl).toHaveBeenCalledWith(ENDPOINT.url);
        expect(mocks.pushInfoBar).toHaveBeenCalledWith(
            expect.objectContaining({
                tone: 'warning',
                title: '未能复制 VNC 密码',
                content: expect.stringContaining('QQ 界面已打开'),
            }),
        );
        expect(JSON.stringify(mocks.pushInfoBar.mock.calls)).not.toContain(ENDPOINT.password);
        expect(warn).not.toHaveBeenCalled();
        expect(error).not.toHaveBeenCalled();
    });

    it('浏览器打开失败时不宣称已打开，并保留上层错误处理', async () => {
        const failure = new Error('系统浏览器打开失败');
        mocks.openExternalUrl.mockRejectedValue(failure);
        const { result } = renderHook(useOpenSnowlumaNovnc);

        await expect(result.current('one')).rejects.toBe(failure);

        expect(mocks.pushInfoBar).toHaveBeenCalledWith(
            expect.objectContaining({
                tone: 'warning',
                title: 'VNC 密码已复制',
                content: expect.stringContaining('浏览器未能打开'),
            }),
        );
        expect(JSON.stringify(mocks.pushInfoBar.mock.calls)).not.toContain(ENDPOINT.password);
        expect(JSON.stringify(mocks.pushInfoBar.mock.calls)).not.toContain('QQ 界面已打开');
    });
});
