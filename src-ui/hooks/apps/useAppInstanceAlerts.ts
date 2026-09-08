// 实例 last_error / 列表拉取失败 → 全局 InfoBar；卡片和页头不堆原文。

import { useEffect } from 'react';
import type { AppInstance } from '../../core/ipc/types';
import { dismissInfoBar } from '../ui/globalInfoBarStore';
import { pushAppErrorBar } from './pushAppErrorBar';

const prevLastError = new Map<string, string>();

function norm(v: string | null | undefined): string | null {
    const t = v?.trim();
    return t && t.length > 0 ? t : null;
}

export function useAppInstanceAlerts(instances: AppInstance[], listError: string | null): void {
    useEffect(() => {
        if (!listError) {
            dismissInfoBar('key:app-list-load');
            return;
        }
        pushAppErrorBar({
            key: 'app-list-load',
            title: '读取实例失败',
            raw: listError,
        });
    }, [listError]);

    useEffect(() => {
        const nextIds = new Set<string>();
        for (const inst of instances) {
            nextIds.add(inst.id);
            const err = norm(inst.last_error);
            const key = `app-last-error:${inst.id}`;
            if (!err) {
                prevLastError.delete(inst.id);
                dismissInfoBar(`key:${key}`);
                continue;
            }
            if (prevLastError.get(inst.id) === err) continue;
            prevLastError.set(inst.id, err);
            pushAppErrorBar({
                key,
                title: `${inst.display_name} 出错`,
                raw: err,
            });
        }
        for (const id of [...prevLastError.keys()]) {
            if (!nextIds.has(id)) {
                prevLastError.delete(id);
                dismissInfoBar(`key:app-last-error:${id}`);
            }
        }
    }, [instances]);
}
