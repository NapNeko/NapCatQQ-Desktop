// 概览：状态卡回答「麦麦现在能不能在 QQ 上回话、还差什么」，全卡只给一个主按钮。
// 能回话的前提看四件事：对接、模型、名单放行、在跑，都是 Desktop 自己读得到、改得了的。

import type { ReactNode } from 'react';
import { Bot, CheckCircle2, Circle, ExternalLink, Link2, ListChecks, Play } from 'lucide-react';
import { Button, Card, Spinner } from '../../../../shared/ui';
import {
    maibotChatDropsEverything,
    maibotChatScope,
    maibotModelSetupIssue,
} from '../../../../core/domain/apps/maibotConfig';
import { cn } from '../../../../shared/utils/cn';
import type { AppInstance, MaiBotInstanceConfig } from '../../../../core/ipc/types';
import { MaiBotRuntimeCard } from './MaiBotRuntimeCard';

type CondKey = 'link' | 'model' | 'chat' | 'run';

const MODEL_SUB: Record<Exclude<ReturnType<typeof maibotModelSetupIssue>, null>, string> = {
    no_provider: '还没有模型提供商，加一个并填上 API Key',
    placeholder_key: '默认带的 DeepSeek 还是占位的 API Key，换成你自己的 Key，或者换一家提供商',
    no_task_model: '回复、规划、杂务三个任务都要挑一个模型',
};
type Tone = 'ready' | 'todo' | 'idle';

const TONE_DOT: Record<Tone, string> = {
    ready: 'bg-success ring-success/15',
    todo: 'bg-brand ring-brand/15',
    idle: 'bg-text-disabled ring-text-disabled/15',
};

export const MaiBotOverviewTab: React.FC<{
    instance: AppInstance;
    config: MaiBotInstanceConfig;
    onGoTab: (tab: string) => void;
    onOpenLink: () => void;
    onStart: () => void;
    starting: boolean;
    /** path 是 WebUI 里的某一页；不给就开首页 */
    onOpenWebUi: (path?: string) => void;
}> = ({ instance, config, onGoTab, onOpenLink, onStart, starting, onOpenWebUi }) => {
    const running = instance.state === 'running';
    const linked = !!instance.link;
    const chat = config.adapter?.chat;
    const chatOk = !!chat && !maibotChatDropsEverything(chat);
    const modelIssue = maibotModelSetupIssue(config.models);

    const conds: { key: CondKey; ok: boolean; label: string }[] = [
        { key: 'link', ok: linked, label: linked ? 'QQ 已对接' : 'QQ 还没对接' },
        { key: 'model', ok: !modelIssue, label: modelIssue ? '模型还没配好' : '模型已配好' },
        {
            key: 'chat',
            ok: chatOk,
            label: !chat
                ? '适配器缺失'
                : chatOk
                  ? `回复：${maibotChatScope(chat)}`
                  : '还没放行群聊',
        },
        { key: 'run', ok: running, label: running ? '运行中' : '已停止' },
    ];
    const missing = conds.filter((c) => !c.ok);
    const next = missing[0]?.key;

    let tone: Tone;
    let title: string;
    let sub: ReactNode;
    let actions: ReactNode;
    if (!next) {
        tone = 'ready';
        title = '可以在 QQ 上找麦麦聊天了';
        sub = '人格、回复方式这些左边各页都能改，运行中保存马上生效';
        actions = (
            <Button size="sm" variant="primary" onClick={() => onOpenWebUi()}>
                <ExternalLink size={13} />
                打开 WebUI
            </Button>
        );
    } else if (next === 'run' && missing.length === 1) {
        tone = 'idle';
        title = '都接好了，启动就能聊';
        sub = '启动后在 QQ 里找麦麦说句话试试';
        actions = <StartButton starting={starting} onStart={onStart} />;
    } else {
        tone = 'todo';
        title = `还差 ${missing.length} 步就能在 QQ 上聊`;
        if (next === 'link') {
            sub = '先对接同一台机器上的 NapCat / SnowLuma 机器人：Bot 开一个 WS 服务，麦麦连过去';
            actions = (
                <Button size="sm" variant="primary" onClick={onOpenLink}>
                    <Link2 size={13} />
                    对接
                </Button>
            );
        } else if (next === 'model' && modelIssue) {
            sub = MODEL_SUB[modelIssue];
            actions = (
                <Button size="sm" variant="primary" onClick={() => onGoTab('models')}>
                    <Bot size={13} />
                    去配模型
                </Button>
            );
        } else if (next === 'chat') {
            sub = chat
                ? '群聊和私聊名单都是空的白名单，消息全被丢掉；先放行要回复的群'
                : '实例里没有 NapCat 适配器插件，重新安装实例可以补上';
            actions = chat ? (
                <Button size="sm" variant="primary" onClick={() => onGoTab('chat')}>
                    <ListChecks size={13} />
                    去放行
                </Button>
            ) : null;
        } else {
            sub = '启动后在 QQ 里找麦麦说句话试试';
            actions = <StartButton starting={starting} onStart={onStart} />;
        }
    }

    return (
        <div className="flex flex-col gap-4">
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
            </Card>

            <MaiBotRuntimeCard instance={instance} onOpenChat={() => onGoTab('trychat')} />

            <Card
                padding="none"
                className="flex flex-wrap items-center justify-between gap-3 px-5 py-4"
            >
                <p className="min-w-0 flex-1 text-[13px] leading-relaxed text-text-secondary">
                    记忆图谱、学到的表达方式、表情包库在 MaiBot 自己的 WebUI 里看。登录 token
                    在「连接」页，打开时会自动复制。
                </p>
                <Button
                    size="sm"
                    variant="secondary"
                    disabled={!running}
                    onClick={() => onOpenWebUi()}
                >
                    <ExternalLink size={13} />
                    {running ? '打开 WebUI' : '启动后可打开 WebUI'}
                </Button>
            </Card>
        </div>
    );
};

function StartButton({ starting, onStart }: { starting: boolean; onStart: () => void }) {
    return (
        <Button size="sm" variant="primary" disabled={starting} onClick={onStart}>
            {starting ? <Spinner size="sm" className="text-white" /> : <Play size={13} />}
            启动
        </Button>
    );
}
