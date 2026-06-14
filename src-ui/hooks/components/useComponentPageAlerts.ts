// 组件页：清单加载失败、单点探测失败 → 全局 InfoBar；卡片只保留状态徽章，不堆长错误文案。
// 边沿 + 模块级抑制：用户关闭持久失败条后不再重复弹出，直到该条件至少恢复过一次。
//
// 挂载：ComponentsPage 顶层（与 useComponentActionErrors 并列）。

import { useEffect, useRef } from 'react';
import type { ComponentRow } from '../../core/domain/components/types';
import { pushInfoBar, dismissInfoBar } from '../ui/globalInfoBarStore';
import {
    clearComponentPageAlertSuppression,
    isComponentPageAlertSuppressed,
    suppressComponentPageAlert,
} from './componentPageAlertState';

const CATALOG_KEY = 'component-catalog';

function detectBannerKey(componentId: string, hostId: string): string {
    return `component-detect:${componentId}:${hostId}`;
}

function isDetectFailureReason(reason: string): boolean {
    const t = reason.trim();
    if (!t) return false;
    if (t === '正在探测') return false;
    return true;
}

function pushIfNotSuppressed(
    alertKey: string,
    opts: Parameters<typeof pushInfoBar>[0],
): void {
    if (isComponentPageAlertSuppressed(alertKey)) return;
    pushInfoBar({
        ...opts,
        key: alertKey,
        onUserDismiss: () => suppressComponentPageAlert(alertKey),
    });
}

/**
 * catalogError：整页清单拉取失败时推一条 danger InfoBar（顶替同 key）。
 * rows + activeHostId：仅当前选中主机的探测失败推 InfoBar；切主机时撤掉其它主机的条。
 * 用户手动关闭持久失败条后被抑制；当对应条件至少出现过一次「非失败」状态时清除抑制，允许下次新失败再弹。
 */
export function useComponentPageAlerts(
    rows: ComponentRow[],
    catalogError: Error | null,
    activeHostId: string | null,
): void {
    const lastCatalogMessageRef = useRef<string | null>(null);

    useEffect(() => {
        const key = CATALOG_KEY;
        if (!catalogError) {
            clearComponentPageAlertSuppression(key);
            if (lastCatalogMessageRef.current != null) {
                dismissInfoBar(`key:${key}`);
                lastCatalogMessageRef.current = null;
            }
            return;
        }
        const message = catalogError.message || '加载组件清单失败';
        lastCatalogMessageRef.current = message;
        pushIfNotSuppressed(key, {
            tone: 'danger',
            title: '组件清单加载失败',
            content: `${message}。可点击页面右上角「刷新」重试。`,
            autoDismissMs: 0,
        });
        console.error('[ComponentsPage] catalog failed:', catalogError);
    }, [catalogError]);

    useEffect(() => {
        for (const row of rows) {
            for (const hostRow of row.rows) {
                const key = detectBannerKey(hostRow.component_id, hostRow.host.host_id);
                if (activeHostId && hostRow.host.host_id !== activeHostId) {
                    dismissInfoBar(`key:${key}`);
                    continue;
                }
                const { status } = hostRow;
                if (
                    status.state === 'unknown' &&
                    isDetectFailureReason(status.reason)
                ) {
                    const heading = `${row.info.display_name} · ${hostRow.host.display_name}`;
                    pushIfNotSuppressed(key, {
                        tone: 'danger',
                        title: `${heading} · 探测失败`,
                        content: status.reason,
                        autoDismissMs: 0,
                    });
                    console.warn(
                        `[ComponentsPage] detect failed: ${hostRow.component_id}@${hostRow.host.host_id}`,
                        status.reason,
                    );
                } else {
                    clearComponentPageAlertSuppression(key);
                    dismissInfoBar(`key:${key}`);
                }
            }
        }
    }, [rows, activeHostId]);
}
