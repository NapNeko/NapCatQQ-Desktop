// Desktop 自更新 IPC 服务。
// 命令名字符串只在此处持有（R3 单一字面量来源）。
// 安装成功后后端会 exit(0)，前端应把失败/取消展示给用户。

import { invoke, isTauri } from '../ipc/transport';
import type { AvailableUpdate } from '../ipc/generated/update/AvailableUpdate';
import type { PrecheckReport } from '../ipc/generated/update/PrecheckReport';
import { withMockDelay } from '../ipc/mock/bootstrap.mock';
import { APP_VERSION } from '../domain/app-meta';

export type { AvailableUpdate, PrecheckReport };

export const desktopUpdateService = {
    check: async (): Promise<AvailableUpdate | null> => {
        if (isTauri) {
            return invoke<AvailableUpdate | null>('check_desktop_update');
        }
        // 浏览器预览：假装没有更新
        return withMockDelay(null, 120);
    },

    precheck: async (update: AvailableUpdate): Promise<PrecheckReport> => {
        if (isTauri) {
            return invoke<PrecheckReport>('precheck_desktop_update', { update });
        }
        return withMockDelay(
            {
                v: 1,
                can_upgrade: true,
                blocking: [],
                warnings: [],
                estimated_migration_time_ms: BigInt(0),
            } satisfies PrecheckReport,
            50,
        );
    },

    /**
     * 下载 MSI 并启动 msiexec；成功时进程会退出，Promise 可能永不 resolve。
     * `expected` 只带 UI 看到的版本；后端会重新 check，不信任本地下发的 URL。
     */
    install: async (expected: AvailableUpdate): Promise<void> => {
        if (isTauri) {
            return invoke<void>('install_desktop_update', { expected });
        }
        console.info(
            '[desktopUpdateService] mock install',
            expected.version,
            'from',
            APP_VERSION,
        );
        return withMockDelay(undefined, 200);
    },
};
