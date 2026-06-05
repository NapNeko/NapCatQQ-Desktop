import { useEffect } from 'react';
import { preferencesStore } from '../preferences/preferencesStore';
import { trayService } from '../../core/services/desktop.service';

/** 启动时把磁盘 app-settings 里的 closeAction 同步到 localStorage 偏好。 */
export function useTrayCloseActionSync(): void {
    useEffect(() => {
        if (typeof window === 'undefined') return;
        void trayService.syncCloseActionFromDisk().then((action) => {
            preferencesStore.setCloseAction(action);
        });
    }, []);
}