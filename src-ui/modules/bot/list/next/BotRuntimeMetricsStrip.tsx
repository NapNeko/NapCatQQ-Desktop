// 列表卡底部可选：实例运行时指标摘要（开关开启且有数据时）

import { useEffect, useState } from 'react';
import { botService } from '../../../../core/services/bot.service';
import { settingsService } from '../../../../core/services/settings.service';
import type { BotRuntimeMetrics } from '../../../../core/ipc/generated/domain/BotRuntimeMetrics';
import { formatBytes } from '../../../../core/domain/bot/runtime-metrics-settings';

export function BotRuntimeMetricsStrip({ botId }: { botId: string }) {
    const [enabled, setEnabled] = useState(false);
    const [intervalMs, setIntervalMs] = useState(3000);
    const [metrics, setMetrics] = useState<BotRuntimeMetrics | null>(null);

    useEffect(() => {
        let cancelled = false;
        void settingsService.get().then((s) => {
            if (cancelled) return;
            setEnabled(!!s.botRuntimeMetricsEnabled);
            setIntervalMs(s.botRuntimeMetricsIntervalMs || 3000);
        });
        return () => {
            cancelled = true;
        };
    }, []);

    useEffect(() => {
        if (!enabled) {
            setMetrics(null);
            return;
        }
        let cancelled = false;
        const load = async () => {
            try {
                const m = await botService.getRuntimeMetrics(botId);
                if (!cancelled) setMetrics(m);
            } catch {
                if (!cancelled) setMetrics(null);
            }
        };
        void load();
        const t = window.setInterval(
            () => void load(),
            Math.max(1000, intervalMs || 3000),
        );
        return () => {
            cancelled = true;
            window.clearInterval(t);
        };
    }, [botId, enabled, intervalMs]);

    if (!enabled || !metrics) return null;
    if (metrics.probe === 'not_injected' || metrics.probe === 'error') {
        return (
            <p className="mt-1 text-[11px] text-text-tertiary">
                指标：
                {metrics.probe === 'error'
                    ? metrics.probe_error || '探针错误'
                    : '未注入（重启实例后生效）'}
            </p>
        );
    }

    const rss = metrics.memory?.rss_bytes;
    const events = metrics.nodes.reduce(
        (s, n) => s + Number(n.events_out ?? 0),
        0,
    );
    const actions = metrics.nodes.reduce(
        (s, n) => s + Number(n.actions_in ?? 0),
        0,
    );

    return (
        <p className="mt-1 font-mono text-[11px] text-text-secondary">
            内存 {formatBytes(rss != null ? Number(rss) : null)}
            {' · '}
            出站事件 {events}
            {' · '}
            入站 action {actions}
            {metrics.probe === 'stale' ? ' · 数据陈旧' : ''}
        </p>
    );
}
