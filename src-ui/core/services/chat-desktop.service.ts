// 聊天账号用途、内存交接和独立窗口的类型化入口。
import { invoke, isTauri, listen } from '../ipc/transport';
import type { ChatAccountPreference } from '../ipc/generated/chat/ChatAccountPreference';
import type { ChatDesktopStatus } from '../ipc/generated/chat/ChatDesktopStatus';
import type { ChatViewState } from '../ipc/generated/chat/ChatViewState';
import type { ChatWindowRequest } from '../ipc/generated/chat/ChatWindowRequest';
import type { ChatWindowState } from '../ipc/generated/chat/ChatWindowState';
import type { ChatTrayNavigation } from '../ipc/generated/chat/ChatTrayNavigation';

export const CHAT_WINDOW_LABEL = 'chat-panel';
let popout = !isTauri && new URLSearchParams(location.search).has('chatPanel');
let restoreReading = popout;
let previewEmbedRequested = !isTauri && new URLSearchParams(location.search).has('chatEmbed');
let previewView: ChatViewState = { v: 1, revision: 0, selectedBot: null, accounts: [] };
let previewPreferences: ChatAccountPreference[] = [];
export const markChatPopoutWindow = () => {
    popout = true;
    restoreReading = true;
};
export const isChatPopoutWindow = () => popout;

export const chatDesktopService = {
    takeTrayNavigation: (): Promise<ChatTrayNavigation | null> =>
        isTauri && popout ? invoke('chat_take_tray_navigation') : Promise.resolve(null),
    async status(): Promise<ChatDesktopStatus> {
        if (isTauri) return invoke('chat_desktop_status');
        const { chatService } = await import('./chat.service');
        const targets = await chatService.targets();
        return {
            v: 1,
            accounts: targets.map((target) => ({
                target,
                preference: previewPreferences.find(
                    (p) => p.botId === target.bot_id && p.selfId === String(target.qq_id),
                ) ?? {
                    botId: target.bot_id,
                    selfId: String(target.qq_id),
                    enabled: false,
                    background: false,
                    tray: false,
                    trayNotification: 'badge',
                    ignoredGroups: [],
                    hiddenGroups: [],
                    notifyUnknownGroups: false,
                    preventRecall: false,
                },
                unread: 0,
                notificationUnread: 0,
                groups:
                    target.backend === 'napcat'
                        ? [
                              { groupId: '20001', qqMuted: false },
                              { groupId: '20002', qqMuted: true },
                          ]
                        : [],
                connection: { state: 'stopped', reason: '预览' },
                error: null,
            })),
        };
    },
    async setPreference(preference: ChatAccountPreference): Promise<void> {
        if (isTauri) return invoke('chat_set_preference', { preference });
        previewPreferences = [
            ...previewPreferences.filter(
                (p) => p.botId !== preference.botId || p.selfId !== preference.selfId,
            ),
            preference,
        ];
    },
    async ignoreGroup(
        botId: string,
        selfId: string,
        groupId: string,
        ignored: boolean,
        hidden = false,
    ): Promise<void> {
        if (isTauri)
            return invoke('chat_set_group_ignored', { botId, selfId, groupId, ignored, hidden });
        const account = (await chatDesktopService.status()).accounts.find(
            (row) => row.target.bot_id === botId && row.preference.selfId === selfId,
        );
        if (!account) throw new Error('聊天账号不存在');
        const field = hidden ? 'hiddenGroups' : 'ignoredGroups';
        const groups = account.preference[field].filter((id) => id !== groupId);
        if (ignored) groups.push(groupId);
        await chatDesktopService.setPreference({ ...account.preference, [field]: groups });
    },
    async mergeHiddenGroups(botId: string, selfId: string, groups: string[]): Promise<void> {
        if (isTauri) return invoke('chat_merge_hidden_groups', { botId, selfId, groups });
        const account = (await chatDesktopService.status()).accounts.find(
            (row) => row.target.bot_id === botId && row.preference.selfId === selfId,
        );
        if (account)
            await chatDesktopService.setPreference({
                ...account.preference,
                hiddenGroups: [...new Set([...account.preference.hiddenGroups, ...groups])],
            });
    },
    selectAccount: (botId: string): Promise<void> =>
        isTauri && popout ? invoke('chat_select_account', { botId }) : Promise.resolve(),
    loadView: async (claim = false): Promise<ChatViewState> => {
        const keepReading = claim && restoreReading;
        if (claim) restoreReading = false;
        let view: ChatViewState;
        if (isTauri) view = await invoke('chat_view_load', { claim });
        else {
            try {
                const saved = JSON.parse(
                    sessionStorage.getItem('ncd.chat.preview.handoff') ?? 'null',
                ) as ChatViewState | null;
                if (saved?.v === 1 && Array.isArray(saved.accounts)) previewView = saved;
            } catch {
                /* 预览存储不可用时保留内存。 */
            }
            if (claim) previewView.revision = (previewView.revision ?? 0) + 1;
            view = structuredClone(previewView);
        }
        if (!claim) return view;
        return keepReading
            ? view
            : {
                  ...view,
                  accounts: view.accounts.map((account) => ({
                      ...account,
                      reading: {},
                      scroll: {},
                  })),
              };
    },
    async saveView(view: ChatViewState): Promise<void> {
        if (isTauri) return invoke('chat_view_save', { view });
        previewView = structuredClone(view);
        sessionStorage.setItem('ncd.chat.preview.handoff', JSON.stringify(previewView));
    },
    setReading: (botId: string, selfId: string, session: string | null): Promise<void> =>
        isTauri ? invoke('chat_set_reading', { botId, selfId, session }) : Promise.resolve(),
    markRead: (botId: string, selfId: string, session: string): Promise<void> =>
        isTauri ? invoke('chat_mark_read', { botId, selfId, session }) : Promise.resolve(),
    flush: (): Promise<void> => (isTauri ? invoke('chat_flush') : Promise.resolve()),
    releaseAccount: (botId: string, selfId: string): Promise<void> =>
        isTauri ? invoke('chat_release_account', { botId, selfId }) : Promise.resolve(),
    open: async (botId?: string): Promise<void> => {
        if (isTauri) return invoke('open_chat_window', { botId: botId ?? null, releaseMain: true });
        const { prepareChatHandoff, getChatSelectedBot } =
            await import('../../hooks/chat/chatStore');
        await prepareChatHandoff(botId ?? getChatSelectedBot());
        const url = new URL(location.href);
        url.searchParams.set('chatPanel', '1');
        window.open(url, 'ncd-chat-preview', 'width=880,height=700');
    },
    reveal: (): Promise<void> => (isTauri ? invoke('reveal_chat_window') : Promise.resolve()),
    focusIfOpen: (): Promise<boolean> =>
        isTauri ? invoke('focus_chat_window') : Promise.resolve(false),
    close: async (embed: boolean): Promise<void> => {
        if (isTauri) return invoke('close_chat_window', { embed });
        const { prepareChatHandoff, getChatSelectedBot } =
            await import('../../hooks/chat/chatStore');
        await prepareChatHandoff(getChatSelectedBot(), undefined, embed);
        if (embed) {
            const url = new URL(location.href);
            url.searchParams.delete('chatPanel');
            url.searchParams.set('chatEmbed', '1');
            location.assign(url);
        } else window.close();
    },
    windowState: async (): Promise<ChatWindowState> => {
        const embedRequested = previewEmbedRequested;
        previewEmbedRequested = false;
        const state: ChatWindowState = isTauri
            ? await invoke('chat_window_state')
            : { v: 1, detached: popout, embedRequested };
        if (state.embedRequested) restoreReading = true;
        return state;
    },
    onRequest: (cb: (request: ChatWindowRequest) => void): Promise<() => void> =>
        isTauri
            ? listen('chat-window-request', cb, { target: popout ? CHAT_WINDOW_LABEL : 'main' })
            : Promise.resolve(() => {}),
    onAccountSelected: (cb: () => void): Promise<() => void> =>
        isTauri
            ? listen('chat-account-selected', cb, { target: popout ? CHAT_WINDOW_LABEL : 'main' })
            : Promise.resolve(() => {}),
    onEmbedRequested: (cb: () => void): Promise<() => void> =>
        isTauri
            ? listen(
                  'chat-embed-requested',
                  () => {
                      restoreReading = true;
                      cb();
                  },
                  { target: 'main' },
              )
            : Promise.resolve(() => {}),
    reply: (requestId: string, error: string | null): Promise<void> =>
        isTauri ? invoke('chat_window_handoff_ready', { requestId, error }) : Promise.resolve(),
};
