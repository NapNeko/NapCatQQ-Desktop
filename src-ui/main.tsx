import React from 'react';
import ReactDOM from 'react-dom/client';
import { AppBootGate } from './app/AppBootGate';
import { AppProvidersNext } from './app/AppProvidersNext';
import { TrayPanel } from './modules/tray/TrayPanel';
import { isTauri } from './core/ipc/transport';
import { DEBUG_WINDOW_LABEL } from './core/services/debug-window.service';
import { CHAT_WINDOW_LABEL, isChatPopoutWindow, markChatPopoutWindow } from './core/services/chat-desktop.service';

// 屏蔽 WebView/浏览器默认右键菜单（后退/刷新/审查），输入框除外（保留系统复制粘贴）
document.addEventListener('contextmenu', (e) => {
    const target = e.target as HTMLElement | null;
    const isInput =
        target instanceof HTMLInputElement ||
        target instanceof HTMLTextAreaElement ||
        target?.isContentEditable;
    if (!isInput) {
        e.preventDefault();
    }
});

async function currentWindowLabel(): Promise<string | null> {
    if (!isTauri) return null;
    try {
        const { getCurrentWindow } = await import('@tauri-apps/api/window');
        return getCurrentWindow().label;
    } catch {
        return null;
    }
}

const root = ReactDOM.createRoot(document.getElementById('root') as HTMLElement);

function render(tree: React.ReactElement) {
    // dev 下 StrictMode 会双挂载，放大 IPC 预热与动画初始化；仅生产启用。
    if (import.meta.env.PROD) {
        root.render(<React.StrictMode>{tree}</React.StrictMode>);
    } else {
        root.render(tree);
    }
}

async function renderTrayPanel(): Promise<void> {
    // 托盘面板是独立小窗:跳过主应用的 Splash/启动闸门,但仍要走 provider + 磁盘偏好水合
    // 才能拿到主题 data-theme / 圆角 token。
    const { hydrateAppUiPreferencesFromDisk } = await import(
        './hooks/preferences/useAppUiPreferencesBootstrap'
    );
    const { applySideEffects } = await import('./hooks/preferences/preferencesStore');
    const { syncRootChromeBackground } = await import('./core/design/surfaceCanvas');
    applySideEffects();
    syncRootChromeBackground();
    await hydrateAppUiPreferencesFromDisk().finally(() => {
        syncRootChromeBackground();
    });
    render(
        <AppProvidersNext>
            <TrayPanel />
        </AppProvidersNext>,
    );
}

async function renderDebugPopout(): Promise<void> {
    // 弹出窗是独立 WebView：跳过主应用的 Splash/启动闸门，但仍走 provider + 磁盘偏好水合，
    // 主题 / 动画 / 圆角才能跟主窗一致（偏好权威在 app-settings.json，不指望 localStorage 跨窗共享）
    const { hydrateAppUiPreferencesFromDisk } = await import(
        './hooks/preferences/useAppUiPreferencesBootstrap'
    );
    const { applySideEffects } = await import('./hooks/preferences/preferencesStore');
    const { syncRootChromeBackground } = await import('./core/design/surfaceCanvas');
    const { markDebugPopoutWindow } = await import('./core/services/debug-window.service');
    const { DebugPopoutApp } = await import('./app/DebugPopoutApp');
    markDebugPopoutWindow();
    applySideEffects();
    syncRootChromeBackground();
    await hydrateAppUiPreferencesFromDisk().finally(() => {
        syncRootChromeBackground();
    });
    render(
        <AppProvidersNext>
            <DebugPopoutApp />
        </AppProvidersNext>,
    );
}

async function renderChatPopout(): Promise<void> {
    markChatPopoutWindow();
    const { hydrateAppUiPreferencesFromDisk } = await import('./hooks/preferences/useAppUiPreferencesBootstrap');
    const { applySideEffects } = await import('./hooks/preferences/preferencesStore');
    const { syncRootChromeBackground } = await import('./core/design/surfaceCanvas');
    const { ChatPopoutApp } = await import('./app/ChatPopoutApp');
    applySideEffects(); syncRootChromeBackground();
    await hydrateAppUiPreferencesFromDisk(); syncRootChromeBackground();
    render(<AppProvidersNext><ChatPopoutApp /></AppProvidersNext>);
}

void (async () => {
    const label = await currentWindowLabel();
    if (label === CHAT_WINDOW_LABEL || isChatPopoutWindow()) {
        await renderChatPopout(); return;
    }
    if (label === 'tray-panel') {
        await renderTrayPanel();
        return;
    }
    if (label === DEBUG_WINDOW_LABEL) {
        await renderDebugPopout();
        return;
    }
    render(
        <AppProvidersNext>
            <AppBootGate />
        </AppProvidersNext>,
    );
})();
