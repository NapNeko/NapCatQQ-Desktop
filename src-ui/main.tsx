import { AppProvidersNext } from './app/AppProvidersNext';
import { renderRoot as render } from './app/renderRoot';
import { isTauri } from './core/ipc/transport';
import { CHAT_WINDOW_LABEL, DEBUG_WINDOW_LABEL, isChatPopoutSearch } from './core/domain/windows';

async function currentWindowLabel(): Promise<string | null> {
    if (!isTauri) return null;
    try {
        const { getCurrentWindow } = await import('@tauri-apps/api/window');
        return getCurrentWindow().label;
    } catch {
        return null;
    }
}

async function renderDebugPopout(): Promise<void> {
    // 弹出窗是独立 WebView：跳过主应用的 Splash/启动闸门，但仍走 provider + 磁盘偏好水合，
    // 主题 / 动画 / 圆角才能跟主窗一致（偏好权威在 app-settings.json，不指望 localStorage 跨窗共享）
    const [preferences, store, surface, service, app] = await Promise.all([
        import('./hooks/preferences/useAppUiPreferencesBootstrap'),
        import('./hooks/preferences/preferencesStore'),
        import('./core/design/surfaceCanvas'),
        import('./core/services/debug-window.service'),
        import('./app/DebugPopoutApp'),
    ]);
    const { hydrateAppUiPreferencesFromDisk } = preferences;
    const { applySideEffects } = store;
    const { syncRootChromeBackground } = surface;
    const { markDebugPopoutWindow } = service;
    const { DebugPopoutApp } = app;
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
    const { startChat } = await import('./app/startChat');
    await startChat();
}

void (async () => {
    const label = await currentWindowLabel();
    if (label === CHAT_WINDOW_LABEL || (!isTauri && isChatPopoutSearch(location.search))) {
        await renderChatPopout();
        return;
    }
    if (label === DEBUG_WINDOW_LABEL) {
        await renderDebugPopout();
        return;
    }
    // 控制台依赖只在主窗载入，弹出窗不解析概览、安装和终端等模块。
    const { AppBootGate } = await import('./app/AppBootGate');
    render(
        <AppProvidersNext>
            <AppBootGate />
        </AppProvidersNext>,
    );
})();
