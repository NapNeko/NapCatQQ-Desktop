// 组件页：清单加载失败、单点探测失败 → 全局 InfoBar；卡片只保留状态徽章，不堆长错误文案。
//
// 挂载：ComponentsPage 顶层（与 useComponentActionErrors 并列）。

import { useEffect, useRef } from 'react';
import type { ComponentRow } from '../../core/domain/components/types';
import { globalInfoBarStore } from '../ui/globalInfoBarStore';

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

/**
 * catalogError：整页清单拉取失败时推一条 danger InfoBar（顶替同 key）。
 * rows + activeHostId：仅当前选中主机的探测失败推 InfoBar；切主机时撤掉其它主机的条。
 */
export function useComponentPageAlerts(
    rows: ComponentRow[],
    catalogError: Error | null,
    activeHostId: string | null,
): void {
    const lastCatalogMessageRef = useRef<string | null>(null);

    useEffect(() => {
        if (!catalogError) {
            if (lastCatalogMessageRef.current != null) {
                globalInfoBarStore.dismiss(`key:${CATALOG_KEY}`);
                lastCatalogMessageRef.current = null;
            }
            return;
        }
        const message = catalogError.message || '加载组件清单失败';
        lastCatalogMessageRef.current = message;
        globalInfoBarStore.push({
            key: CATALOG_KEY,
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
                    globalInfoBarStore.dismiss(`key:${key}`);
                    continue;
                }
                const { status } = hostRow;
                if (
                    status.state === 'unknown' &&
                    isDetectFailureReason(status.reason)
                ) {
                    const heading = `${row.info.display_name} · ${hostRow.host.display_name}`;
                    globalInfoBarStore.push({
                        key,
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
                    globalInfoBarStore.dismiss(`key:${key}`);
                }
            }
        }
    }, [rows, activeHostId]);
}