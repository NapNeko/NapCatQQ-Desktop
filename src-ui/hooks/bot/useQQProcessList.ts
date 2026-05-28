// SnowLuma HotStart 模式下选择目标 QQ.exe 的 picker hook。
//
// 流程：
//   1. listQQProcesses 拉所有主 QQ.exe → 立即 setState 显示
//   2. 并发对每个 PID 调 probeQQLoginInfo（QQ NT 在 127.0.0.1:9210-9219 的
//      tencent:// HTTP 端点 + JWT 解码）拿到当前登录的 uin / nickName
//   3. 每条探测结果到达就增量更新对应行的 login 字段，UI 渐进显示
//
// 为什么不在后端直接返回带登录信息的列表：单 PID 探测最坏 10s（10 端口 × 1s
// timeout），如果在后端串行枚举会让 dialog 长时间空白；前端并发触发 + 增量
// 渲染体验最好。
//
// 探测出错（端口都不响应 / 未登录）的行 `loginProbed=true, loginUin=''`，
// UI 可以据此显示"未登录"灰字。

import { useCallback, useState } from 'react';
import { botService, type QQProcessInfo } from '../../core/services/bot.service';

export type { QQProcessInfo };

/// 给 PID picker 用的合并视图：原始进程信息 + 异步探测结果。
export interface QQProcessRow extends QQProcessInfo {
    /// 是否已完成探测（区分"探测中"与"探测了但未登录"）。
    loginProbed: boolean;
    /// 当前登录 uin，空串表示未登录或未探测到。
    loginUin: string;
    /// 昵称，可能为空。
    loginNickname: string;
}

export function useQQProcessList() {
    const [processes, setProcesses] = useState<QQProcessRow[]>([]);
    const [isLoading, setIsLoading] = useState(false);
    const [error, setError] = useState<string | null>(null);

    const load = useCallback(async (): Promise<void> => {
        setIsLoading(true);
        setError(null);
        setProcesses([]);
        try {
            const list = await botService.listQQProcesses();
            // 先把所有 row 放上去，loginProbed 全部 false（UI 显示"探测中"）
            const initialRows: QQProcessRow[] = list.map((p) => ({
                ...p,
                loginProbed: false,
                loginUin: '',
                loginNickname: '',
            }));
            setProcesses(initialRows);
            setIsLoading(false);

            // 并发探测，每个完成后增量更新对应行
            await Promise.allSettled(
                list.map(async (p) => {
                    let info;
                    try {
                        info = await botService.probeQQLoginInfo(p.pid);
                    } catch {
                        info = null;
                    }
                    setProcesses((prev) =>
                        prev.map((row) =>
                            row.pid === p.pid
                                ? {
                                    ...row,
                                    loginProbed: true,
                                    loginUin: info?.uin ?? '',
                                    loginNickname: info?.nickname ?? '',
                                }
                                : row,
                        ),
                    );
                }),
            );
        } catch (err) {
            setError(`列出 QQ 进程失败: ${String(err)}`);
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
