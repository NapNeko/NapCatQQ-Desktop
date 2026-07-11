// 首屏闸门：磁盘 UI 偏好就绪 → StartupSplash → AppNext。

import React, { useCallback, useEffect, useState } from 'react';
import './index.css';
import { StartupSplash } from './StartupSplash';
import { AppNext } from './AppNext';
import { SplashConfetti } from '../shared/ui/motion';
import { useMotion } from '../hooks/preferences/useMotion';
import { hydrateAppUiPreferencesFromDisk } from '../hooks/preferences/useAppUiPreferencesBootstrap';
import { applySideEffects } from '../hooks/preferences/preferencesStore';
import { syncRootChromeBackground } from '../core/design/surfaceCanvas';
import { invoke } from '@tauri-apps/api/core';
import { RouteErrorBoundary } from '../shared/ui/RouteErrorBoundary';

export const AppBootGate: React.FC = () => {
    const [prefsReady, setPrefsReady] = useState(false);
    const [shellReady, setShellReady] = useState(false);
    const [showApp, setShowApp] = useState(false);
    const [confetti, setConfetti] = useState(false);
    const { enabled, level } = useMotion();

    useEffect(() => {
        applySideEffects();
        syncRootChromeBackground();
        void hydrateAppUiPreferencesFromDisk().finally(() => {
            syncRootChromeBackground();
            setPrefsReady(true);
        });
    }, []);

    useEffect(() => {
        if (!prefsReady) return;
        const id = requestAnimationFrame(() => setShellReady(true));
        return () => cancelAnimationFrame(id);
    }, [prefsReady]);

    const handleSplashFinished = useCallback(() => {
        setShowApp(true);
        if (enabled && level !== 'elegant') {
            setConfetti(true);
        }
        document.getElementById('root')?.removeAttribute('aria-busy');

        // 显示主窗口（避免透明窗口启动闪烁）
        void invoke('show_main_window').catch((err) => {
            console.error('[AppBootGate] 显示主窗口失败:', err);
        });
    }, [enabled, level]);

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
        <div className="relative h-full min-h-0 w-full">
            {showApp ? (
                <div className="relative z-0 h-full min-h-0 w-full">
                    <RouteErrorBoundary title="主界面渲染失败">
                        <AppNext />
                    </RouteErrorBoundary>
                </div>
            ) : null}
            {!showApp ? (
                <StartupSplash shellReady={shellReady} onFinished={handleSplashFinished} />
            ) : null}
            {confetti ? (
                <SplashConfetti onDone={() => setConfetti(false)} />
            ) : null}
        </div>
    );
};

export default AppBootGate;