// 首屏闸门：磁盘 UI 偏好就绪 → StartupSplash → AppNext。

import React, { useCallback, useEffect, useRef, useState } from 'react';
import './index.css';
import { StartupSplash } from './StartupSplash';
import { AppNext } from './AppNext';
import { hydrateAppUiPreferencesFromDisk } from '../hooks/preferences/useAppUiPreferencesBootstrap';
import { applySideEffects } from '../hooks/preferences/preferencesStore';
import { syncRootChromeBackground } from '../core/design/surfaceCanvas';
import { invoke } from '@tauri-apps/api/core';
import { isTauri } from '../core/ipc/transport';
import { RouteErrorBoundary } from '../shared/ui/RouteErrorBoundary';
import { perfMark, perfMeasure } from '../core/domain/performance/perfMarks';

export const AppBootGate: React.FC = () => {
    const [prefsReady, setPrefsReady] = useState(false);
    const [shellReady, setShellReady] = useState(false);
    const [splashDone, setSplashDone] = useState(false);
    // 主界面壳的入场动画（侧栏/标题栏/主区）押到 splash 开始揭示时才播，
    // 否则它们在 splash 底下就播完了，用户永远看不到。
    const [revealed, setRevealed] = useState(false);
    const windowShownRef = useRef(false);

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

    // 窗口配置 visible:false（透明窗口在内容画出来前 show 会闪边框阴影），
    // 等 splash 首帧真正上屏（双 rAF）再显示，动画才是给人看的而不是在隐形窗口里空转。
    useEffect(() => {
        if (!prefsReady || windowShownRef.current) return;
        let inner = 0;
        const outer = requestAnimationFrame(() => {
            inner = requestAnimationFrame(() => {
                if (windowShownRef.current) return;
                windowShownRef.current = true;
                perfMark('window_shown', { once: true });
                if (!isTauri) return;
                void invoke('show_main_window').catch((err) => {
                    console.error('[AppBootGate] 显示主窗口失败:', err);
                });
            });
        });
        return () => {
            cancelAnimationFrame(outer);
            cancelAnimationFrame(inner);
        };
    }, [prefsReady]);

    const handleReveal = useCallback(() => {
        setRevealed(true);
        perfMark('splash_reveal', { once: true });
    }, []);

    const handleSplashFinished = useCallback(() => {
        setRevealed(true);
        setSplashDone(true);
        document.getElementById('root')?.removeAttribute('aria-busy');
        perfMark('splash_exit', { once: true });
        perfMeasure('boot_prefs_to_splash_exit', 'prefs_ready', 'splash_exit');
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
            <div
                className="relative z-0 h-full min-h-0 w-full"
                data-boot-reveal={revealed ? 'on' : 'off'}
            >
                <RouteErrorBoundary title="主界面渲染失败">
                    <AppNext />
                </RouteErrorBoundary>
            </div>

            {!splashDone ? (
                <StartupSplash
                    shellReady={shellReady}
                    onReveal={handleReveal}
                    onFinished={handleSplashFinished}
                />
            ) : null}
        </div>
    );
};

export default AppBootGate;
