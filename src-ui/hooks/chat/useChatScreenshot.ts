// 截图绑定发起时的账号与会话，切换会话不改变附件去向。
import { useCallback, useEffect, useMemo, useRef, useState, useSyncExternalStore } from 'react';
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
import type { ChatScreenshotAttachment } from '../../core/ipc/generated/chat/ChatScreenshotAttachment';
import type { ChatScreenshotShortcutEvent } from '../../core/ipc/generated/chat/ChatScreenshotShortcutEvent';

const captureListeners = new Set<() => void>();
const preferenceListeners = new Set<() => void>();
let preferences: ChatScreenshotPreferences | undefined;
let shortcutConsumers = 0;
let appShortcutConsumers = 0;
let configuredShortcut: string | null = null;
let requestedShortcut: string | null = null;
let shortcutRevision = 0;
let shortcutCleanup: ReturnType<typeof setTimeout> | undefined;
interface Capture {
    store: ChatAccountStore;
    session: SessionKey;
    selfId: string;
    revision: number;
    addToChat: boolean;
    cancelled: boolean;
    nativeId?: string;
    onFinished?: () => void;
}
let activeCapture: Capture | null = null;
const shortcutContexts = new Map<string, Capture>();
const nativeCaptures = new Map<string, Capture>();
const completedNativeCaptures = new Set<string>();
let shortcutListening: Promise<void> | null = null;

function ensureShortcutListener() {
    // 完成结果可能在 Composer 卸载期间到达，监听随 WebView 存活，不跟会话重挂。
    return (shortcutListening ??= chatScreenshotService
        .onShortcut(receiveShortcut)
        .then(() => {})
        .catch((error) => {
            shortcutListening = null;
            throw error;
        }));
}

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

function configureShortcut(
    shortcut: string | null,
    global = true,
    context = '',
    hideWindow = true,
    releaseOwner = false,
    addToChat = true,
) {
    const identity =
        shortcut === null
            ? releaseOwner
                ? 'released'
                : null
            : JSON.stringify([global, shortcut, context, hideWindow, addToChat]);
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
            context,
            hideWindow,
            releaseOwner,
            addToChat,
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

function configureAppShortcut() {
    const preference = getPreferences();
    configureShortcut(
        preference.shortcut,
        preference.globalShortcut,
        '',
        preference.hideWindow,
        false,
        preference.addToChat,
    );
}

export async function cancelChatScreenshot(store?: ChatAccountStore): Promise<void> {
    const capture = activeCapture;
    if (!capture) {
        // 隐藏页可能还没收到 started；交接仍需取消原生侧已经启动的截图。
        if (!store) await chatScreenshotService.cancel().catch(() => {});
        return;
    }
    if (capture.cancelled || (store && capture.store !== store)) return;
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
    if (options.addToChat && (account.drafts[session] ?? EMPTY_DRAFT).attachments.length >= 8) {
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
        addToChat: options.addToChat,
        cancelled: false,
    };
    activeCapture = capture;
    emitCapture();
    try {
        const file = await chatScreenshotService.capture({
            hideWindow: options.hideWindow,
            addToChat: options.addToChat,
        });
        applyScreenshot(capture, file);
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
            if (!shortcutConsumers) shortcutContexts.clear();
            emitCapture();
        }
    }
}

function applyScreenshot(capture: Capture, file: ChatScreenshotAttachment | null) {
    const { store, session } = capture;
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
    if (!capture.addToChat) return;
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
}

function receiveShortcut(event: ChatScreenshotShortcutEvent) {
    if (completedNativeCaptures.has(event.capture_id)) return;
    if (event.result.kind === 'started') {
        const context = shortcutContexts.get(event.context);
        if (!context || nativeCaptures.has(event.capture_id)) return;
        const capture = { ...context, nativeId: event.capture_id, cancelled: false };
        nativeCaptures.set(event.capture_id, capture);
        if (!activeCapture) activeCapture = capture;
        emitCapture();
        return;
    }
    // 会话切换会重挂监听，finished 仍可用注册时锁定的上下文落回原草稿。
    const capture = nativeCaptures.get(event.capture_id) ?? shortcutContexts.get(event.context);
    if (!capture) return;
    completedNativeCaptures.add(event.capture_id);
    while (completedNativeCaptures.size > 32) {
        const oldest = completedNativeCaptures.values().next().value;
        if (oldest === undefined) break;
        completedNativeCaptures.delete(oldest);
    }
    try {
        if (event.result.kind === 'failed') throw new Error(event.result.message);
        applyScreenshot(capture, event.result.file);
        if (capture.addToChat && capture.store.getSnapshot().account.active === capture.session)
            capture.onFinished?.();
    } catch (error) {
        if (!capture.cancelled)
            pushInfoBar({
                key: 'chat:screenshot:capture',
                tone: 'danger',
                title: '截图未完成',
                content: errorText(error),
            });
    } finally {
        nativeCaptures.delete(event.capture_id);
        if (activeCapture?.nativeId === event.capture_id) activeCapture = null;
        if (!shortcutConsumers && !activeCapture) shortcutContexts.clear();
        emitCapture();
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
    const [shortcutReady, setShortcutReady] = useState(false);
    const mounted = useRef(false);
    const finished = useRef(onFinished);
    const selfId = store.getSnapshot().account.selfId;
    const viewRevision = store.viewRevision;
    const context = useMemo(
        () => [store.target.bot_id, selfId, session, viewRevision, crypto.randomUUID()].join('/'),
        [store, selfId, session, viewRevision],
    );
    finished.current = onFinished;
    useEffect(() => {
        mounted.current = true;
        let disposed = false;
        void ensureShortcutListener()
            .then(() => {
                if (!disposed) setShortcutReady(true);
            })
            .catch(() => {});
        return () => {
            mounted.current = false;
            disposed = true;
        };
    }, []);
    const start = useCallback(async () => {
        if (collapsed || activeCapture) return;
        const options = getPreferences();
        await captureChatScreenshot(store, session, options);
        if (options.addToChat && mounted.current && store.getSnapshot().account.active === session)
            finished.current?.();
    }, [store, session, collapsed]);
    useEffect(() => {
        if (shortcutSuspended) {
            if (shortcutCleanup !== undefined) clearTimeout(shortcutCleanup);
            configureShortcut(null);
            return;
        }
        if (collapsed || !shortcutReady) return;
        shortcutConsumers++;
        if (shortcutCleanup !== undefined) clearTimeout(shortcutCleanup);
        shortcutContexts.set(context, {
            store,
            session,
            selfId,
            revision: viewRevision,
            addToChat: preference.addToChat,
            cancelled: false,
            onFinished: () => {
                if (mounted.current) finished.current?.();
            },
        });
        // 正在截图的上下文另有持有；这里只留当前与上一个注册，避免留住已释放的账号。
        while (shortcutContexts.size > 2) {
            const oldest = shortcutContexts.keys().next().value;
            if (oldest === undefined) break;
            shortcutContexts.delete(oldest);
        }
        configureShortcut(
            preference.shortcut,
            preference.globalShortcut,
            context,
            preference.hideWindow,
            false,
            preference.addToChat,
        );
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
            window.removeEventListener('keydown', handle);
            shortcutConsumers--;
            // 会话切换会重挂 Composer，同一轮交接不反复注册原生快捷键。
            shortcutCleanup = setTimeout(() => {
                if (!shortcutConsumers) {
                    configureShortcut(null, true, '', true, true);
                    if (appShortcutConsumers) configureAppShortcut();
                    if (!activeCapture) shortcutContexts.clear();
                }
            }, 0);
        };
    }, [
        collapsed,
        shortcutSuspended,
        shortcutReady,
        preference.shortcut,
        preference.globalShortcut,
        preference.hideWindow,
        preference.addToChat,
        start,
        context,
        store,
        session,
        selfId,
        viewRevision,
    ]);
    return { capturing, preferences: preference, updatePreferences, start, suspendShortcut };
}

export function useAppScreenshotShortcut(enabled: boolean) {
    const preference = useSyncExternalStore(subscribePreferences, getPreferences, getPreferences);
    useEffect(() => {
        if (!enabled) {
            requestedShortcut = null;
            configuredShortcut = null;
            return;
        }
        appShortcutConsumers++;
        // 输入框会提供会话目标；启动兜底只负责让快捷键在其他页面也可用。
        if (!shortcutConsumers) configureAppShortcut();
        return () => {
            appShortcutConsumers--;
        };
    }, [enabled, preference]);
}
