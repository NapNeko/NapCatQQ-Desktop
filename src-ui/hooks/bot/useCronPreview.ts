// 定时重启 cron 的即时校验 + 预览：输入停顿后问后端要接下来几次触发时刻。
// 后端解析失败 → error（红字）；浏览器预览环境后端不可用返回空列表 → ok 但无文案。

import { useEffect, useState } from 'react';
import { botService } from '../../core/services/bot.service';
import { formatCronPreview } from '../../core/domain/bot/auto-restart';

export type CronPreviewState =
    | { status: 'idle' }
    | { status: 'ok'; text: string | null }
    | { status: 'error'; message: string };

const DEBOUNCE_MS = 350;

export function useCronPreview(expr: string): CronPreviewState {
    const [state, setState] = useState<CronPreviewState>({ status: 'idle' });

    useEffect(() => {
        const trimmed = expr.trim();
        if (!trimmed) {
            setState({ status: 'idle' });
            return;
        }
        let cancelled = false;
        const timer = setTimeout(() => {
            botService
                .previewAutoRestartCron(trimmed)
                .then((times) => {
                    if (!cancelled) setState({ status: 'ok', text: formatCronPreview(times) });
                })
                .catch((err: unknown) => {
                    if (!cancelled) setState({ status: 'error', message: String(err) });
                });
        }, DEBOUNCE_MS);
        return () => {
            cancelled = true;
            clearTimeout(timer);
        };
    }, [expr]);

    return state;
}
