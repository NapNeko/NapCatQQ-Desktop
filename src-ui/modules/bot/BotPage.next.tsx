// Bot 业务区路由壳（next）。
//
// 跟旧 BotPage.tsx 对应：3 视图 (list / config / log) 的浅路由切换。
// Step 7 完成 list；Step 8 (本次) 完成 config 推倒重写。log 仍走旧 Fluent 树等 Step 9。
//
// 切到 list 时把 selectedBotId 清掉，避免下次进 config 复用陈旧 ID。

import { useState } from 'react';
import { Button } from '../../shared/ui';
import { BotListPageNext } from './list/BotListPage.next';
import { BotConfigPageNext } from './config/BotConfigPage.next';
import { BotLogPageNext } from './log/BotLogPage.next';

type View = 'list' | 'config' | 'log';

export function BotPageNext() {
    const [view, setView] = useState<View>('list');
    const [selectedBotId, setSelectedBotId] = useState<string | null>(null);

    const goList = () => {
        setSelectedBotId(null);
        setView('list');
    };

    return (
        <div className="flex h-full w-full flex-col">
            {view === 'list' && (
                <BotListPageNext
                    onConfigureBot={(botId) => {
                        setSelectedBotId(botId);
                        setView('config');
                    }}
                    onViewLogs={(botId) => {
                        setSelectedBotId(botId);
                        setView('log');
                    }}
                />
            )}
            {view === 'config' && (
                <BotConfigPageNext
                    botId={selectedBotId}
                    onBack={goList}
                    onSavedStay={(savedBotId) => setSelectedBotId(savedBotId)}
                />
            )}
            {view === 'log' &&
                (selectedBotId ? (
                    <BotLogPageNext botId={selectedBotId} onBack={goList} />
                ) : (
                    <div className="flex flex-col items-center gap-3 rounded-md bg-elevated p-6 ring-1 ring-border-subtle">
                        <p className="text-sm text-text-secondary">未选择要查看日志的 Bot 实例</p>
                        <Button size="sm" variant="ghost" onClick={goList}>
                            返回实例列表
                        </Button>
                    </div>
                ))}
        </div>
    );
}

export default BotPageNext;
