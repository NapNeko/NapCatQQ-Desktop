// BootstrapPanel 用的 mock CPU/RAM 抖动。
// 真接入系统指标后这里会切换到 services 层。

import { useEffect, useState } from 'react';

export interface ResourceUsage {
    cpu: number;
    ram: number;
}

export function useResourceMonitor(): ResourceUsage {
    const [cpu, setCpu] = useState(12);
    const [ram, setRam] = useState(45);

    useEffect(() => {
        const t = setInterval(() => {
            setCpu((prev) => {
                const delta = Math.floor(Math.random() * 5) - 2;
                return Math.max(5, Math.min(30, prev + delta));
            });
            setRam((prev) => {
                const delta = Math.floor(Math.random() * 3) - 1;
                return Math.max(42, Math.min(48, prev + delta));
            });
        }, 2000);
        return () => clearInterval(t);
    }, []);

    return { cpu, ram };
}
