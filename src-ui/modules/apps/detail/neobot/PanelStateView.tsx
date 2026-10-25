// 区分整页不可用和局部读取失败，提供对应的恢复入口。

import type { ReactNode } from 'react';
import { AlertCircle, Box, KeyRound, RotateCw, Unplug } from 'lucide-react';
import { Button, PagePlaceholder, Spinner } from '../../../../shared/ui';
import { versionRequirementText } from '../../../../core/domain/apps/neobotCapabilities';
import type { PanelPageGate, PanelState } from '../../../../core/domain/apps/neobotPanelState';
import { useNeoBotPanelAccess } from '../../../../hooks/apps/neoBotPanelContext';

interface GateAction {
    label: string;
    action: 'goTab' | 'retry' | 'webui';
    tab?: string;
}

interface GateLook {
    icon: typeof KeyRound;
    well: string;
    title: string;
    body: ReactNode;
    primary?: GateAction;
    secondary?: GateAction;
}

function gateLook(gate: PanelPageGate, access: ReturnType<typeof useNeoBotPanelAccess>): GateLook {
    switch (gate.kind) {
        case 'unauthorized': {
            if (access && !access.loginSupported) {
                return {
                    icon: KeyRound,
                    well: 'bg-warning-soft text-warning',
                    title: '当前版本不支持桌面端登录',
                    body:
                        versionRequirementText(access.instance.installed_version, 'panelLogin') +
                        '。可通过浏览器使用面板。',
                    primary: { label: '打开控制台', action: 'webui' },
                    secondary: { label: '查看面板凭据', action: 'goTab', tab: 'console' },
                };
            }
            if (access?.passwordRemembered) {
                return {
                    icon: KeyRound,
                    well: 'bg-warning-soft text-warning',
                    title: '面板密码已失效',
                    body: '已保存的密码未通过验证，请重新填写当前面板密码。',
                    primary: { label: '更新面板密码', action: 'goTab', tab: 'console' },
                    secondary: { label: '打开控制台', action: 'webui' },
                };
            }
            return {
                icon: KeyRound,
                well: 'bg-warning-soft text-warning',
                title: '需要面板密码',
                body: '保存面板密码后即可读取数据，密码仅保存在本机密钥库。',
                primary: { label: '填写面板密码', action: 'goTab', tab: 'console' },
                secondary: { label: '打开控制台', action: 'webui' },
            };
        }
        case 'unreachable':
            return {
                icon: Unplug,
                well: 'bg-inset text-text-tertiary',
                title: '面板没有响应',
                body: '确认实例已启动，面板就绪后重试。',
                primary: { label: '刷新', action: 'retry' },
                secondary: { label: '打开控制台', action: 'webui' },
            };
        case 'unsupported':
            return {
                icon: Box,
                well: 'bg-inset text-text-tertiary',
                title: '当前实例不支持面板读取',
                body: '可通过浏览器打开控制台。',
                primary: { label: '打开控制台', action: 'webui' },
            };
        default:
            return {
                icon: AlertCircle,
                well: 'bg-danger-soft text-danger',
                title: '无法读取面板',
                body: gate.message || '重试或打开控制台查看运行状态。',
                primary: { label: '刷新', action: 'retry' },
                secondary: { label: '打开控制台', action: 'webui' },
            };
    }
}

export function PanelGateView({
    gate,
    onRetry,
    onGoTab,
}: {
    gate: PanelPageGate;
    onRetry: () => void;
    onGoTab?: (tab: string) => void;
}) {
    const access = useNeoBotPanelAccess();

    if (gate.kind === 'loading') {
        return (
            <PagePlaceholder className="gap-2.5 py-12">
                <Spinner size="sm" tone="muted" />
                <p className="text-xs text-text-tertiary">正在读取面板…</p>
            </PagePlaceholder>
        );
    }

    const look = gateLook(gate, access);
    const Icon = look.icon;

    const run = (action: GateAction) => {
        if (action.action === 'retry') return onRetry;
        if (action.action === 'webui') return access ? access.onOpenWebUi : undefined;
        const navigate = access?.onGoTab ?? onGoTab;
        return navigate && action.tab ? () => navigate(action.tab!) : undefined;
    };
    const primaryRun = look.primary ? run(look.primary) : undefined;
    const secondaryRun = look.secondary ? run(look.secondary) : undefined;
    const canShow = !!(look.primary && primaryRun) || !!(look.secondary && secondaryRun);

    return (
        <PagePlaceholder className="gap-4 py-12">
            <span
                className={`flex h-11 w-11 items-center justify-center rounded-lg ${look.well}`}
                aria-hidden
            >
                <Icon size={20} strokeWidth={1.8} />
            </span>
            <div className="flex flex-col gap-1.5">
                <p className="text-sm font-medium text-text">{look.title}</p>
                <p className="max-w-[23rem] text-xs leading-relaxed text-text-tertiary">
                    {look.body}
                </p>
            </div>
            {canShow && (
                <div className="flex flex-wrap items-center justify-center gap-2">
                    {look.primary && primaryRun && (
                        <Button size="sm" variant="primary" onClick={primaryRun}>
                            {look.primary.action === 'retry' && (
                                <RotateCw size={13} strokeWidth={2.1} />
                            )}
                            {look.primary.label}
                        </Button>
                    )}
                    {look.secondary && secondaryRun && (
                        <Button size="sm" variant="ghost" onClick={secondaryRun}>
                            {look.secondary.label}
                        </Button>
                    )}
                </div>
            )}
        </PagePlaceholder>
    );
}

interface SectionHint {
    tone: 'warning' | 'danger' | 'neutral';
    title: string;
    body: string;
    action?: { label: string; tab: string };
}

function sectionHint(state: PanelState<unknown>, notFoundHint?: string): SectionHint | null {
    switch (state.kind) {
        case 'unauthorized':
            return {
                tone: 'warning',
                title: '需要面板密码',
                body: '登录后可读取此项数据。',
                action: { label: '填写密码', tab: 'console' },
            };
        case 'unreachable':
            return { tone: 'neutral', title: '面板没有响应', body: '确认实例在运行后重试。' };
        case 'notFound':
            return {
                tone: 'neutral',
                title: '当前版本不支持此项数据',
                body: notFoundHint || state.message || '更新 NeoBot 后重试。',
            };
        case 'malformed':
            return {
                tone: 'neutral',
                title: '无法解析面板数据',
                body: '面板返回格式与当前桌面端不兼容。',
            };
        case 'failed':
            return {
                tone: 'danger',
                title: '面板返回了错误',
                body: state.message || '重试或打开控制台查看详情。',
            };
        case 'unsupported':
            return { tone: 'neutral', title: '不支持此项数据', body: '可在上游控制台查看。' };
        default:
            return null;
    }
}

const HINT_TONE: Record<SectionHint['tone'], string> = {
    warning: 'border-warning/20 bg-warning-soft/40',
    danger: 'border-danger/20 bg-danger-soft/40',
    neutral: 'border-border-subtle bg-inset/50',
};

const HINT_ICON: Record<SectionHint['tone'], string> = {
    warning: 'text-warning',
    danger: 'text-danger',
    neutral: 'text-text-tertiary',
};

export function PanelSectionState({
    state,
    onRetry,
    notFoundHint,
    bare,
}: {
    state: PanelState<unknown> | undefined;
    onRetry?: () => void;
    notFoundHint?: string;
    /** 不要外框（已经在别的提示里） */
    bare?: boolean;
}) {
    const access = useNeoBotPanelAccess();
    if (!state) {
        return (
            <div className="flex items-center gap-2 py-0.5 text-xs text-text-tertiary">
                <Spinner size="xs" tone="muted" />
                正在读取…
            </div>
        );
    }
    if (state.kind === 'ok') return null;
    const hint = sectionHint(state, notFoundHint);
    if (!hint) return null;
    const goTab = hint.action && access ? () => access.onGoTab(hint.action!.tab) : undefined;

    return (
        <div
            className={
                bare
                    ? 'flex items-start gap-2 py-0.5'
                    : `flex items-start gap-2 rounded-sm border px-3 py-2 ${HINT_TONE[hint.tone]}`
            }
        >
            <AlertCircle
                size={13}
                strokeWidth={2}
                className={`mt-0.5 shrink-0 ${HINT_ICON[hint.tone]}`}
            />
            <div className="min-w-0 flex-1">
                <p className="text-xs font-medium text-text">{hint.title}</p>
                <p className="mt-0.5 text-2xs leading-relaxed text-text-tertiary">{hint.body}</p>
            </div>
            <div className="flex shrink-0 items-center gap-1">
                {goTab && hint.action && (
                    <Button size="sm" variant="ghost" className="h-6 px-2 text-2xs" onClick={goTab}>
                        {hint.action.label}
                    </Button>
                )}
                {onRetry && (
                    <Button
                        size="sm"
                        variant="ghost"
                        className="h-6 px-2 text-2xs"
                        onClick={onRetry}
                    >
                        重试
                    </Button>
                )}
            </div>
        </div>
    );
}

interface PanelStateViewProps<T> {
    state: PanelState<T> | undefined;
    isError: boolean;
    errorMessage?: string;
    onRetry: () => void;
    onGoTab: (tab: string) => void;
    /** 「面板没有这个接口」时说的话（通常是「需要 NeoBot x.y.z 及以上」） */
    notFoundHint?: string;
    children: (data: T) => ReactNode;
}

export function PanelStateView<T>({
    state,
    isError,
    errorMessage,
    onRetry,
    onGoTab,
    notFoundHint,
    children,
}: PanelStateViewProps<T>) {
    if (state?.kind === 'ok') return <>{children(state.data)}</>;
    if (isError) {
        return (
            <PanelSectionState
                state={{ kind: 'failed', message: errorMessage ?? '读取失败' }}
                onRetry={onRetry}
            />
        );
    }
    if (!state)
        return (
            <PanelGateView
                gate={{ kind: 'loading', message: '', blocked: 0, total: 1 }}
                onRetry={onRetry}
                onGoTab={onGoTab}
            />
        );

    if (state.kind === 'notFound') {
        return <PanelSectionState state={state} onRetry={onRetry} notFoundHint={notFoundHint} />;
    }
    return (
        <PanelGateView
            gate={{
                kind:
                    state.kind === 'unsupported' ||
                    state.kind === 'unauthorized' ||
                    state.kind === 'unreachable'
                        ? state.kind
                        : 'unavailable',
                message: 'message' in state ? state.message : '',
                blocked: 1,
                total: 1,
            }}
            onRetry={onRetry}
            onGoTab={onGoTab}
        />
    );
}
