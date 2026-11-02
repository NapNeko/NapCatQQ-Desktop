import { accountKey } from '../../../core/domain/chat/model';
import { chatDesktopService } from '../../../core/services/chat-desktop.service';
import { inlineImageService } from '../../../core/services/inline-image.service';
import type { DebugTarget } from '../../../core/ipc/generated/debug/DebugTarget';
import type { ChatAccountView } from '../../../core/ipc/generated/chat/ChatAccountView';
import type { ChatViewState } from '../../../core/ipc/generated/chat/ChatViewState';
import { cancelChatScreenshot } from '../useChatScreenshot';
import { ChatAccountStore } from './ChatAccountStore';

const accounts = new Map<string, ChatAccountStore>();
const views = new Map<string, ChatAccountView>();
let selectedChatBot: string | null = null;
let viewRevision = 0;
let handoffInProgress = false;
let viewSaveQueue: Promise<void> = Promise.resolve();
let viewLoadRequest: Promise<ChatViewState> | null = null;
export const selectChatBot = (bot: string) => {
    selectedChatBot = bot;
};
export const getChatSelectedBot = () => selectedChatBot;
export function chatAccount(target: DebugTarget): ChatAccountStore {
    const key = accountKey(target.bot_id, String(target.qq_id));
    let store = accounts.get(key);
    if (!store) {
        store = new ChatAccountStore(target);
        store.viewRevision = viewRevision;
        const view = views.get(key);
        if (view) store.restoreView(view);
        accounts.set(key, store);
    }
    store.target = target;
    return store;
}
export function reconcileChatAccounts(targets: DebugTarget[]) {
    const live = new Set(targets.map((t) => accountKey(t.bot_id, String(t.qq_id))));
    for (const key of views.keys()) if (!live.has(key)) views.delete(key);
    for (const [key, store] of accounts) {
        if (!live.has(key)) {
            void store
                .disconnect()
                .then(() =>
                    chatDesktopService.releaseAccount(
                        store.target.bot_id,
                        store.getSnapshot().account.selfId,
                    ),
                )
                .catch(() => {});
            accounts.delete(key);
        } else {
            const target = targets.find((t) => t.bot_id === store.target.bot_id);
            if (target) store.target = target;
        }
    }
}
export function restoreChatView(view: ChatViewState) {
    viewRevision = view.revision;
    views.clear();
    for (const account of view.accounts)
        views.set(accountKey(account.botId, account.selfId), account);
    selectedChatBot = view.selectedBot;
    for (const [key, store] of accounts) {
        store.viewRevision = viewRevision;
        const saved = views.get(key);
        if (saved) store.restoreView(saved);
    }
}
export function loadChatView() {
    if (viewLoadRequest) return viewLoadRequest;
    const request = viewSaveQueue.catch(() => {}).then(() => chatDesktopService.loadView(true));
    viewLoadRequest = request;
    void request
        .finally(() => {
            if (viewLoadRequest === request) viewLoadRequest = null;
        })
        .catch(() => {});
    return request;
}
const withoutReading = (view: ChatAccountView): ChatAccountView => ({
    ...view,
    reading: {},
    scroll: {},
});
function savedView(selectedBot: string | null, keepReading = false): ChatViewState {
    const saved = [...views.values()].filter(
        (view) =>
            view.active || Object.values(view.drafts).some((d) => d.text || d.attachments.length),
    );
    if (saved.length > 8) throw new Error('草稿账号超过 8 个，请先清理不再使用的草稿');
    return structuredClone({
        v: 1,
        revision: viewRevision,
        selectedBot,
        accounts: saved.map((view) =>
            keepReading && view.botId === selectedBot ? view : withoutReading(view),
        ),
    });
}
function persistView(view: ChatViewState) {
    const saved = viewSaveQueue.catch(() => {}).then(() => chatDesktopService.saveView(view));
    viewSaveQueue = saved;
    return saved;
}
export function releaseChatAccount(store: ChatAccountStore): Promise<void> {
    void cancelChatScreenshot(store);
    const key = accountKey(store.target.bot_id, store.getSnapshot().account.selfId);
    store.requestRelease(() => {
        void releaseChatAccount(store).catch(() => {});
    });
    if (store.viewRevision === viewRevision)
        views.set(key, handoffInProgress ? store.view() : withoutReading(store.view()));
    return store.release(
        () =>
            !handoffInProgress && store.viewRevision === viewRevision
                ? persistView(savedView(selectedChatBot))
                : Promise.resolve(),
        async () => {
            if (accounts.get(key) === store) accounts.delete(key);
            inlineImageService.release(store.target.bot_id, store.getSnapshot().account.selfId);
            await chatDesktopService.releaseAccount(
                store.target.bot_id,
                store.getSnapshot().account.selfId,
            );
        },
    );
}
export async function prepareChatHandoff(
    selectedBot: string | null,
    unmount?: () => void,
    keepReading = true,
) {
    await cancelChatScreenshot();
    // 控制台未挂聊天页时，runtime 中可能是上次独立窗留下的新草稿。
    if (!accounts.size) {
        unmount?.();
        await chatDesktopService.flush();
        return;
    }
    if ([...accounts.values()].some((store) => store.isSending()))
        throw new Error('消息仍在发送，请稍后切换窗口');
    for (const [key, store] of accounts) views.set(key, store.view());
    const view = savedView(selectedBot, keepReading);
    handoffInProgress = true;
    try {
        unmount?.();
        await persistView(view);
        await Promise.all([...accounts.values()].map(releaseChatAccount));
        await chatDesktopService.flush();
    } finally {
        handoffInProgress = false;
    }
}
