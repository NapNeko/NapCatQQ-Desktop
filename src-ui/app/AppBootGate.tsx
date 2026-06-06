// 首屏闸门：StartupSplash 后再挂载 AppNext（同步路由，无 lazy）。

import React, { useCallback, useEffect, useState } from 'react';
import './index.css';
import { StartupSplash } from './StartupSplash';
import { AppNext } from './AppNext';
import { SplashConfetti } from '../shared/ui/motion';
import { useMotion } from '../hooks/preferences/useMotion';

export const AppBootGate: React.FC = () => {
    const [shellReady, setShellReady] = useState(false);
    const [showApp, setShowApp] = useState(false);
    const [confetti, setConfetti] = useState(false);
    const { enabled, level } = useMotion();

    // 给 Splash 至少一帧绘制；主包已在入口同步拉齐。
    useEffect(() => {
        const id = requestAnimationFrame(() => setShellReady(true));
        return () => cancelAnimationFrame(id);
    }, []);

    const handleSplashFinished = useCallback(() => {
        setShowApp(true);
        if (enabled && level !== 'elegant') {
            setConfetti(true);
        }
        document.getElementById('root')?.removeAttribute('aria-busy');
    }, [enabled, level]);

    return (
        <>
            {!showApp && (
                <StartupSplash shellReady={shellReady} onFinished={handleSplashFinished} />
            )}
            {showApp && <AppNext />}
            {confetti && (
                <SplashConfetti onDone={() => setConfetti(false)} />
            )}
        </>
    );
};

export default AppBootGate;