// 「机器人到底会不会回话」的体检条。四步有先后：没有源就没有模型，没有模型就选不了默认。
// 每一步都能点过去修，不再只是一行说明。

import { AlertTriangle, ArrowRight, Check } from 'lucide-react';
import { Badge, Card } from '../../../../shared/ui';
import { astrbotReadiness } from '../../../../core/domain/apps/astrbotConfig';
import { cn } from '../../../../shared/utils/cn';
import { JumpLink } from './parts';
import type { AstrBotInstanceConfig } from '../../../../core/ipc/types';

type Step = { label: string; done: boolean; tab: string; fix: string };

export const AstrBotReadyBar: React.FC<{
    config: AstrBotInstanceConfig;
    onGoTab: (tab: string) => void;
}> = ({ config, onGoTab }) => {
    const r = astrbotReadiness(config);
    const steps: Step[] = [
        { label: '模型提供商', done: r.hasSource, tab: 'models', fix: '加一个提供商' },
        { label: '对话模型', done: r.hasEnabledChat, tab: 'models', fix: '给提供商加一个启用的模型' },
        { label: '默认模型', done: r.defaultOk, tab: 'models', fix: '在模型行上点「设为默认」' },
        { label: '总开关', done: r.llmOn, tab: 'talk', fix: '打开「对话」的启用开关' },
    ];
    const missing = steps.filter((s) => !s.done).length;
    const next = steps.find((s) => !s.done);
    const defaultModel = config.models.find((m) => m.id === config.ai.default_provider_id);

    return (
        <Card variant="outlined" padding="none" className="overflow-hidden">
            <div className="flex flex-wrap items-center justify-between gap-x-4 gap-y-2 px-4 py-3">
                <div className="flex min-w-0 items-center gap-2.5">
                    <Badge tone={missing ? 'warning' : 'success'} dot={!missing}>
                        {missing ? `还差 ${missing} 步` : '可以对话'}
                    </Badge>
                    <p className="truncate text-[13px] text-text-secondary">
                        {next ? next.fix : `当前模型 ${defaultModel?.model || config.ai.default_provider_id}`}
                    </p>
                </div>
                {next && (
                    <JumpLink tab={next.tab} onGo={onGoTab} className="text-xs">
                        去补上
                    </JumpLink>
                )}
            </div>

            <div className="flex flex-wrap items-center gap-y-2 border-t border-border-subtle bg-inset/40 px-4 py-2.5">
                {steps.map((s, i) => {
                    const isNext = s === next;
                    return (
                        <div key={s.label} className="flex items-center">
                            {i > 0 && <span className="mx-2 h-px w-4 bg-border-subtle" aria-hidden />}
                            <button
                                type="button"
                                onClick={() => onGoTab(s.tab)}
                                title={s.done ? `已就绪，去「${s.tab === 'talk' ? '对话' : '模型'}」页` : s.fix}
                                className={cn(
                                    'group inline-flex items-center gap-1.5 rounded-sm py-0.5 pl-0.5 pr-1.5 transition-colors',
                                    'hover:bg-surface focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand/40',
                                )}
                            >
                                <span
                                    className={cn(
                                        'flex h-4 w-4 shrink-0 items-center justify-center rounded-full',
                                        s.done && 'bg-success text-white',
                                        !s.done && isNext && 'border border-brand bg-brand-soft',
                                        !s.done && !isNext && 'border border-border',
                                    )}
                                >
                                    {s.done && <Check size={10} strokeWidth={3} />}
                                </span>
                                <span
                                    className={cn(
                                        'text-xs',
                                        s.done && 'text-text-secondary',
                                        !s.done && isNext && 'font-medium text-text',
                                        !s.done && !isNext && 'text-text-disabled',
                                    )}
                                >
                                    {s.label}
                                </span>
                                <ArrowRight
                                    size={11}
                                    className="text-text-disabled opacity-0 transition-opacity group-hover:opacity-100"
                                />
                            </button>
                        </div>
                    );
                })}
            </div>

            {r.whitelistMayBlock && (
                <div className="flex items-center gap-2 border-t border-warning/20 bg-warning-soft/50 px-4 py-2.5">
                    <AlertTriangle size={14} className="shrink-0 text-warning" />
                    <p className="min-w-0 flex-1 text-xs text-text-secondary">
                        白名单开着但名单是空的，现在谁都不会得到回复
                    </p>
                    <JumpLink tab="talk" onGo={onGoTab} className="text-xs">
                        去改
                    </JumpLink>
                </div>
            )}
        </Card>
    );
};
