import type { ConfigFrontendPreferences } from '../../ipc/types';

const JSON_KEYS = new Set([
    'ncd.terminal.prefs.v1',
    'ncd.terminal.layout.v1',
    'ncd.chat.ui.v1',
    'ncd:bot_custom_order:v1',
]);
const NICKNAME_PREFIX = 'ncd.maibot.chat.name.';
const RESTORED_EVENT = 'ncd:config-preferences-restored';
const MAX_VALUE_BYTES = 512 * 1024;
const MAX_TOTAL_BYTES = 2 * 1024 * 1024;

function isRecord(value: unknown): value is Record<string, unknown> {
    return value !== null && typeof value === 'object' && !Array.isArray(value);
}

function allowedKey(key: string): boolean {
    return JSON_KEYS.has(key) || (key.startsWith(NICKNAME_PREFIX)
        && /^[A-Za-z0-9_-]{1,160}$/.test(key.slice(NICKNAME_PREFIX.length)));
}

function invalid(key: string): never {
    throw new Error(`界面偏好格式无效：${key}`);
}

function stringList(value: unknown, maxLength = 1024): value is string[] {
    return Array.isArray(value) && value.length <= maxLength
        && value.every(item => typeof item === 'string' && item.length <= 256);
}

function validateJsonPreference(key: string, value: unknown): void {
    if (key === 'ncd:bot_custom_order:v1') {
        if (!stringList(value) || value.some(id => id.length === 0)) invalid(key);
        return;
    }
    if (!isRecord(value)) invalid(key);
    const data = value as Record<string, unknown>;
    const numbers = key === 'ncd.terminal.prefs.v1'
        ? ['fontSize', 'lineHeight', 'scrollback']
        : key === 'ncd.terminal.layout.v1'
            ? ['height', 'filesWidth'] : ['listWidth', 'composerHeight'];
    for (const field of numbers) {
        const item = data[field];
        if (field === 'composerHeight' && item === null) continue;
        if (item !== undefined && (typeof item !== 'number' || !Number.isFinite(item))) invalid(key);
    }
    const booleans = key === 'ncd.terminal.prefs.v1'
        ? ['cursorBlink', 'copyOnSelect', 'highlight', 'confirmMultilinePaste', 'gpu']
        : key === 'ncd.terminal.layout.v1' ? ['filesOpen'] : [];
    for (const field of booleans) {
        if (data[field] !== undefined && typeof data[field] !== 'boolean') invalid(key);
    }
    if (key === 'ncd.terminal.prefs.v1') {
        const choices: Record<string, readonly unknown[]> = {
            cursorStyle: ['block', 'bar', 'underline'],
            rightClick: ['menu', 'paste'],
            colorScheme: ['auto', 'dark'],
            defaultShell: [null, 'pwsh', 'windows_powershell', 'cmd', 'git_bash', 'wsl'],
        };
        for (const [field, allowed] of Object.entries(choices)) {
            if (data[field] !== undefined && !allowed.includes(data[field])) invalid(key);
        }
        if (data.snippets !== undefined && (!Array.isArray(data.snippets) || data.snippets.length > 1000
            || data.snippets.some(item => !isRecord(item) || typeof item.label !== 'string'
                || typeof item.command !== 'string' || item.label.length > 256 || item.command.length > 32768))) {
            invalid(key);
        }
    }
    if (key === 'ncd.chat.ui.v1' && data.hiddenConversations !== undefined) {
        if (!isRecord(data.hiddenConversations) || Object.keys(data.hiddenConversations).length > 128
            || Object.values(data.hiddenConversations).some(keys => !stringList(keys))) invalid(key);
    }
}

export function validateFrontendPreferences(value: unknown): asserts value is ConfigFrontendPreferences {
    if (!isRecord(value) || value.version !== 1 || !isRecord(value.storage)) {
        throw new Error('界面偏好备份格式不受支持');
    }
    const entries = Object.entries(value.storage);
    if (entries.length > 512) throw new Error('界面偏好条目过多');
    const encoder = new TextEncoder();
    if (encoder.encode(JSON.stringify(value)).length > MAX_TOTAL_BYTES) throw new Error('界面偏好备份过大');
    for (const [key, raw] of entries) {
        if (!allowedKey(key)) throw new Error(`不支持恢复此界面偏好：${key}`);
        if (typeof raw !== 'string' || encoder.encode(raw).length > MAX_VALUE_BYTES) invalid(key);
        if (JSON_KEYS.has(key)) {
            let parsed: unknown;
            try { parsed = JSON.parse(raw as string); } catch { invalid(key); }
            validateJsonPreference(key, parsed);
        } else if ((raw as string).length > 128 || (raw as string).includes('\0')) {
            invalid(key);
        }
    }
}

export function collectFrontendPreferences(storage: Storage = window.localStorage): ConfigFrontendPreferences {
    const snapshot: ConfigFrontendPreferences = { version: 1, storage: {} };
    for (let index = 0; index < storage.length; index += 1) {
        const key = storage.key(index);
        if (key === null || !allowedKey(key)) continue;
        const value = storage.getItem(key);
        if (value !== null) snapshot.storage[key] = value;
    }
    validateFrontendPreferences(snapshot);
    return snapshot;
}

export function restoreFrontendPreferences(
    snapshot: ConfigFrontendPreferences,
    storage: Storage = window.localStorage,
): void {
    validateFrontendPreferences(snapshot);
    // 快照缺省的白名单键一并清掉：报告「偏好已恢复」时目标机不该残留旧值。先收集再动手，避免遍历中删键错位
    const omitted: string[] = [];
    for (let index = 0; index < storage.length; index += 1) {
        const key = storage.key(index);
        if (key !== null && allowedKey(key) && !(key in snapshot.storage)) omitted.push(key);
    }
    const touched = [...omitted, ...Object.keys(snapshot.storage)];
    const before = new Map(touched.map(key => [key, storage.getItem(key)]));
    const attempted: string[] = [];
    try {
        for (const key of omitted) {
            attempted.push(key);
            storage.removeItem(key);
        }
        for (const [key, value] of Object.entries(snapshot.storage)) {
            attempted.push(key);
            storage.setItem(key, value);
        }
    } catch (error) {
        const rollbackErrors: string[] = [];
        for (const key of attempted.reverse()) {
            try {
                const original = before.get(key);
                if (original == null) storage.removeItem(key);
                else storage.setItem(key, original);
            } catch (rollbackError) {
                rollbackErrors.push(String(rollbackError));
            }
        }
        const message = error instanceof Error ? error.message : String(error);
        throw new Error(`界面偏好恢复失败：${message}${rollbackErrors.length ? `；部分原偏好未能恢复：${rollbackErrors.join('；')}` : ''}`);
    }
    if (storage === window.localStorage) {
        window.dispatchEvent(new CustomEvent<string[]>(RESTORED_EVENT, { detail: Object.keys(snapshot.storage) }));
    }
}

/** 同窗口导入和其他窗口的存储变化都通过对应偏好模块重新水合。 */
export function onFrontendPreferenceRestored(key: string, restore: () => void): () => void {
    const local = (event: Event) => {
        if ((event as CustomEvent<string[]>).detail?.includes(key)) restore();
    };
    const external = (event: StorageEvent) => {
        if (event.key === key && event.storageArea === window.localStorage) restore();
    };
    window.addEventListener(RESTORED_EVENT, local);
    window.addEventListener('storage', external);
    return () => {
        window.removeEventListener(RESTORED_EVENT, local);
        window.removeEventListener('storage', external);
    };
}
