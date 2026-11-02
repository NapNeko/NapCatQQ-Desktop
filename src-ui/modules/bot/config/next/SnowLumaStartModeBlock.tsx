import { Flame, Snowflake, type LucideIcon } from 'lucide-react';
import { ExpandPresence } from '../../../../shared/ui/motion/ExpandPresence';
import type { BotBasicConfig } from '../../../../core/ipc/generated/domain/BotBasicConfig';
import type { SnowLumaStartMode } from '../../../../core/ipc/generated/domain/SnowLumaStartMode';
import { IdentityChoiceCard } from './IdentityChoiceCard';
import { RuntimeNotice } from './RuntimeNotice';

const SL_MODE_ITEMS: ReadonlyArray<{
    value: 'cold_start' | 'hot_start';
    label: string;
    summary: string;
    icon: LucideIcon;
}> = [
    {
        value: 'cold_start',
        label: '冷启动（默认）',
        summary: '由桌面端拉起并托管 QQ 进程',
        icon: Snowflake,
    },
    {
        value: 'hot_start',
        label: '热启动',
        summary: '附加到已登录此号的 QQ 进程',
        icon: Flame,
    },
];

export function SnowLumaStartModeBlock({
    data,
    onChange,
}: {
    data: BotBasicConfig;
    onChange: (patch: Partial<BotBasicConfig>) => void;
}) {
    const mode: 'cold_start' | 'hot_start' =
        data.snowlumaStartMode?.mode === 'hot_start' ? 'hot_start' : 'cold_start';

    const setMode = (m: 'cold_start' | 'hot_start') => {
        const value: SnowLumaStartMode =
            m === 'cold_start' ? { mode: 'cold_start' } : { mode: 'hot_start' };
        onChange({ snowlumaStartMode: value });
    };

    return (
        <div className="flex flex-col">
            <div
                role="radiogroup"
                aria-label="SnowLuma 启动模式"
                className="grid grid-cols-1 gap-3 sm:grid-cols-2"
            >
                {SL_MODE_ITEMS.map((item) => (
                    <IdentityChoiceCard
                        key={item.value}
                        selected={mode === item.value}
                        onSelect={() => setMode(item.value)}
                        label={item.label}
                        summary={item.summary}
                        icon={item.icon}
                    />
                ))}
            </div>
            <ExpandPresence visible={mode === 'hot_start'}>
                <div className="pt-2.5">
                    <RuntimeNotice tone="neutral">
                        启动前需在此 QQ 号（
                        <span className="font-mono">{data.QQID || '未填'}</span>
                        ）下登录 QQ；找不到匹配进程时启动会失败
                    </RuntimeNotice>
                </div>
            </ExpandPresence>
        </div>
    );
}
