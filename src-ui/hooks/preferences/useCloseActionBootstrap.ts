// 启动时把 app-settings.json 中的 closeAction 同步到 preferencesStore，
// 使标题栏关闭行为与磁盘一致（设置页保存会同时写两边）。

import { useEffect } from 'react';
import { settingsService } from '../../core/services/settings.service';
import { preferencesStore } from '../preferences/preferencesStore';

export function useCloseActionBootstrap(): void {
    useEffect(() => {
        void settingsService.get().then((s) => {
            preferencesStore.applySnapshot({ closeAction: s.closeAction });
        });
    }, []);
}