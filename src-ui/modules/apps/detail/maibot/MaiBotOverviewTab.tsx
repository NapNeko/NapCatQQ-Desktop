// 概览：状态卡回答「麦麦现在能不能在 QQ 上回话、还差什么」，全卡只给一个主按钮。
// 能回话的前提只看 Desktop 管得着的三件事：对接、名单放行、在跑。
// 模型 Key 在 MaiBot 自己的 WebUI 里配，Desktop 不读 model_config，所以只指路、不判定。

import type { ReactNode } from 'react';
import { CheckCircle2, Circle, ExternalLink, Link2, ListChecks, Play } from 'lucide-react';
import { Button, Card, Spinner } from '../../../../shared/ui';
import { maibotChatDropsEverything, maibotChatScope } from '../../../../core/domain/apps/maibotConfig';
import { cn } from '../../../../shared/utils/cn';
import type { AppInstance, MaiBotInstanceConfig } from '../../../../core/ipc/types';

type CondKey = 'link' | 'chat' | 'run';
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
    onOpenWebUi: () => void;
}> = ({ instance, config, onGoTab, onOpenLink, onStart, starting, onOpenWebUi }) => {
    const running = instance.state === 'running';
    const linked = !!instance.link;
    const chat = config.adapter?.chat;
    const chatOk = !!chat && !maibotChatDropsEverything(chat);

    const conds: { key: CondKey; ok: boolean; label: string }[] = [
        { key: 'link', ok: linked, label: linked ? 'QQ 已对接' : 'QQ 还没对接' },
        {
            key: 'chat',
            ok: chatOk,
            label: !chat ? '适配器缺失' : chatOk ? `回复：${maibotChatScope(chat)}` : '还没放行群聊',
        },
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
        title = '可以在 QQ 上找麦麦聊天了';
        sub = '还没配大模型的话，在 WebUI 里配：第一次登录会带你走一遍';
        actions = (
            <Button size="sm" variant="primary" onClick={onOpenWebUi}>
                <ExternalLink size={13} />
                打开 WebUI
            </Button>
        );
    } else if (next === 'run' && missing.length === 1) {
        tone = 'idle';
        title = '都接好了，启动就能聊';
        sub = '启动后在 WebUI 里配大模型，第一次登录有向导';
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
            sub = '启动后在 WebUI 里配大模型，第一次登录有向导';
            actions = <StartButton starting={starting} onStart={onStart} />;
        }
    }

    return (
        <div className="flex flex-col gap-4">
            <Card variant="hero" padding="none" className="px-6 py-5">
                <div className="flex flex-wrap items-start justify-between gap-x-6 gap-y-4">
                    <div className="min-w-0 flex-1">
                        <h2 className="flex items-center gap-3 font-display text-[19px] font-semibold leading-snug text-text">
                            <span className={cn('h-2 w-2 shrink-0 rounded-full ring-4', TONE_DOT[tone])} aria-hidden />
                            {title}
                        </h2>
                        <p className="mt-1.5 pl-5 text-[13px] leading-relaxed text-text-secondary">{sub}</p>
                    </div>
                    {actions && <div className="flex shrink-0 items-center gap-2">{actions}</div>}
                </div>
                <ul className="mt-4 flex flex-wrap items-center gap-x-6 gap-y-2 pl-5">
                    {conds.map((c) => (
                        <li
                            key={c.key}
                            className={cn(
                                'inline-flex items-center gap-1.5 text-xs',
                                c.ok ? 'text-text-secondary' : c.key === next ? 'font-medium text-text' : 'text-text-tertiary',
                            )}
                        >
                            {c.ok ? (
                                <CheckCircle2 size={14} className="text-success" />
                            ) : (
                                <Circle size={14} className={c.key === next ? 'text-brand' : 'text-text-disabled'} />
                            )}
                            {c.label}
                        </li>
                    ))}
                </ul>
            </Card>

            <Card padding="none" className="flex flex-wrap items-center justify-between gap-3 px-5 py-4">
                <p className="min-w-0 flex-1 text-[13px] leading-relaxed text-text-secondary">
                    大模型、人格、表情包和插件都在 MaiBot 自己的 WebUI 里配。登录 token 在「连接」页，打开时会自动复制。
                </p>
                <Button size="sm" variant="secondary" disabled={!running} onClick={onOpenWebUi}>
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
