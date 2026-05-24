// SnowLuma HotStart 模式下选择目标 QQ.exe 的 picker hook。
// 现状：BotBasicTab 打开 dialog 时主动 `load()`，避免页面打开就发请求。

import { useCallback, useState } from 'react';
import { botService, type QQProcessInfo } from '../../core/services/bot.service';

export type { QQProcessInfo };

export function useQQProcessList() {
    const [processes, setProcesses] = useState<QQProcessInfo[]>([]);
    const [isLoading, setIsLoading] = useState(false);
    const [error, setError] = useState<string | null>(null);

    const load = useCallback(async (): Promise<void> => {
        setIsLoading(true);
        setError(null);
        setProcesses([]);
        try {
            const result = await botService.listQQProcesses();
            setProcesses(result);
        } catch (err) {
            setError(`列出 QQ 进程失败: ${String(err)}`);
        } finally {
            setIsLoading(false);
        }
    }, []);

    const reset = useCallback(() => {
        setProcesses([]);
        setError(null);
        setIsLoading(false);
    }, []);

    return { processes, isLoading, error, load, reset };
}
