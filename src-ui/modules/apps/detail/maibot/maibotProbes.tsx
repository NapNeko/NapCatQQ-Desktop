// 运行中才出的几个小工具：测提供商连接、从服务商拉模型、MCP 连接状态和试连、从聊过的会话里挑目标。
// 都走麦麦自己的 WebUI 接口；结果只给发起的那张卡看，不改表单以外的东西。

import { useState } from 'react';
import { Activity, Check, List, MessagesSquare, PlugZap } from 'lucide-react';
import {
    Button,
    Popover,
    PopoverClose,
    PopoverContent,
    PopoverTrigger,
    Spinner,
} from '../../../../shared/ui';
import { cn } from '../../../../shared/utils/cn';
import { toAppConfigError } from '../../../../core/domain/apps/appConfigError';
import type {
    MaiBotAPIProvider,
    MaiBotChatSession,
    MaiBotMCPServerItemConfig,
    MaiBotMcpStatus,
    MaiBotProviderCheck,
    MaiBotProviderModel,
} from '../../../../core/ipc/types';
import { useMaiBotProbes } from '../../../../hooks/apps/useMaiBotRuntime';

const errText = (e: unknown) => toAppConfigError(e).message;

// 列表十几二十条时不限高会顶出窗口；和 AstrBot 预设菜单同一套写法
const SCROLL_STYLE = { maxHeight: 'min(22rem, calc(var(--radix-popover-content-available-height, 22rem) - 8px))' };

function describeCheck(c: MaiBotProviderCheck): { tone: 'ok' | 'bad' | 'meh'; text: string } {
    const ms = c.latency_ms !== undefined ? `，${Math.round(c.latency_ms)} ms` : '';
    if (!c.network_ok) return { tone: 'bad', text: c.error || '连不上这个地址' };
    if (c.api_key_valid === false) return { tone: 'bad', text: c.error || 'API Key 不对或过期了' };
    if (c.api_key_valid) return { tone: 'ok', text: `连得上，Key 有效${ms}` };
    return { tone: 'meh', text: `地址连得上${ms}，Key 没法验证` };
}

const TONE: Record<'ok' | 'bad' | 'meh', string> = {
    ok: 'text-success',
    bad: 'text-danger',
    meh: 'text-text-tertiary',
};

/** 从服务商列出的模型里挑。`added` 里有的打勾，挑了由调用方决定是加新模型还是填标识 */
const ModelMenu: React.FC<{
    loading: boolean;
    error: unknown;
    models: readonly MaiBotProviderModel[] | undefined;
    added: ReadonlySet<string>;
    onPick: (id: string) => void;
}> = ({ loading, error, models, added, onPick }) => {
    if (loading) {
        return (
            <div className="flex items-center gap-2 px-3 py-3 text-xs text-text-secondary">
                <Spinner size="sm" />
                正在问服务商…
            </div>
        );
    }
    if (error) return <p className="px-3 py-3 text-xs text-danger">{errText(error)}</p>;
    if (!models?.length) return <p className="px-3 py-3 text-xs text-text-tertiary">服务商没列出模型</p>;
    return (
        <div className="overflow-y-auto overscroll-contain" style={SCROLL_STYLE}>
            {models.map((m) => (
                <PopoverClose key={m.id} asChild>
                    <button
                        type="button"
                        className="flex w-full items-center justify-between gap-3 rounded-sm px-2 py-1.5 text-left hover:bg-inset"
                        onClick={() => onPick(m.id)}
                    >
                        <span className="min-w-0 truncate font-mono text-xs text-text">{m.id}</span>
                        {added.has(m.id) && <Check size={13} className="shrink-0 text-success" />}
                    </button>
                </PopoverClose>
            ))}
        </div>
    );
};

export const ProviderProbe: React.FC<{
    instanceId: string;
    provider: MaiBotAPIProvider;
    /** 这家已经加过的模型标识 */
    added: ReadonlySet<string>;
    onAddModel: (identifier: string) => void;
    disabled?: boolean;
}> = ({ instanceId, provider, added, onAddModel, disabled }) => {
    const { providerModels, testProvider } = useMaiBotProbes(instanceId);
    const result = testProvider.data ? describeCheck(testProvider.data) : null;
    return (
        <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
            <Button
                size="sm"
                variant="ghost"
                disabled={disabled || testProvider.isPending}
                onClick={() => testProvider.mutate(provider)}
            >
                {testProvider.isPending ? <Spinner size="sm" /> : <Activity size={13} />}
                测连接
            </Button>
            <Popover onOpenChange={(open) => open && providerModels.mutate(provider)}>
                <PopoverTrigger asChild>
                    <Button size="sm" variant="ghost" disabled={disabled}>
                        <List size={13} />
                        从服务商加模型
                    </Button>
                </PopoverTrigger>
                <PopoverContent align="start" sideOffset={6} className="w-80 p-1">
                    <ModelMenu
                        loading={providerModels.isPending}
                        error={providerModels.error}
                        models={providerModels.data}
                        added={added}
                        onPick={onAddModel}
                    />
                </PopoverContent>
            </Popover>
            {testProvider.error ? (
                <span className="text-2xs text-danger">{errText(testProvider.error)}</span>
            ) : (
                result && <span className={cn('text-2xs', TONE[result.tone])}>{result.text}</span>
            )}
        </div>
    );
};

/** 模型卡上填标识用：列出它所属提供商的模型，点一个填进去 */
export const ModelIdPicker: React.FC<{
    instanceId: string;
    provider: MaiBotAPIProvider | undefined;
    current: string;
    onPick: (identifier: string) => void;
    disabled?: boolean;
}> = ({ instanceId, provider, current, onPick, disabled }) => {
    const { providerModels } = useMaiBotProbes(instanceId);
    return (
        <Popover onOpenChange={(open) => open && provider && providerModels.mutate(provider)}>
            <PopoverTrigger asChild>
                <button
                    type="button"
                    disabled={disabled || !provider}
                    className="self-start text-2xs text-brand hover:underline disabled:cursor-not-allowed disabled:text-text-disabled disabled:no-underline"
                >
                    {provider ? '从服务商列表里挑' : '先选提供商才能列模型'}
                </button>
            </PopoverTrigger>
            <PopoverContent align="start" sideOffset={6} className="w-80 p-1">
                <ModelMenu
                    loading={providerModels.isPending}
                    error={providerModels.error}
                    models={providerModels.data}
                    added={new Set(current ? [current] : [])}
                    onPick={onPick}
                />
            </PopoverContent>
        </Popover>
    );
};

const McpServerRow: React.FC<{
    instanceId: string;
    server: MaiBotMCPServerItemConfig;
    live: MaiBotMcpStatus['servers'][number] | undefined;
    initialized: boolean;
}> = ({ instanceId, server, live, initialized }) => {
    const { testMcp } = useMaiBotProbes(instanceId);
    const t = testMcp.data;
    let state: { tone: 'ok' | 'bad' | 'meh'; text: string };
    if (!server.enabled) state = { tone: 'meh', text: '没启用' };
    else if (!initialized) state = { tone: 'meh', text: '麦麦还在初始化 MCP' };
    else if (!live) state = { tone: 'meh', text: '还没加载（刚保存的要等麦麦重载）' };
    else if (live.connected) state = { tone: 'ok', text: `已连上 · ${live.tool_count} 个工具` };
    else state = { tone: 'bad', text: live.error || '没连上' };
    return (
        <div className="flex flex-col gap-1.5 py-2.5">
            <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
                <span className="min-w-0 truncate text-[13px] font-medium text-text">{server.name || '未命名'}</span>
                <span className="font-mono text-2xs text-text-tertiary">{server.transport}</span>
                <span className={cn('text-2xs', TONE[state.tone])}>{state.text}</span>
                <span className="flex-1" />
                <Button size="sm" variant="ghost" disabled={testMcp.isPending} onClick={() => testMcp.mutate(server)}>
                    {testMcp.isPending ? <Spinner size="sm" /> : <PlugZap size={13} />}
                    试连
                </Button>
            </div>
            {testMcp.error ? (
                <p className="text-2xs text-danger">{errText(testMcp.error)}</p>
            ) : t ? (
                t.success ? (
                    <p className="text-2xs text-success">
                        连得上，有 {t.tools.length} 个工具
                        {t.tools.length > 0 && (
                            <span className="text-text-tertiary">
                                ：{t.tools.slice(0, 6).map((x) => x.title || x.name).join('、')}
                                {t.tools.length > 6 && ' 等'}
                            </span>
                        )}
                    </p>
                ) : (
                    <p className="text-2xs text-danger">{t.error || '连不上'}</p>
                )
            ) : null}
        </div>
    );
};

/** MCP 页顶：表单里每个服务（含没保存的草稿）的连接状态，外加用草稿试连一次 */
export const McpStatusPanel: React.FC<{
    instanceId: string;
    servers: readonly MaiBotMCPServerItemConfig[];
    status: MaiBotMcpStatus | undefined;
}> = ({ instanceId, servers, status }) => {
    if (!servers.length) return null;
    return (
        <div className="flex flex-col divide-y divide-border-subtle/70 rounded-md border border-border-subtle bg-inset/30 px-4 py-1">
            {servers.map((s, i) => (
                <McpServerRow
                    key={`${i}-${s.name}`}
                    instanceId={instanceId}
                    server={s}
                    live={status?.servers.find((x) => x.name === s.name)}
                    initialized={status?.initialized ?? false}
                />
            ))}
        </div>
    );
};

/** 列表里挑目标用：麦麦见过的群和私聊，按最近活跃排。挑了由调用方填成一条规则 */
export const ChatTargetPicker: React.FC<{
    sessions: readonly MaiBotChatSession[];
    onPick: (s: MaiBotChatSession) => void;
    disabled?: boolean;
}> = ({ sessions, onPick, disabled }) => {
    const [q, setQ] = useState('');
    const shown = sessions.filter(
        (s) => !q.trim() || s.display_name.includes(q.trim()) || s.target_id.includes(q.trim()),
    );
    return (
        <Popover onOpenChange={(open) => !open && setQ('')}>
            <PopoverTrigger asChild>
                <Button size="sm" variant="ghost" disabled={disabled || !sessions.length}>
                    <MessagesSquare size={13} />
                    从聊过的里选
                </Button>
            </PopoverTrigger>
            <PopoverContent align="end" sideOffset={6} className="w-80 p-1">
                <input
                    autoFocus
                    value={q}
                    onChange={(e) => setQ(e.target.value)}
                    placeholder="按名字或号码找"
                    className="mb-1 w-full rounded-sm border border-border-subtle bg-field px-2 py-1.5 text-xs text-text outline-none focus:border-brand"
                />
                <div className="overflow-y-auto overscroll-contain" style={SCROLL_STYLE}>
                    {shown.map((s) => (
                        <PopoverClose key={s.session_id} asChild>
                            <button
                                type="button"
                                className="flex w-full items-center gap-2 rounded-sm px-2 py-1.5 text-left hover:bg-inset"
                                onClick={() => onPick(s)}
                            >
                                <span className="shrink-0 rounded-xs bg-inset px-1 text-2xs text-text-tertiary">
                                    {s.chat_type === 'private' ? '私聊' : '群'}
                                </span>
                                <span className="min-w-0 flex-1 truncate text-xs text-text">{s.display_name || s.target_id}</span>
                                <span className="shrink-0 font-mono text-2xs text-text-tertiary">{s.target_id}</span>
                            </button>
                        </PopoverClose>
                    ))}
                    {!shown.length && <p className="px-2 py-2 text-xs text-text-tertiary">没有对得上的</p>}
                </div>
            </PopoverContent>
        </Popover>
    );
};
