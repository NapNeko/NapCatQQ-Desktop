// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, renderHook } from '@testing-library/react';
import { useOpenWebui } from './useOpenWebui';

const mocks = vi.hoisted(() => ({
    openWebui: vi.fn(),
    openExternalUrl: vi.fn(),
    writeText: vi.fn(),
    pushInfoBar: vi.fn(),
}));
vi.mock('../../core/services/desktop.service', () => ({
    snowlumaService: { openWebui: mocks.openWebui },
}));
vi.mock('../../core/services/clipboard.service', () => ({
    clipboardService: { writeText: mocks.writeText },
}));
vi.mock('../../core/ipc/transport', () => ({ openExternalUrl: mocks.openExternalUrl }));
vi.mock('../ui/globalInfoBarStore', () => ({ pushInfoBar: mocks.pushInfoBar }));

const ARGS = { botId: 'one', flavor: 'snowluma' as const };
const ENDPOINT = { url: 'http://127.0.0.1:47099/', password: 'fixture-password' };

beforeEach(() => {
    vi.resetAllMocks();
    mocks.openWebui.mockResolvedValue(ENDPOINT);
    mocks.writeText.mockResolvedValue(undefined);
    mocks.openExternalUrl.mockResolvedValue(undefined);
});

afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
});

describe('useOpenWebui', () => {
    it('复制 SnowLuma 密码并在浏览器打开成功后确认', async () => {
        const { result } = renderHook(useOpenWebui);

        await result.current(ARGS);

        expect(mocks.openWebui).toHaveBeenCalledWith('one');
        expect(mocks.writeText).toHaveBeenCalledWith(ENDPOINT.password);
        expect(mocks.openExternalUrl).toHaveBeenCalledWith(ENDPOINT.url);
        expect(mocks.pushInfoBar).toHaveBeenCalledWith(
            expect.objectContaining({ tone: 'success', title: '密码已复制' }),
        );
        expect(JSON.stringify(mocks.pushInfoBar.mock.calls)).not.toContain(ENDPOINT.password);
    });

    it.each(['', '   '])(
        '密码为空白 %j 时保留原剪贴板，打开页面并说明未取到密码',
        async (password) => {
            mocks.openWebui.mockResolvedValue({ ...ENDPOINT, password });
            const { result } = renderHook(useOpenWebui);

            await result.current(ARGS);

            expect(mocks.writeText).not.toHaveBeenCalled();
            expect(mocks.openExternalUrl).toHaveBeenCalledWith(ENDPOINT.url);
            expect(mocks.pushInfoBar).toHaveBeenCalledWith(
                expect.objectContaining({
                    tone: 'warning',
                    title: '未读取到登录密码',
                    content: expect.stringContaining('WebUI 已打开'),
                }),
            );
        },
    );

    it('剪贴板拒绝时仍打开页面并明确提示，不输出密码或原始异常', async () => {
        mocks.writeText.mockRejectedValue(new Error(`Clipboard denied: ${ENDPOINT.password}`));
        const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
        const error = vi.spyOn(console, 'error').mockImplementation(() => {});
        const { result } = renderHook(useOpenWebui);

        await result.current(ARGS);

        expect(mocks.openExternalUrl).toHaveBeenCalledWith(ENDPOINT.url);
        expect(mocks.pushInfoBar).toHaveBeenCalledWith(
            expect.objectContaining({ tone: 'warning', title: '未能复制登录密码' }),
        );
        expect(JSON.stringify(mocks.pushInfoBar.mock.calls)).not.toContain(ENDPOINT.password);
        expect(warn).not.toHaveBeenCalled();
        expect(error).not.toHaveBeenCalled();
    });

    it('取端点失败时交给上层报错，不复制或打开，也不宣称成功', async () => {
        const failure = new Error('远端隧道未就绪');
        mocks.openWebui.mockRejectedValue(failure);
        const { result } = renderHook(useOpenWebui);

        await expect(result.current(ARGS)).rejects.toBe(failure);

        expect(mocks.writeText).not.toHaveBeenCalled();
        expect(mocks.openExternalUrl).not.toHaveBeenCalled();
        expect(mocks.pushInfoBar).not.toHaveBeenCalled();
    });

    it('浏览器打开失败时如实反馈密码已复制，并保留上层错误处理', async () => {
        const failure = new Error('系统浏览器打开失败');
        mocks.openExternalUrl.mockRejectedValue(failure);
        const { result } = renderHook(useOpenWebui);

        await expect(result.current(ARGS)).rejects.toBe(failure);

        expect(mocks.writeText).toHaveBeenCalledWith(ENDPOINT.password);
        expect(mocks.pushInfoBar).toHaveBeenCalledWith(
            expect.objectContaining({
                tone: 'warning',
                title: '密码已复制',
                content: expect.stringContaining('浏览器未能打开'),
            }),
        );
        expect(JSON.stringify(mocks.pushInfoBar.mock.calls)).not.toContain('WebUI 已打开');
    });

    it('NapCat 继续使用 token 链接，不复制密码或显示 SnowLuma 提示', async () => {
        const { result } = renderHook(useOpenWebui);

        await result.current({
            botId: 'napcat-one',
            flavor: 'napcat',
            napcat: { port: 6099, token: 'fixture token' },
        });

        expect(mocks.openExternalUrl).toHaveBeenCalledWith(
            'http://127.0.0.1:6099/webui?token=fixture%20token',
        );
        expect(mocks.openWebui).not.toHaveBeenCalled();
        expect(mocks.writeText).not.toHaveBeenCalled();
        expect(mocks.pushInfoBar).not.toHaveBeenCalled();
    });

    it('NapCat 链接尚未就绪时继续拒绝打开', async () => {
        const { result } = renderHook(useOpenWebui);

        await expect(result.current({ botId: 'napcat-one', flavor: 'napcat' })).rejects.toThrow(
            'NapCat WebUI 链接尚未就绪',
        );
        expect(mocks.openExternalUrl).not.toHaveBeenCalled();
        expect(mocks.writeText).not.toHaveBeenCalled();
    });
});
