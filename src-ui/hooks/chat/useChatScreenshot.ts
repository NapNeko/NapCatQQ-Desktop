// 截图绑定发起时的账号与会话，切换会话不改变附件去向。
import { useCallback, useEffect, useRef, useState, useSyncExternalStore } from 'react';
import {
    DEFAULT_SCREENSHOT_PREFERENCES,
    SCREENSHOT_PREFERENCES_KEY,
    parseScreenshotPreferences,
    screenshotShortcutFromEvent,
    type ChatScreenshotPreferences,
} from '../../core/domain/chat/chatScreenshotPreferences';
import { EMPTY_DRAFT, type SessionKey } from '../../core/domain/chat/model';
import { errorText } from '../../core/domain/errors';
import { chatScreenshotService } from '../../core/services/chat-screenshot.service';
import { isLocalFileToken, LOCAL_FILE_PREFIX } from '../../core/domain/debug/streamActions';
import { dismissInfoBar, pushInfoBar } from '../ui/globalInfoBarStore';
import type { ChatAccountStore } from './chatStore';
import { isTauri } from '../../core/domain/runtime/env';

const captureListeners = new Set<() => void>();
const preferenceListeners = new Set<() => void>();
let preferences: ChatScreenshotPreferences | undefined;
let shortcutConsumers = 0;
let configuredShortcut: string | null = null;
let requestedShortcut: string | null = null;
let shortcutRevision = 0;
let shortcutCleanup: ReturnType<typeof setTimeout> | undefined;
interface Capture {
    store: ChatAccountStore;
    session: SessionKey;
    selfId: string;
    revision: number;
    cancelled: boolean;
}
let activeCapture: Capture | null = null;

function readPreferences() {
    try {
        return parseScreenshotPreferences(localStorage.getItem(SCREENSHOT_PREFERENCES_KEY));
    } catch {
        return DEFAULT_SCREENSHOT_PREFERENCES;
    }
}
const getPreferences = () => (preferences ??= readPreferences());
const emitCapture = () => captureListeners.forEach((listener) => listener());
const emitPreferences = () => preferenceListeners.forEach((listener) => listener());
const getCapturing = () => activeCapture !== null;
const subscribeCapture = (listener: () => void) => {
    captureListeners.add(listener);
    return () => {
        captureListeners.delete(listener);
    };
};
function receivePreferences(event: StorageEvent) {
    if (event.key !== SCREENSHOT_PREFERENCES_KEY && event.key !== null) return;
    preferences = readPreferences();
    emitPreferences();
}
const subscribePreferences = (listener: () => void) => {
    if (!preferenceListeners.size) window.addEventListener('storage', receivePreferences);
    preferenceListeners.add(listener);
    return () => {
        preferenceListeners.delete(listener);
        if (!preferenceListeners.size) window.removeEventListener('storage', receivePreferences);
    };
};

function updatePreferences(patch: Partial<ChatScreenshotPreferences>) {
    preferences = parseScreenshotPreferences(JSON.stringify({ ...getPreferences(), ...patch }));
    try {
        localStorage.setItem(SCREENSHOT_PREFERENCES_KEY, JSON.stringify(preferences));
    } catch {
        pushInfoBar({
            key: 'chat:screenshot:preferences',
            tone: 'warning',
            title: '截图设置无法保存',
            content: '设置已在当前窗口生效，重开后会恢复默认值',
        });
    }
    emitPreferences();
}

function configureShortcut(shortcut: string | null, global = true) {
    const identity = shortcut === null ? null : `${global ? 'global' : 'focus'}:${shortcut}`;
    if (requestedShortcut === identity) return;
    requestedShortcut = identity;
    const revision = ++shortcutRevision;
    const parts = shortcut?.split('+') ?? [];
    void chatScreenshotService
        .shortcut({
            enabled: shortcut !== null,
            global,
            control: parts.includes('Ctrl'),
            alt: parts.includes('Alt'),
            shift: parts.includes('Shift'),
            key: parts.at(-1) ?? '',
        })
        .then(() => {
            configuredShortcut = identity;
            if (revision === shortcutRevision && shortcut)
                dismissInfoBar('key:chat:screenshot:shortcut');
        })
        .catch((error) => {
            if (revision !== shortcutRevision) return;
            // 注册冲突时原生侧保留旧键；页面关闭仍需释放它，同值重挂也可以重试。
            requestedShortcut = configuredShortcut;
            if (shortcut)
                pushInfoBar({
                    key: 'chat:screenshot:shortcut',
                    tone: 'warning',
                    title: '截图快捷键未启用',
                    content: errorText(error),
                });
        });
}

export async function cancelChatScreenshot(store?: ChatAccountStore): Promise<void> {
    const capture = activeCapture;
    if (!capture || capture.cancelled || (store && capture.store !== store)) return;
    capture.cancelled = true;
    // 立即失效结果，原生窗口释放前仍保留单次调用锁。
    await chatScreenshotService.cancel().catch(() => {});
}

export async function captureChatScreenshot(
    store: ChatAccountStore,
    session: SessionKey,
    options: ChatScreenshotPreferences = getPreferences(),
): Promise<void> {
    if (activeCapture) return;
    const account = store.getSnapshot().account;
    if ((account.drafts[session] ?? EMPTY_DRAFT).attachments.length >= 8) {
        pushInfoBar({
            key: 'chat:screenshot:capture',
            tone: 'warning',
            title: '无法添加截图',
            content: '一次最多添加 8 个附件',
        });
        return;
    }
    const capture: Capture = {
        store,
        session,
        selfId: account.selfId,
        revision: store.viewRevision,
        cancelled: false,
    };
    activeCapture = capture;
    emitCapture();
    try {
        const file = await chatScreenshotService.capture({ hideWindow: options.hideWindow });
        if (
            !file ||
            capture.cancelled ||
            store.releaseWhenIdle ||
            store.viewRevision !== capture.revision ||
            store.getSnapshot().account.selfId !== capture.selfId
        )
            return;
        const latest = store.getSnapshot().account.drafts[session] ?? EMPTY_DRAFT;
        if (file.clipboardError)
            pushInfoBar({
                key: 'chat:screenshot:clipboard',
                tone: 'warning',
                title: '截图已完成，剪贴板未更新',
                content: file.clipboardError,
            });
        if (latest.attachments.length >= 8) throw new Error('一次最多添加 8 个附件');
        const path = isLocalFileToken(file.path)
            ? file.path.slice(LOCAL_FILE_PREFIX.length)
            : file.path;
        store.draft(session, {
            ...latest,
            attachments: [
                ...latest.attachments,
                {
                    key: crypto.randomUUID(),
                    type: 'image',
                    path,
                    previewPath: file.previewPath,
                    name: file.name,
                },
            ],
        });
    } catch (error) {
        if (!capture.cancelled)
            pushInfoBar({
                key: 'chat:screenshot:capture',
                tone: 'danger',
                title: '截图未完成',
                content: errorText(error),
            });
    } finally {
        if (activeCapture === capture) {
            activeCapture = null;
            emitCapture();
        }
    }
}

export function useChatScreenshot(
    store: ChatAccountStore,
    session: SessionKey,
    collapsed: boolean,
    onFinished?: () => void,
) {
    const capturing = useSyncExternalStore(subscribeCapture, getCapturing, getCapturing);
    const preference = useSyncExternalStore(subscribePreferences, getPreferences, getPreferences);
    const [shortcutSuspended, suspendShortcut] = useState(false);
    const mounted = useRef(false);
    const finished = useRef(onFinished);
    finished.current = onFinished;
    useEffect(() => {
        mounted.current = true;
        return () => {
            mounted.current = false;
        };
    }, []);
    const start = useCallback(async () => {
        if (collapsed || activeCapture) return;
        await captureChatScreenshot(store, session);
        if (mounted.current && store.getSnapshot().account.active === session) finished.current?.();
    }, [store, session, collapsed]);
    useEffect(() => {
        if (collapsed || shortcutSuspended) return;
        let disposed = false;
        let unlisten: (() => void) | undefined;
        shortcutConsumers++;
        if (shortcutCleanup !== undefined) clearTimeout(shortcutCleanup);
        configureShortcut(preference.shortcut, preference.globalShortcut);
        void chatScreenshotService
            .onShortcut(() => {
                if (!disposed) void start();
            })
            .then((stop) => {
                if (disposed) stop();
                else unlisten = stop;
            })
            .catch(() => {});
        const handle = (event: globalThis.KeyboardEvent) => {
            if (isTauri) return;
            if (
                event.defaultPrevented ||
                event.isComposing ||
                event.repeat ||
                document.visibilityState === 'hidden' ||
                (event.target instanceof Element &&
                    event.target.closest('[data-chat-screenshot-shortcut]')) ||
                screenshotShortcutFromEvent(event) !== preference.shortcut
            )
                return;
            event.preventDefault();
            void start();
        };
        window.addEventListener('keydown', handle);
        return () => {
            disposed = true;
            unlisten?.();
            window.removeEventListener('keydown', handle);
            shortcutConsumers--;
            // 会话切换会重挂 Composer，同一轮交接不反复注册原生快捷键。
            shortcutCleanup = setTimeout(() => {
                if (!shortcutConsumers) configureShortcut(null);
            }, 0);
        };
    }, [collapsed, shortcutSuspended, preference.shortcut, preference.globalShortcut, start]);
    return { capturing, preferences: preference, updatePreferences, start, suspendShortcut };
}
