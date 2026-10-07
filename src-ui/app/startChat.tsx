// 聊天窗从独立入口启动，偏好水合完成后再显示。
import { AppProvidersNext } from './AppProvidersNext';
import { renderRoot } from './renderRoot';
import { markChatPopoutWindow } from '../core/services/chat-desktop.service';

export async function startChat(): Promise<void> {
    markChatPopoutWindow();
    const [preferences, store, surface, app] = await Promise.all([
        import('../hooks/preferences/useAppUiPreferencesBootstrap'),
        import('../hooks/preferences/preferencesStore'),
        import('../core/design/surfaceCanvas'),
        import('./ChatPopoutApp'),
    ]);
    const { ChatPopoutApp } = app;
    store.applySideEffects();
    surface.syncRootChromeBackground();
    await preferences.hydrateAppUiPreferencesFromDisk();
    surface.syncRootChromeBackground();
    renderRoot(
        <AppProvidersNext>
            <ChatPopoutApp />
        </AppProvidersNext>,
    );
}
