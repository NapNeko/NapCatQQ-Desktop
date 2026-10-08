// 概览：状态卡回答「Koishi 现在能不能在 QQ 上回话、还差什么」，全卡只给一个主按钮；
// 下面是运行中的机器人（控制台 status 推送）、当前设置的卡片（点一张去对应页）、控制台里几个常用页的直达。
// 版式和 AstrBot / 麦麦的概览同一套：hero 大卡 + 带图标方块的卡片。

import type { ComponentType, ReactNode } from 'react';
import {
    AtSign,
    CheckCircle2,
    ChevronRight,
    Circle,
    Database,
    FileText,
    FolderTree,
    Hash,
    Link2,
    MessageSquare,
    Network,
    Play,
    Puzzle,
    RotateCw,
    ScrollText,
    Store,
    TerminalSquare,
    type LucideProps,
} from 'lucide-react';
import { Badge, Button, Card, Spinner } from '../../../../shared/ui';
import { cn } from '../../../../shared/utils/cn';
import {
    effective,
    isGroup,
    isLinkNode,
    koishiServer,
    linkNode,
    walk,
} from '../../../../core/domain/apps/koishiConfig';
import { useKoishiRestart, useKoishiStatus } from '../../../../hooks/apps/useKoishiRuntime';
import type {
    AppInstance,
    KoishiBotState,
    KoishiBotStatus,
    KoishiInstanceConfig,
} from '../../../../core/ipc/types';

type CondKey = 'link' | 'adapter' | 'run';
type Tone = 'ready' | 'todo' | 'idle';

const TONE_DOT: Record<Tone, string> = {
    ready: 'bg-success ring-success/15',
    todo: 'bg-brand ring-brand/15',
    idle: 'bg-text-disabled ring-text-disabled/15',
};

const BOT_STATE: Record<
    KoishiBotState,
    { label: string; tone: 'success' | 'warning' | 'neutral' }
> = {
    online: { label: '在线', tone: 'success' },
    connect: { label: '连接中', tone: 'warning' },
    reconnect: { label: '重连中', tone: 'warning' },
    disconnect: { label: '断开中', tone: 'neutral' },
    offline: { label: '离线', tone: 'neutral' },
};

/** 控制台功能的直达（都是应用内的原生页：试聊 / 日志 / 数据库 / 文件 / 指令） */
const CONSOLE_LINKS: {
    tab: string;
    label: string;
    sub: string;
    icon: ComponentType<LucideProps>;
}[] = [
    { tab: 'sandbox', label: '试聊', sub: '不连 QQ 也能试指令', icon: MessageSquare },
    { tab: 'log', label: '日志', sub: '搜索、按级别筛', icon: ScrollText },
    { tab: 'database', label: '数据库', sub: '用户、频道和插件的表', icon: Database },
    { tab: 'files', label: '文件', sub: '实例目录里的文件', icon: FileText },
    { tab: 'commands', label: '指令', sub: '别名、权限、冷却', icon: TerminalSquare },
];

function SectionTitle({ children }: { children: ReactNode }) {
    return (
        <div className="mb-3 mt-8 flex items-center gap-2.5">
            <span className="h-3.5 w-0.5 shrink-0 rounded-full bg-brand/45" aria-hidden />
            <h3 className="text-[13.5px] font-semibold leading-none tracking-tight text-text">
                {children}
            </h3>
        </div>
    );
}

export const KoishiOverviewTab: React.FC<{
    instance: AppInstance;
    config: KoishiInstanceConfig;
    onGoTab: (tab: string) => void;
    onOpenLink: () => void;
    onStart: () => void;
    starting: boolean;
}> = ({ instance, config, onGoTab, onOpenLink, onStart, starting }) => {
    const running = instance.state === 'running';
    const linked = !!instance.link;
    const entry = linkNode(config);
    // 对接条目在，但自己或上级分组被停了：Bot 连得上端口，Koishi 却不认
    const entryOn = !!entry && effective(config.plugins).some(isLinkNode);

    const conds: { key: CondKey; ok: boolean; label: string }[] = [
        {
            key: 'link',
            ok: linked,
            label: linked ? `已对接 Bot ${instance.link?.bot_id}` : 'QQ 还没对接',
        },
        {
            key: 'adapter',
            ok: !linked || entryOn,
            label: !linked
                ? 'OneBot 适配器随对接写好'
                : entryOn
                  ? 'OneBot 适配器已启用'
                  : 'OneBot 适配器被停用了',
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
        title = '可以在 QQ 上用 Koishi 了';
        sub = '给机器人发一句 help 看看有哪些指令；想要新功能去插件市场挑';
        actions = (
            <Button size="sm" variant="primary" onClick={() => onGoTab('sandbox')}>
                <MessageSquare size={13} />
                去试聊
            </Button>
        );
    } else if (next === 'run' && missing.length === 1) {
        tone = 'idle';
        title = '都接好了，启动就能用';
        sub = '启动后在 QQ 里给机器人发一句 help 试试';
        actions = <StartButton starting={starting} onStart={onStart} />;
    } else {
        tone = 'todo';
        title = `还差 ${missing.length} 步就能在 QQ 上用`;
        if (next === 'link') {
            sub = '先对接一个 NapCat / SnowLuma 机器人：Bot 作客户端连 Koishi，本机、远端都行';
            actions = (
                <Button size="sm" variant="primary" onClick={onOpenLink}>
                    <Link2 size={13} />
                    对接
                </Button>
            );
        } else if (next === 'adapter') {
            sub = '插件树里 adapter-onebot:ncd-link（或它所在的分组）是停用的，打开它 Bot 才连得上';
            actions = (
                <Button size="sm" variant="primary" onClick={() => onGoTab('plugins')}>
                    <Puzzle size={13} />
                    去插件页
                </Button>
            );
        } else {
            sub = '启动后在 QQ 里给机器人发一句 help 试试';
            actions = <StartButton starting={starting} onStart={onStart} />;
        }
    }

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
            </Card>

            {running && <RuntimeSection instance={instance} />}

            <SectionTitle>当前设置</SectionTitle>
            <SettingTiles instance={instance} config={config} onGoTab={onGoTab} />

            <SectionTitle>工具</SectionTitle>
            <div className="grid grid-cols-2 gap-3 lg:grid-cols-5">
                {CONSOLE_LINKS.map((l) => (
                    <ConsoleLink
                        key={l.tab}
                        link={l}
                        disabled={!running}
                        onOpen={() => onGoTab(l.tab)}
                    />
                ))}
            </div>
            {!running && <p className="mt-2.5 text-2xs text-text-tertiary">启动后才能用</p>}
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

const pct = (v: number | undefined) =>
    v === undefined ? '—' : `${(v * 100).toFixed(v < 0.1 ? 1 : 0)}%`;

function Stat({ label, value }: { label: string; value: string }) {
    return (
        <div className="flex min-w-0 flex-col gap-0.5">
            <span className="font-display text-[17px] font-semibold tabular-nums leading-tight text-text">
                {value}
            </span>
            <span className="text-2xs text-text-tertiary">{label}</span>
        </div>
    );
}

function BotRow({ bot }: { bot: KoishiBotStatus }) {
    const st = BOT_STATE[bot.state];
    const name = bot.name || bot.self_id || bot.sid;
    return (
        <li className="flex items-center gap-3 py-2.5 first:pt-0 last:pb-0">
            <span className="relative flex h-9 w-9 shrink-0 items-center justify-center overflow-hidden rounded-full bg-inset text-[13px] font-medium text-text-secondary">
                {bot.avatar ? (
                    <img
                        src={bot.avatar}
                        alt=""
                        className="h-full w-full object-cover"
                        referrerPolicy="no-referrer"
                    />
                ) : (
                    name.slice(0, 1).toUpperCase()
                )}
            </span>
            <div className="min-w-0 flex-1">
                <div className="flex items-center gap-2">
                    <span className="truncate text-[13px] font-medium text-text">{name}</span>
                    <Badge tone={st.tone}>{st.label}</Badge>
                </div>
                <p className="mt-0.5 truncate text-2xs text-text-tertiary">
                    <span className="font-mono">{bot.sid}</span>
                    {bot.error && <span className="ml-2 text-danger">{bot.error}</span>}
                </p>
            </div>
            <div className="hidden shrink-0 items-center gap-5 text-right sm:flex">
                <div>
                    <p className="text-[13px] font-medium tabular-nums text-text">
                        {bot.message_received}
                    </p>
                    <p className="text-2xs text-text-tertiary">收到</p>
                </div>
                <div>
                    <p className="text-[13px] font-medium tabular-nums text-text">
                        {bot.message_sent}
                    </p>
                    <p className="text-2xs text-text-tertiary">发出</p>
                </div>
            </div>
        </li>
    );
}

/** 运行中：控制台推的 Bot 在线和占用；控制台还没起来时说明一句，不当成出错 */
function RuntimeSection({ instance }: { instance: AppInstance }) {
    const status = useKoishiStatus(instance.id, true);
    const restart = useKoishiRestart(instance.id, instance.display_name);
    const s = status.data;
    const ok = s?.gate === 'ok';
    const online = ok ? s.bots.filter((b) => b.state === 'online').length : 0;

    return (
        <Card padding="none" className="mt-4 flex flex-col gap-4 px-5 py-4">
            <div className="flex flex-wrap items-center justify-between gap-3">
                {ok ? (
                    <div className="grid grid-cols-3 gap-x-8">
                        <Stat label="机器人在线" value={`${online} / ${s.bots.length}`} />
                        <Stat label="Koishi 占内存" value={pct(s.memory?.[0])} />
                        <Stat label="Koishi 占 CPU" value={pct(s.cpu?.[0])} />
                    </div>
                ) : (
                    <div className="flex min-w-0 items-center gap-2 text-[13px] text-text-secondary">
                        {(!s || s.gate === 'unreachable') && <Spinner size="sm" />}
                        <span>{s?.message || '正在连 Koishi 控制台…'}</span>
                    </div>
                )}
                <Button
                    size="sm"
                    variant="secondary"
                    disabled={!ok || restart.isPending}
                    onClick={() => restart.mutate()}
                >
                    {restart.isPending ? <Spinner size="sm" /> : <RotateCw size={13} />}
                    重启
                </Button>
            </div>
            {ok && (
                <div className="border-t border-border-subtle/70 pt-3.5">
                    {s.bots.length === 0 ? (
                        <p className="text-[13px] text-text-tertiary">还没有机器人连上来</p>
                    ) : (
                        <ul className="flex flex-col divide-y divide-border-subtle/70">
                            {s.bots.map((b) => (
                                <BotRow key={b.sid} bot={b} />
                            ))}
                        </ul>
                    )}
                </div>
            )}
        </Card>
    );
}

type TileDef = {
    key: string;
    tab: string;
    icon: ComponentType<LucideProps>;
    label: string;
    value: string | null;
    empty: string;
};

const TAB_LABEL: Record<string, string> = {
    server: '服务器',
    global: '全局设置',
    plugins: '插件',
    market: '插件市场',
    connection: '连接',
};

function listText(v: unknown): string | null {
    const arr = Array.isArray(v) ? v.map(String).filter((s) => s.trim()) : [];
    return arr.length ? arr.map((s) => `「${s}」`).join(' ') : null;
}

function SettingTiles({
    instance,
    config,
    onGoTab,
}: {
    instance: AppInstance;
    config: KoishiInstanceConfig;
    onGoTab: (tab: string) => void;
}) {
    const server = koishiServer(config);
    const plugins = walk(config.plugins).filter((n) => !isGroup(n));
    const on = effective(config.plugins).filter((n) => !isGroup(n));
    const tiles: TileDef[] = [
        {
            key: 'port',
            tab: 'server',
            icon: Network,
            label: '监听',
            value: `${server.host === '0.0.0.0' ? '所有网卡' : '只本机'} · ${server.port}`,
            empty: '',
        },
        {
            key: 'prefix',
            tab: 'global',
            icon: Hash,
            label: '指令前缀',
            value: listText(config.global.prefix),
            empty: '没设',
        },
        {
            key: 'nickname',
            tab: 'global',
            icon: AtSign,
            label: '昵称',
            value: listText(config.global.nickname),
            empty: '没设',
        },
        {
            key: 'plugins',
            tab: 'plugins',
            icon: FolderTree,
            label: '插件',
            value: `${on.length} / ${plugins.length} 个在用`,
            empty: '',
        },
        {
            key: 'link',
            tab: 'connection',
            icon: Link2,
            label: '对接',
            value: instance.link ? `Bot ${instance.link.bot_id}` : null,
            empty: '还没对接',
        },
        {
            key: 'market',
            tab: 'market',
            icon: Store,
            label: '插件市场',
            value: '几千个插件可装',
            empty: '',
        },
    ];
    return (
        <div className="grid grid-cols-2 gap-3 lg:grid-cols-3">
            {tiles.map((t) => {
                const Icon = t.icon;
                return (
                    <button
                        key={t.key}
                        type="button"
                        title={`去「${TAB_LABEL[t.tab]}」`}
                        onClick={() => onGoTab(t.tab)}
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
                            <span className="block text-xs text-text-tertiary">{t.label}</span>
                            <span
                                className={cn(
                                    'mt-0.5 block truncate text-[13.5px]',
                                    t.value ? 'font-medium text-text' : 'text-text-disabled',
                                )}
                            >
                                {t.value ?? t.empty}
                            </span>
                        </span>
                        <ChevronRight
                            size={14}
                            className="shrink-0 text-text-disabled transition-colors group-hover:text-text-secondary"
                        />
                    </button>
                );
            })}
        </div>
    );
}

function ConsoleLink({
    link,
    disabled,
    onOpen,
}: {
    link: (typeof CONSOLE_LINKS)[number];
    disabled: boolean;
    onOpen: () => void;
}) {
    const Icon = link.icon;
    return (
        <button
            type="button"
            disabled={disabled}
            onClick={onOpen}
            className={cn(
                'group flex min-w-0 flex-col gap-2 rounded-md bg-surface px-3.5 py-3 text-left shadow-card',
                'transition-[box-shadow,transform] duration-200 enabled:hover:-translate-y-px enabled:hover:shadow-popover',
                'disabled:cursor-not-allowed disabled:opacity-55',
                'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand/40',
            )}
        >
            <span className="flex items-center justify-between">
                <span className="flex h-8 w-8 items-center justify-center rounded-md bg-inset text-text-tertiary transition-colors group-enabled:group-hover:text-brand">
                    <Icon size={15} />
                </span>
                <ChevronRight
                    size={13}
                    className="text-text-disabled transition-colors group-enabled:group-hover:text-text-secondary"
                />
            </span>
            <span className="min-w-0">
                <span className="block text-[13px] font-medium text-text">{link.label}</span>
                <span className="mt-0.5 block truncate text-2xs text-text-tertiary">
                    {link.sub}
                </span>
            </span>
        </button>
    );
}
