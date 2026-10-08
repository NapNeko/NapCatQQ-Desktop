// Desktop 自更新的 React 入口：消费后端存的上次更新结果。
// 检查 / 安装不在这里 —— 手动检查走 hooks/desktop/useDesktopUpdateCheck，
// 安装走组件动作 desktop_self，本 hook 只管启动提示条。

import { useEffect } from 'react';
import { desktopUpdateService } from '../../core/services/desktop-update.service';
import { pushInfoBar } from '../ui/globalInfoBarStore';

/** 自更新结果：resume / 失败日志消费一次（#126 装完无反馈） */
export function useDesktopUpdateStartupNotice(): void {
    useEffect(() => {
        let cancelled = false;
        void (async () => {
            try {
                const notice = await desktopUpdateService.consumeStartupNotice();
                if (cancelled || !notice) return;
                // wire: serde rename_all = snake_case → "success" | "incomplete" | "failure"
                const tone =
                    notice.kind === 'success'
                        ? 'success'
                        : notice.kind === 'failure'
                          ? 'danger'
                          : 'warning';
                const title =
                    notice.kind === 'success'
                        ? '更新完成'
                        : notice.kind === 'failure'
                          ? '上次更新失败'
                          : '更新可能未完成';
                pushInfoBar({
                    key: 'desktop-update-startup',
                    tone,
                    title,
                    content: notice.message,
                    autoDismissMs: notice.kind === 'success' ? 6000 : 12000,
                });
            } catch {
                // 启动提示失败不挡主流程
            }
        })();
        return () => {
            cancelled = true;
        };
    }, []);
}
