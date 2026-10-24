// 桌面文本复制走系统剪贴板，浏览器预览保留 Clipboard API。
import { invoke, isTauri } from '../ipc/transport';

export const clipboardService = {
    writeText: async (text: string): Promise<void> => {
        if (isTauri) return invoke<void>('write_clipboard_text', { text });
        await navigator.clipboard.writeText(text);
    },
};
