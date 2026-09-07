// Bot 日志页。数据来自 useBotLogStream，面板复用 LogConsole。

import { ArrowLeft } from 'lucide-react';
import { Badge, Button } from '../../../shared/ui';
import { ActionMotionIcon } from '../../../shared/ui/motion';
import { LogConsole } from '../../../shared/log/LogConsole';
import { useBotLogStream } from '../../../hooks/bot/useBotLogStream';

interface BotLogPageNextProps {
    botId: string;
    onBack: () => void;
}

export function BotLogPageNext({ botId, onBack }: BotLogPageNextProps) {
    const { logs, clear } = useBotLogStream(botId);

    return (
        <div className="flex h-full min-h-0 flex-col gap-3">
            <div className="flex items-center gap-2">
                <Button variant="ghost" size="icon" onClick={onBack} aria-label="返回">
                    <ActionMotionIcon icon={ArrowLeft} size={16} />
                </Button>
                <h2 className="text-[15px] font-semibold leading-none text-text">
                    实例 {botId} 运行日志
                </h2>
                <Badge tone="neutral" appearance="soft">
                    {logs.length} 行
                </Badge>
            </div>
            <LogConsole logs={logs} onClear={clear} />
        </div>
    );
}

export default BotLogPageNext;
