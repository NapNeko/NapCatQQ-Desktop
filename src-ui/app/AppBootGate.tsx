// 首屏闸门：StartupSplash 后再挂载 AppNext（同步路由，无 lazy）。

import React, { useCallback, useEffect, useState } from 'react';
import './index.css';
import { StartupSplash } from './StartupSplash';
import { AppNext } from './AppNext';

export const AppBootGate: React.FC = () => {
    const [shellReady, setShellReady] = useState(false);
    const [showApp, setShowApp] = useState(false);

    // 给 Splash 至少一帧绘制；主包已在入口同步拉齐。
    useEffect(() => {
        const id = requestAnimationFrame(() => setShellReady(true));
        return () => cancelAnimationFrame(id);
    }, []);

    const handleSplashFinished = useCallback(() => {
        setShowApp(true);
        document.getElementById('root')?.removeAttribute('aria-busy');
    }, []);

    return (
        <>
            {!showApp && (
                <StartupSplash shellReady={shellReady} onFinished={handleSplashFinished} />
            )}
            {showApp && <AppNext />}
        </>
    );
};

export default AppBootGate;