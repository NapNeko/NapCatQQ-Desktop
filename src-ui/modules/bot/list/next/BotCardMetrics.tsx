// 高频指标订阅限定在这一行，轮询不重渲染整张卡和其对话框。
import { useBotRuntimeMetrics } from '../../../../hooks/bot/useBotRuntimeMetrics';
import { BotRuntimeMetricsStrip } from './BotRuntimeMetricsStrip';

export function BotCardMetrics({
    botId,
    onViewMetrics,
}: {
    botId: string;
    onViewMetrics: (botId: string) => void;
}) {
    const { enabled, metrics } = useBotRuntimeMetrics(botId);
    return (
        <BotRuntimeMetricsStrip
            enabled={enabled}
            metrics={metrics}
            onOpenDetail={() => onViewMetrics(botId)}
        />
    );
}
