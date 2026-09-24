// 概览：左边状态卡回答「它现在能不能在 QQ 上回话、还差什么」，右边是当前设置的摘要，点一行就去那一页。
// 「还差什么」全页只在状态卡里说一次，别的页最多在侧栏亮个点。能在原地补的就在原地补，不让人先跳页。

import type { ReactNode } from 'react';
import { AlertTriangle, Check, ChevronDown, ChevronRight, ExternalLink, Link2, MessageSquare, Play } from 'lucide-react';
import { Button, Card, Select, Spinner } from '../../../../shared/ui';
import {
    astrbotConfigWarnings,
    astrbotSetup,
    astrbotWakeHint,
    enabledChatModels,
    presetLabel,
} from '../../../../core/domain/apps/astrbotConfig';
import { ProviderDialog } from './ProviderDialog';
import { ProviderPresetMenu, useProviderEditor } from './providerEditor';
import { ASTRBOT_TAB_LABEL } from './astrbotNav';
import { JumpLink } from './parts';
import { cn } from '../../../../shared/utils/cn';
import type { AppInstance, AstrBotAiSettings, AstrBotDashboardStatus, AstrBotInstanceConfig } from '../../../../core/ipc/types';

type Step = {
    key: string;
    done: boolean;
    title: string;
    desc: ReactNode;
    /** primary = 这是眼下该做的那一步 */
    action?: (primary: boolean) => ReactNode;
};

type Notice = { key: string; text: string; tab: string; action: string };

export const AstrBotOverviewTab: React.FC<{
    instance: AppInstance;
    config: AstrBotInstanceConfig;
    /** 最近一次保存成功的版本：草稿就绪但还没保存时，状态卡要说「保存后」 */
    saved: AstrBotInstanceConfig | null;
    onChange: (next: AstrBotInstanceConfig) => void;
    errors: Record<string, string>;
    disabled?: boolean;
    dash: AstrBotDashboardStatus | undefined;
    onGoTab: (tab: string) => void;
    onOpenLink: () => void;
    onStart: () => void;
    starting: boolean;
    onOpenWebUi: (path?: string) => void;
}> = ({ instance, config, saved, onChange, errors, disabled, dash, onGoTab, onOpenLink, onStart, starting, onOpenWebUi }) => {
    const running = instance.state === 'running';
    const linked = !!instance.link;
    const setup = astrbotSetup(config, linked);
    const savedReady = !!saved && astrbotSetup(saved, linked).ready;
    const wake = astrbotWakeHint(config);
    const editor = useProviderEditor(config, onChange);
    const savedIds = new Set((saved?.sources ?? []).map((s) => s.id));
    const setAi = (patch: Partial<AstrBotAiSettings>) => onChange({ ...config, ai: { ...config.ai, ...patch } });

    const chat = enabledChatModels(config);
    const chatSource = setup.chatSourceIndex >= 0 ? config.sources[setup.chatSourceIndex] : undefined;
    const current = chat.find((m) => m.id === config.ai.default_provider_id);
    const currentSource = current && config.sources.find((s) => s.id === current.provider_source_id);

    const linkStep: Step = {
        key: 'link',
        done: setup.linkDone,
        title: '连上 QQ',
        desc: instance.link
            ? `已对接 ${instance.link.bot_id}`
            : setup.linkDone
              ? `没对接 QQ，走的是 ${config.other_platforms.join('、')}`
              : '对接 NapCat / SnowLuma 后，QQ 消息才会转给它',
        action: setup.linkDone
            ? undefined
            : (primary) => (
                  <Button size="sm" variant={primary ? 'primary' : 'secondary'} onClick={onOpenLink}>
                      <Link2 size={13} />
                      对接
                  </Button>
              ),
    };

    const llmStep: Step = { key: 'llm', done: setup.llmIssue === null, title: '接入大模型', desc: '' };
    switch (setup.llmIssue) {
        case 'no_source':
            llmStep.desc = '选一家提供商，填上 API Key';
            llmStep.action = (primary) => (
                <ProviderPresetMenu onPick={editor.openCreate}>
                    <Button size="sm" variant={primary ? 'primary' : 'secondary'} disabled={disabled}>
                        添加提供商
                        <ChevronDown size={12} className="-mr-0.5 opacity-80" />
                    </Button>
                </ProviderPresetMenu>
            );
            break;
        case 'no_model':
            llmStep.desc = chatSource?.enable
                ? `「${chatSource.id}」下还没有启用的对话模型`
                : `提供商「${chatSource?.id}」停用了`;
            llmStep.action = (primary) => (
                <Button
                    size="sm"
                    variant={primary ? 'primary' : 'secondary'}
                    disabled={disabled}
                    onClick={() => editor.openEdit(setup.chatSourceIndex)}
                >
                    {chatSource?.enable ? '加模型' : '去启用'}
                </Button>
            );
            break;
        case 'no_default':
            llmStep.desc = '选一个模型来回复';
            llmStep.action = () => (
                <Select
                    value={undefined}
                    items={chat.map((m) => ({ value: m.id, label: m.model || m.id }))}
                    placeholder="选择模型"
                    disabled={disabled}
                    className="w-44"
                    onValueChange={(default_provider_id) => setAi({ default_provider_id })}
                />
            );
            break;
        case 'llm_off':
            llmStep.desc = '大模型回复关着，现在只回指令和插件';
            llmStep.action = (primary) => (
                <Button size="sm" variant={primary ? 'primary' : 'secondary'} disabled={disabled} onClick={() => setAi({ enable: true })}>
                    打开
                </Button>
            );
            break;
        default:
            llmStep.desc = [current?.model || current?.id, currentSource && (presetLabel(currentSource) ?? currentSource.id)]
                .filter(Boolean)
                .join(' · ');
    }

    const steps = [linkStep, llmStep];
    const firstOpen = steps.findIndex((s) => !s.done);
    const left = steps.filter((s) => !s.done).length;

    const notices: Notice[] = astrbotConfigWarnings(config).map((w) => ({
        key: w.key,
        text: w.text,
        tab: w.area,
        action: `去「${ASTRBOT_TAB_LABEL[w.area]}」`,
    }));
    if (running && dash?.gate === 'auth') {
        notices.unshift({ key: 'dash-auth', text: '控制台没登上，人格、知识库、会话规则暂时管不了', tab: 'connections', action: '去填 WebUI 密码' });
    } else if (running && dash?.gate === 'unreachable') {
        notices.unshift({ key: 'dash-down', text: '连不上 AstrBot 控制台，人格、知识库、会话规则暂时管不了', tab: 'log', action: '看日志' });
    }

    const checklist = (
        <div>
            <div className="mb-2 flex items-baseline justify-between gap-3">
                <h2 className="text-[15px] font-semibold text-text">让它在 QQ 上开口</h2>
                <span className="shrink-0 text-xs text-text-tertiary">还差 {left} 步</span>
            </div>
            <ol className="flex flex-col gap-1">
                {steps.map((s, i) => (
                    // 停着的时候「启动」才是主按钮，清单里的动作一律降成次要
                    <StepRow key={s.key} index={i + 1} step={s} current={i === firstOpen} emphasize={running} />
                ))}
            </ol>
            <p className="mx-2.5 mt-2 border-t border-dashed border-border pt-2.5 text-xs text-text-tertiary">
                两步都完成后，{wake.sentence}
            </p>
        </div>
    );

    return (
        <div className="grid grid-cols-1 items-start gap-6 lg:grid-cols-[minmax(0,1.15fr)_minmax(0,1fr)]">
            <Card variant="outlined" padding="none" className="p-4">
                {!running ? (
                    <>
                        <h2 className="text-[15px] font-semibold text-text-secondary">实例没在运行</h2>
                        <p className="mt-1 text-[13px] leading-relaxed text-text-secondary">
                            配置照常可以改，启动后生效。人格、知识库、会话规则要实例运行时才能管理。
                        </p>
                        <Button size="sm" variant="primary" className="mt-3" disabled={starting} onClick={onStart}>
                            {starting ? <Spinner size="xs" className="text-white" /> : <Play size={13} />}
                            启动
                        </Button>
                        {!setup.ready && <div className="mt-4 border-t border-border-subtle pt-4">{checklist}</div>}
                    </>
                ) : !setup.ready ? (
                    checklist
                ) : (
                    <>
                        <h2 className="flex items-center gap-2.5 text-[15px] font-semibold text-text">
                            <span className="h-2 w-2 shrink-0 rounded-full bg-success ring-4 ring-success/15" aria-hidden />
                            {savedReady ? '可以对话了' : '保存后就能对话了'}
                        </h2>
                        <p className="mt-1.5 text-[13px] leading-relaxed text-text-secondary">{wake.sentence}</p>
                        {savedReady && (
                            <div className="mt-3 flex flex-wrap gap-2">
                                <Button size="sm" variant="primary" onClick={() => onOpenWebUi('/chat')}>
                                    <MessageSquare size={13} />
                                    网页试聊
                                </Button>
                                <Button size="sm" variant="secondary" onClick={() => onOpenWebUi()}>
                                    <ExternalLink size={13} />
                                    打开 WebUI
                                </Button>
                            </div>
                        )}
                    </>
                )}
                {notices.length > 0 && (
                    <ul className="mt-4 flex flex-col gap-1.5">
                        {notices.map((n) => (
                            <li
                                key={n.key}
                                className="flex items-center gap-2.5 rounded-md bg-warning-soft/60 px-3 py-2 text-[12.5px] text-text-secondary"
                            >
                                <AlertTriangle size={13} className="shrink-0 text-warning" />
                                <span className="min-w-0 flex-1">{n.text}</span>
                                <JumpLink tab={n.tab} onGo={onGoTab} className="shrink-0 text-xs">
                                    {n.action}
                                </JumpLink>
                            </li>
                        ))}
                    </ul>
                )}
            </Card>

            <SettingsSummary config={config} wakeShort={wake.short} onGoTab={onGoTab} />

            {editor.draft && (
                <ProviderDialog
                    draft={editor.draft}
                    config={config}
                    savedIds={savedIds}
                    running={running}
                    instanceId={instance.id}
                    errors={errors}
                    onChange={editor.setDraft}
                    onCancel={editor.cancel}
                    onConfirm={editor.confirm}
                />
            )}
        </div>
    );
};

const StepRow: React.FC<{ index: number; step: Step; current: boolean; emphasize: boolean }> = ({
    index,
    step,
    current,
    emphasize,
}) => (
    <li className={cn('flex items-center gap-3 rounded-md px-2.5 py-2.5', current && 'bg-brand-tint')}>
        <span
            className={cn(
                'flex h-5 w-5 shrink-0 items-center justify-center rounded-full border text-2xs font-semibold',
                step.done && 'border-success bg-success text-white',
                !step.done && current && 'border-brand text-brand',
                !step.done && !current && 'border-border text-text-tertiary',
            )}
        >
            {step.done ? <Check size={11} strokeWidth={3} /> : index}
        </span>
        <div className="min-w-0 flex-1">
            <p className={cn('text-[13px]', step.done ? 'text-text-secondary' : 'font-medium text-text')}>{step.title}</p>
            <p className="truncate text-xs text-text-tertiary">{step.desc}</p>
        </div>
        {step.action && <div className="shrink-0">{step.action(current && emphasize)}</div>}
    </li>
);

type SummaryRow = { label: string; value: string | null; empty: string; tab: string };

const SettingsSummary: React.FC<{
    config: AstrBotInstanceConfig;
    wakeShort: string;
    onGoTab: (tab: string) => void;
}> = ({ config, wakeShort, onGoTab }) => {
    const model = enabledChatModels(config).find((m) => m.id === config.ai.default_provider_id);
    const g = config.gates;
    const sub = config.subagent;
    const rows: SummaryRow[] = [
        { label: '对话模型', value: model ? model.model || model.id : null, empty: '还没有', tab: 'models' },
        { label: '人格', value: config.ai.default_personality || null, empty: '内置', tab: 'persona' },
        { label: '唤醒方式', value: wakeShort, empty: '', tab: 'talk' },
        {
            label: '回复范围',
            // 名单为空时上游不做检查，开着开关也是所有会话
            value: g.enable_id_white_list && g.id_whitelist.length ? `白名单 ${g.id_whitelist.length} 个会话` : '所有会话',
            empty: '',
            tab: 'talk',
        },
        { label: '知识库', value: config.kb.names.length ? config.kb.names.join('、') : null, empty: '未挂载', tab: 'kb' },
        { label: '子代理', value: sub.main_enable ? `${sub.agents.length} 个` : null, empty: '没开', tab: 'subagent' },
    ];
    return (
        <section>
            <h3 className="px-2 pb-1 text-xs font-medium text-text-tertiary">现在的设置</h3>
            <div className="flex flex-col">
                {rows.map((r) => (
                    <button
                        key={r.label}
                        type="button"
                        title={`去「${ASTRBOT_TAB_LABEL[r.tab]}」`}
                        onClick={() => onGoTab(r.tab)}
                        className={cn(
                            'group flex w-full items-center gap-3 rounded-sm border-b border-border-subtle px-2 py-2.5 text-left last:border-b-0',
                            'transition-colors hover:bg-inset/60',
                            'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-brand',
                        )}
                    >
                        <span className="w-16 shrink-0 text-[12.5px] text-text-tertiary">{r.label}</span>
                        <span
                            className={cn(
                                'min-w-0 flex-1 truncate text-right text-[13px]',
                                r.value ? 'text-text' : 'text-text-disabled',
                            )}
                        >
                            {r.value ?? r.empty}
                        </span>
                        <ChevronRight
                            size={13}
                            className="shrink-0 text-text-disabled transition-colors group-hover:text-text-secondary"
                        />
                    </button>
                ))}
            </div>
        </section>
    );
};
