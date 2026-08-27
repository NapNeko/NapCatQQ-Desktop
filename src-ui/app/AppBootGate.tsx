// 首屏闸门：磁盘 UI 偏好就绪 → StartupSplash → AppNext。

import React, { useCallback, useEffect, useState } from 'react';
import './index.css';
import { StartupSplash } from './StartupSplash';
import { AppNext } from './AppNext';
import { hydrateAppUiPreferencesFromDisk } from '../hooks/preferences/useAppUiPreferencesBootstrap';
import { applySideEffects } from '../hooks/preferences/preferencesStore';
import { syncRootChromeBackground } from '../core/design/surfaceCanvas';
import { invoke } from '@tauri-apps/api/core';
import { RouteErrorBoundary } from '../shared/ui/RouteErrorBoundary';
import { perfMark, perfMeasure } from '../core/domain/performance/perfMarks';

export const AppBootGate: React.FC = () => {
    const [prefsReady, setPrefsReady] = useState(false);
    const [shellReady, setShellReady] = useState(false);
    const [splashDone, setSplashDone] = useState(false);

    useEffect(() => {
        applySideEffects();
        syncRootChromeBackground();
        void hydrateAppUiPreferencesFromDisk().finally(() => {
            syncRootChromeBackground();
            perfMark('prefs_ready', { once: true });
            setPrefsReady(true);
        });
    }, []);

    useEffect(() => {
        if (!prefsReady) return;
        const id = requestAnimationFrame(() => setShellReady(true));
        return () => cancelAnimationFrame(id);
    }, [prefsReady]);

    const handleSplashFinished = useCallback(() => {
        setSplashDone(true);
        document.getElementById('root')?.removeAttribute('aria-busy');
        perfMark('splash_exit', { once: true });
        perfMeasure('boot_prefs_to_splash_exit', 'prefs_ready', 'splash_exit');

        // 显示主窗口（避免透明窗口启动闪烁）
        void invoke('show_main_window').catch((err) => {
            console.error('[AppBootGate] 显示主窗口失败:', err);
        });
    }, []);

    if (!prefsReady) {
        return (
            <div
                className="fixed inset-0 z-[200] bg-canvas"
                role="status"
                aria-busy="true"
                aria-label="正在加载设置"
            />
        );
    }

    return (
        <div className="relative h-full min-h-0 w-full overflow-hidden bg-canvas">
            {/* 主界面预先挂载于底层，杜绝硬切闪白与布局跳动 */}
            <div className="relative z-0 h-full min-h-0 w-full">
                <RouteErrorBoundary title="主界面渲染失败">
                    <AppNext />
                </RouteErrorBoundary>
            </div>

            {/* 启动层：执行完毕后平滑透明度溶图淡出 */}
            {!splashDone ? (
                <StartupSplash shellReady={shellReady} onFinished={handleSplashFinished} />
            ) : null}
        </div>
    );
};

export default AppBootGate;