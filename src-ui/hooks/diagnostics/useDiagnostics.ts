// 诊断 / Demo 事件触发 hook，让 features 层不直接调 service。

import { useCallback } from 'react';
import { diagnosticsService } from '../../core/services/desktop.service';

export function useDiagnostics() {
    const publishDemo = useCallback(async (): Promise<void> => {
        try {
            await diagnosticsService.publishDemoEvent();
        } catch (err) {
            // eslint-disable-next-line no-console
            console.error('触发 Demo 事件失败:', err);
        }
    }, []);

    const publishRuntime = useCallback(async (): Promise<void> => {
        try {
            await diagnosticsService.publishRuntimeStatus();
        } catch (err) {
            // eslint-disable-next-line no-console
            console.error('触发 Runtime status 事件失败:', err);
        }
    }, []);

    return { publishDemo, publishRuntime };
}
