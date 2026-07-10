// 启动后根据 BootstrapSnapshot.layout_consolidate 推一次 InfoBar。
// 成功：提示已备份到桌面（含密钥）并完成整理；失败：给原因，不阻断主界面。

import { useEffect, useRef } from 'react';
import type { BootstrapSnapshot } from '../../core/ipc/types';
import { resolveLayoutConsolidateAlert } from '../../core/domain/bootstrap/layout-consolidate-alert';
import { pushInfoBar } from '../ui/globalInfoBarStore';

const BAR_KEY = 'data-layout-consolidate';

/**
 * bootstrap 就绪后展示布局收敛结果；同会话只推一次（key 顶替 + ref）。
 */
export function useDataLayoutConsolidateAlert(
    bootstrap: BootstrapSnapshot | null | undefined,
): void {
    const shownRef = useRef(false);

    useEffect(() => {
        if (!bootstrap || shownRef.current) return;
        const alert = resolveLayoutConsolidateAlert(bootstrap.layout_consolidate);
        if (alert.kind === 'none') return;

        shownRef.current = true;
        pushInfoBar({
            key: BAR_KEY,
            tone: alert.kind,
            title: alert.title,
            content: alert.content,
            autoDismissMs: alert.autoDismissMs,
        });
    }, [bootstrap]);
}
