// 账号托盘菜单与预览共用一个窗口，动作绑定本次展开的账号。
import { invoke, isTauri, listen } from '../ipc/transport';
import type { ChatTrayPanelData } from '../ipc/generated/chat/ChatTrayPanelData';
import type { ChatTrayPanelAction } from '../ipc/generated/chat/ChatTrayPanelAction';

export const CHAT_TRAY_PANEL_LABEL = 'chat-tray-panel';
export const isChatTrayPreview = () => !isTauri && new URLSearchParams(location.search).has('chatTray');
let previewBackground = true;

export const chatTrayService = {
    async data(): Promise<ChatTrayPanelData | null> {
        if (isTauri) return invoke('chat_tray_panel_data');
        const conversations = [
            { key: 'private:0', type: 'private' as const, id: '0', name: '林夕', unread: 2, pinned: false, lastAt: Date.now() - 60_000, preview: '刚才说的那个方案，我整理好了', boxed: false },
            { key: 'group:0', type: 'group' as const, id: '0', name: '开发讨论', unread: 12, pinned: false, lastAt: Date.now() - 180_000, preview: '小林：[图片]', boxed: false },
            { key: 'private:00', type: 'private' as const, id: '00', name: '阿橙', unread: 1, pinned: false, lastAt: Date.now() - 300_000, preview: '晚上见！', boxed: false },
        ];
        return { v: 1, generation: 1, menu: new URLSearchParams(location.search).get('chatTray') !== 'messages', snapshot: {
            v: 1, account: { target: { bot_id: 'tray-preview', qq_id: 0, name: '小南', backend: 'napcat', host: { kind: 'local' }, running: true, online: true },
                preference: { botId: 'tray-preview', selfId: '0', enabled: true, background: previewBackground, tray: true, trayNotification: 'badge', ignoredGroups: [], hiddenGroups: [], notifyUnknownGroups: false },
                unread: 15, notificationUnread: 15, groups: [], connection: { state: 'connected' }, error: null },
            conversations, conversationCount: conversations.length,
        } };
    },
    ready: (generation: number, height: number): Promise<void> => isTauri ? invoke('chat_tray_panel_ready', { generation, height }) : Promise.resolve(),
    hide: (generation: number): Promise<void> => isTauri ? invoke('chat_tray_panel_hide', { generation }) : Promise.resolve(),
    async action(generation: number, action: ChatTrayPanelAction, session?: string): Promise<void> {
        if (isTauri) return invoke('chat_tray_panel_action', { generation, action, session: session ?? null });
        if (action === 'background') previewBackground = !previewBackground;
    },
    onChanged: (callback: () => void): Promise<() => void> => isTauri
        ? listen('chat-tray-panel-changed', callback, { target: CHAT_TRAY_PANEL_LABEL }) : Promise.resolve(() => {}),
};
