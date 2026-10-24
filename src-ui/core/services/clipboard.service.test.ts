// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { clipboardService } from './clipboard.service';

const transport = vi.hoisted(() => ({ invoke: vi.fn(), isTauri: true }));
vi.mock('../ipc/transport', () => ({
    invoke: transport.invoke,
    get isTauri() {
        return transport.isTauri;
    },
}));

const browserWrite = vi.fn();
const TEXT = 'fixture text';

beforeEach(() => {
    vi.resetAllMocks();
    transport.isTauri = true;
    transport.invoke.mockResolvedValue(undefined);
    browserWrite.mockResolvedValue(undefined);
    vi.stubGlobal('navigator', { clipboard: { writeText: browserWrite } });
});

afterEach(() => vi.unstubAllGlobals());

describe('clipboardService.writeText', () => {
    it('桌面通过原生命令写入，不依赖 WebView Clipboard API', async () => {
        expect(await clipboardService.writeText(TEXT)).toBeUndefined();

        expect(transport.invoke).toHaveBeenCalledOnce();
        expect(transport.invoke).toHaveBeenCalledWith('write_clipboard_text', { text: TEXT });
        expect(browserWrite).not.toHaveBeenCalled();
    });

    it('原生写入失败直接返回给调用方，不回退到另一条剪贴板路径', async () => {
        const failure = new Error('剪贴板正在被占用');
        transport.invoke.mockRejectedValue(failure);

        await expect(clipboardService.writeText(TEXT)).rejects.toBe(failure);

        expect(browserWrite).not.toHaveBeenCalled();
    });

    it('浏览器预览使用 Clipboard API，不调用 Tauri', async () => {
        transport.isTauri = false;

        await clipboardService.writeText(TEXT);

        expect(browserWrite).toHaveBeenCalledWith(TEXT);
        expect(transport.invoke).not.toHaveBeenCalled();
    });

    it('浏览器拒绝写入时保留失败，供打开 WebUI 动作准确提示', async () => {
        transport.isTauri = false;
        const failure = new Error('clipboard denied');
        browserWrite.mockRejectedValue(failure);

        await expect(clipboardService.writeText(TEXT)).rejects.toBe(failure);

        expect(transport.invoke).not.toHaveBeenCalled();
    });
});
