// 概览：顶部一张状态卡回答「它现在能不能在 QQ 上回话、还差什么」，全卡只给一个主按钮；
// 下面是当前设置的卡片，点一张去对应页。和首页同一套语言：hero 大卡 + 带图标方块的卡片。
// 「还差什么」全页只在状态卡里说一次，别的页最多在侧栏亮个点。

import type { ComponentType, ReactNode } from 'react';
import type { LucideProps } from 'lucide-react';
import {
    AlertTriangle,
    AtSign,
    Bot,
    Boxes,
    CheckCircle2,
    ChevronDown,
    ChevronRight,
    Circle,
    ExternalLink,
    Library,
    Link2,
    MessageSquare,
    Play,
    ShieldCheck,
    UserRound,
} from 'lucide-react';
import { Button, Card, Select, Spinner } from '../../../../shared/ui';
import {
    astrbotConfigWarnings,
    astrbotSetup,
    astrbotWakeHint,
    enabledChatModels,
    type AstrBotLlmIssue,
} from '../../../../core/domain/apps/astrbotConfig';
import { ProviderDialog } from './ProviderDialog';
import { ProviderPresetMenu, useProviderEditor } from './providerEditor';
import { ASTRBOT_TAB_LABEL } from './astrbotNav';
import { JumpLink } from './parts';
import { cn } from '../../../../shared/utils/cn';
import type {
    AppInstance,
    AstrBotAiSettings,
    AstrBotDashboardStatus,
    AstrBotInstanceConfig,
} from '../../../../core/ipc/types';

/** 能回话的三个前提，按该补的先后排；状态卡底下那一行就是它们 */
type CondKey = 'link' | 'llm' | 'run';
type Cond = { key: CondKey; ok: boolean; label: string };

type Notice = { key: string; text: string; tab: string; action: string };

type Tone = 'ready' | 'todo' | 'idle';

const TONE_DOT: Record<Tone, string> = {
    ready: 'bg-success ring-success/15',
    todo: 'bg-brand ring-brand/15',
    idle: 'bg-text-disabled ring-text-disabled/15',
};

const LLM_LABEL: Record<AstrBotLlmIssue | 'ok', string> = {
    no_source: '大模型还没接入',
    no_model: '还没有可用的模型',
    no_default: '还没选用哪个模型',
    llm_off: '大模型回复关着',
    ok: '大模型已接入',
};

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
}> = ({
    instance,
    config,
    saved,
    onChange,
    errors,
    disabled,
    dash,
    onGoTab,
    onOpenLink,
    onStart,
    starting,
    onOpenWebUi,
}) => {
    const running = instance.state === 'running';
    const linked = !!instance.link;
    const setup = astrbotSetup(config, linked);
    const savedReady = !!saved && astrbotSetup(saved, linked).ready;
    const wake = astrbotWakeHint(config);
    const editor = useProviderEditor(config, onChange);
    const savedIds = new Set((saved?.sources ?? []).map((s) => s.id));
    const setAi = (patch: Partial<AstrBotAiSettings>) =>
        onChange({ ...config, ai: { ...config.ai, ...patch } });
    const chat = enabledChatModels(config);
    const chatSource =
        setup.chatSourceIndex >= 0 ? config.sources[setup.chatSourceIndex] : undefined;

    const conds: Cond[] = [
        {
            key: 'link',
            ok: setup.linkDone,
            label: linked
                ? 'QQ 已对接'
                : setup.linkDone
                  ? `走 ${config.other_platforms.join('、')}`
                  : 'QQ 还没对接',
        },
        { key: 'llm', ok: setup.llmIssue === null, label: LLM_LABEL[setup.llmIssue ?? 'ok'] },
        { key: 'run', ok: running, label: running ? '运行中' : '已停止' },
    ];
    const missing = conds.filter((c) => !c.ok);
    const next = missing[0]?.key;

    let tone: Tone;
    let title: string;
    let sub: ReactNode;
    let actions: ReactNode = null;
    if (!next) {
        tone = 'ready';
        title = savedReady ? '可以对话了' : '保存后就能对话了';
        sub = wake.sentence;
        if (savedReady) {
            actions = (
                <>
                    <Button size="sm" variant="secondary" onClick={() => onOpenWebUi()}>
                        <ExternalLink size={13} />
                        打开 WebUI
                    </Button>
                    <Button size="sm" variant="primary" onClick={() => onOpenWebUi('/chat')}>
                        <MessageSquare size={13} />
                        网页试聊
                    </Button>
                </>
            );
        }
    } else if (next === 'run' && missing.length === 1) {
        tone = 'idle';
        title = '配好了，启动就能对话';
        sub = '配置照常可以改，启动后生效';
        actions = <StartButton starting={starting} onStart={onStart} />;
    } else {
        tone = 'todo';
        title = `还差 ${missing.length} 步就能在 QQ 上对话`;
        if (next === 'link') {
            sub = '先对接一个 NapCat / SnowLuma 机器人，QQ 消息才会转给它';
            actions = (
                <Button size="sm" variant="primary" onClick={onOpenLink}>
                    <Link2 size={13} />
                    对接
                </Button>
            );
        } else if (next === 'run') {
            sub = '配置照常可以改，启动后生效';
            actions = <StartButton starting={starting} onStart={onStart} />;
        } else {
            switch (setup.llmIssue) {
                case 'no_source':
                    sub = '接入一个大模型：选一家提供商，填上 API Key';
                    actions = (
                        <ProviderPresetMenu onPick={editor.openCreate}>
                            <Button size="sm" variant="primary" disabled={disabled}>
                                添加提供商
                                <ChevronDown size={12} className="-mr-0.5 opacity-80" />
                            </Button>
                        </ProviderPresetMenu>
                    );
                    break;
                case 'no_model':
                    sub = chatSource?.enable
                        ? `「${chatSource.id}」下还没有启用的对话模型`
                        : `提供商「${chatSource?.id}」停用了`;
                    actions = (
                        <Button
                            size="sm"
                            variant="primary"
                            disabled={disabled}
                            onClick={() => editor.openEdit(setup.chatSourceIndex)}
                        >
                            {chatSource?.enable ? '加模型' : '去启用'}
                        </Button>
                    );
                    break;
                case 'no_default':
                    sub = '有可用的模型了，选一个来回复';
                    actions = (
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
                default:
                    sub = '大模型回复关着，现在只回指令和插件';
                    actions = (
                        <Button
                            size="sm"
                            variant="primary"
                            disabled={disabled}
                            onClick={() => setAi({ enable: true })}
                        >
                            打开
                        </Button>
                    );
            }
        }
    }

    const notices: Notice[] = astrbotConfigWarnings(config).map((w) => ({
        key: w.key,
        text: w.text,
        tab: w.area,
        action: `去「${ASTRBOT_TAB_LABEL[w.area]}」`,
    }));
    if (running && dash?.gate === 'auth') {
        notices.unshift({
            key: 'dash-auth',
            text: '控制台没登上，人格、知识库、会话规则暂时管不了',
            tab: 'connections',
            action: '去填 WebUI 密码',
        });
    } else if (running && dash?.gate === 'unreachable') {
        notices.unshift({
            key: 'dash-down',
            text: '连不上 AstrBot 控制台，人格、知识库、会话规则暂时管不了',
            tab: 'log',
            action: '看日志',
        });
    }

    const model = chat.find((m) => m.id === config.ai.default_provider_id);
    const g = config.gates;
    const tiles: TileDef[] = [
        {
            key: 'model',
            tab: 'models',
            icon: Boxes,
            label: '对话模型',
            value: model ? model.model || model.id : null,
            empty: '还没有',
        },
        {
            key: 'persona',
            tab: 'persona',
            icon: UserRound,
            label: '人格',
            value: config.ai.default_personality || null,
            empty: '内置',
        },
        {
            key: 'kb',
            tab: 'kb',
            icon: Library,
            label: '知识库',
            value: config.kb.names.length ? config.kb.names.join('、') : null,
            empty: '未挂载',
        },
        { key: 'wake', tab: 'talk', icon: AtSign, label: '唤醒方式', value: wake.short, empty: '' },
        {
            key: 'scope',
            tab: 'talk',
            icon: ShieldCheck,
            label: '回复范围',
            // 名单为空时上游不做检查，开着开关也是所有会话
            value:
                g.enable_id_white_list && g.id_whitelist.length
                    ? `白名单 ${g.id_whitelist.length} 个会话`
                    : '所有会话',
            empty: '',
        },
        {
            key: 'subagent',
            tab: 'subagent',
            icon: Bot,
            label: '子代理',
            value: config.subagent.main_enable ? `${config.subagent.agents.length} 个` : null,
            empty: '没开',
        },
    ];

    return (
        <div className="flex flex-col">
            <Card variant="hero" padding="none" className="px-6 py-5">
                <div className="flex flex-wrap items-start justify-between gap-x-6 gap-y-4">
                    <div className="min-w-0 flex-1">
                        <h2 className="flex items-center gap-3 font-display text-[19px] font-semibold leading-snug text-text">
                            <span
                                className={cn(
                                    'h-2 w-2 shrink-0 rounded-full ring-4',
                                    TONE_DOT[tone],
                                )}
                                aria-hidden
                            />
                            {title}
                        </h2>
                        <p className="mt-1.5 pl-5 text-[13px] leading-relaxed text-text-secondary">
                            {sub}
                        </p>
                    </div>
                    {actions && <div className="flex shrink-0 items-center gap-2">{actions}</div>}
                </div>

                <ul className="mt-4 flex flex-wrap items-center gap-x-6 gap-y-2 pl-5">
                    {conds.map((c) => (
                        <li
                            key={c.key}
                            className={cn(
                                'inline-flex items-center gap-1.5 text-xs',
                                c.ok
                                    ? 'text-text-secondary'
                                    : c.key === next
                                      ? 'font-medium text-text'
                                      : 'text-text-tertiary',
                            )}
                        >
                            {c.ok ? (
                                <CheckCircle2 size={14} className="text-success" />
                            ) : (
                                <Circle
                                    size={14}
                                    className={c.key === next ? 'text-brand' : 'text-text-disabled'}
                                />
                            )}
                            {c.label}
                        </li>
                    ))}
                </ul>

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

            <div className="mb-3 mt-8 flex items-center gap-2.5">
                <span className="h-3.5 w-0.5 shrink-0 rounded-full bg-brand/45" aria-hidden />
                <h3 className="text-[13.5px] font-semibold leading-none tracking-tight text-text">
                    当前设置
                </h3>
            </div>
            <div className="grid grid-cols-2 gap-3 lg:grid-cols-3">
                {tiles.map((t) => (
                    <SettingTile key={t.key} tile={t} onOpen={() => onGoTab(t.tab)} />
                ))}
            </div>

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

const StartButton: React.FC<{ starting: boolean; onStart: () => void }> = ({
    starting,
    onStart,
}) => (
    <Button size="sm" variant="primary" disabled={starting} onClick={onStart}>
        {starting ? <Spinner size="xs" className="text-white" /> : <Play size={13} />}
        启动
    </Button>
);

type TileDef = {
    key: string;
    tab: string;
    icon: ComponentType<LucideProps>;
    label: string;
    value: string | null;
    /** value 为空时显示的灰字 */
    empty: string;
};

const SettingTile: React.FC<{ tile: TileDef; onOpen: () => void }> = ({ tile, onOpen }) => {
    const Icon = tile.icon;
    return (
        <button
            type="button"
            title={`去「${ASTRBOT_TAB_LABEL[tile.tab]}」`}
            onClick={onOpen}
            className={cn(
                'group flex min-w-0 items-center gap-3 rounded-md bg-surface px-3.5 py-3 text-left shadow-card',
                'transition-[box-shadow,transform] duration-200 hover:-translate-y-px hover:shadow-popover',
                'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand/40',
            )}
        >
            <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-md bg-inset text-text-tertiary transition-colors group-hover:text-brand">
                <Icon size={16} />
            </span>
            <span className="min-w-0 flex-1">
                <span className="block text-xs text-text-tertiary">{tile.label}</span>
                <span
                    className={cn(
                        'mt-0.5 block truncate text-[13.5px]',
                        tile.value ? 'font-medium text-text' : 'text-text-disabled',
                    )}
                >
                    {tile.value ?? tile.empty}
                </span>
            </span>
            <ChevronRight
                size={14}
                className="shrink-0 text-text-disabled transition-colors group-hover:text-text-secondary"
            />
        </button>
    );
};
