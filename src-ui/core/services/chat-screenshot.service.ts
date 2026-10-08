// 截图像素留在原生侧，Chat 只接收附件文件引用。
import { invoke, isTauri, listen } from '../ipc/transport';
import type { ChatScreenshotRequest } from '../ipc/generated/chat/ChatScreenshotRequest';
import type { ChatScreenshotAttachment } from '../ipc/generated/chat/ChatScreenshotAttachment';
import type { ChatScreenshotShortcut } from '../ipc/generated/chat/ChatScreenshotShortcut';
import type { ChatScreenshotShortcutEvent } from '../ipc/generated/chat/ChatScreenshotShortcutEvent';
import { CHAT_WINDOW_LABEL, isChatPopoutWindow } from './chat-desktop.service';

export type { ChatScreenshotRequest, ChatScreenshotAttachment };
let shortcutQueue = Promise.resolve();

export const chatScreenshotService = {
    capture(request: ChatScreenshotRequest): Promise<ChatScreenshotAttachment | null> {
        if (!isTauri) return Promise.reject(new Error('请在桌面端使用截图'));
        return invoke('chat_screenshot_capture', { request });
    },
    cancel(): Promise<void> {
        return isTauri ? invoke('chat_screenshot_cancel') : Promise.resolve();
    },
    shortcut(request: ChatScreenshotShortcut): Promise<void> {
        if (!isTauri) return Promise.resolve();
        const operation = shortcutQueue.then(() =>
            invoke<void>('chat_screenshot_shortcut', { request }),
        );
        shortcutQueue = operation.catch(() => {});
        return operation;
    },
    onShortcut(callback: () => void): Promise<() => void> {
        return isTauri
            ? listen<ChatScreenshotShortcutEvent>(
                  'chat-screenshot-shortcut',
                  (event) => {
                      if (event.v === 1) callback();
                  },
                  { target: isChatPopoutWindow() ? CHAT_WINDOW_LABEL : 'main' },
              )
            : Promise.resolve(() => {});
    },
};
