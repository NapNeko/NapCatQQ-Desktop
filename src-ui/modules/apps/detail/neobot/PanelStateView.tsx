// 面板页的共用外壳：四态（加载 / 不支持 / 没凭据 / 打不通 / 失败 / 形状不认识）统一成一块提示，
// 只把成功状态交给各页渲染。
//
// 为什么值得共用：每一态都对应一句**能照做的话**，而凭据卡片在「Web 控制台」页——
// 提示里必须说清去哪、并给一个直接跳过去的按钮，而不是让用户自己在侧栏找。

import type { ReactNode } from 'react';
import { Button } from '../../../../shared/ui';
import type { PanelState } from './useNeoBotPanel';

interface BlockedHint {
    title: string;
    body: string;
    /** 有的话给一个跳转按钮（例如「去填面板密码」） */
    action?: { label: string; tab: string };
}

/** 面板没给可用数据时，按状态给一句能照做的话 */
export function blockedHint(state: PanelState<unknown>): BlockedHint | null {
    switch (state.kind) {
        case 'unsupported':
            return {
                title: '该框架不支持面板转发',
                body: '这个框架没有可由桌面端读取的控制台接口，或用的是另一套对接方式。',
            };
        case 'unauthorized':
            return {
                title: '先填面板密码',
                // 凭据卡片在「Web 控制台」页，不在本页——指路，别说「上面」
                body: '面板接口要登录才能读。到「Web 控制台」页看「面板凭据」那一栏——它会先探面板的登录状态，告诉你是「还没设密码」还是「把密码填进来」。',
                action: { label: '去填面板密码', tab: 'console' },
            };
        case 'unreachable':
            return {
                title: '面板打不通',
                body: '实例可能没在运行，或面板端口与桌面端读到的不一致。先确认实例是运行中，再点刷新。',
            };
        case 'failed':
            return { title: '面板返回了错误', body: state.message || '面板拒绝了这次请求。' };
        case 'malformed':
            return {
                title: '面板的返回不认识',
                body: '面板答了，但结构不是桌面端预期的样子——多半是面板改版了，桌面端这边要跟着更新。',
            };
        default:
            return null;
    }
}

const Hint: React.FC<{ title: string; children: ReactNode }> = ({ title, children }) => (
    <section className="rounded-md border border-border-subtle bg-surface px-4 py-3">
        <h3 className="text-sm font-semibold text-text">{title}</h3>
        <p className="mt-1 text-xs leading-relaxed text-text-secondary">{children}</p>
    </section>
);

interface PanelStateViewProps<T> {
    state: PanelState<T> | undefined;
    isError: boolean;
    errorMessage?: string;
    onRetry: () => void;
    onGoTab: (tab: string) => void;
    children: (data: T) => ReactNode;
}

/** 把四态渲染掉；成功时把数据交给 children */
export function PanelStateView<T>({
    state,
    isError,
    errorMessage,
    onRetry,
    onGoTab,
    children,
}: PanelStateViewProps<T>) {
    if (isError) {
        return (
            <Hint title="读取失败">
                {errorMessage}
                <Button className="ml-2" size="sm" variant="secondary" onClick={onRetry}>
                    重试
                </Button>
            </Hint>
        );
    }
    if (!state) return <p className="text-xs text-text-tertiary">正在读取面板…</p>;
    if (state.kind === 'ok') return <>{children(state.data)}</>;

    const hint = blockedHint(state as PanelState<unknown>);
    if (!hint) return null;
    return (
        <Hint title={hint.title}>
            {hint.body}
            {hint.action && (
                <Button
                    className="ml-2"
                    size="sm"
                    variant="primary"
                    onClick={() => onGoTab(hint.action!.tab)}
                >
                    {hint.action.label}
                </Button>
            )}
            <Button className="ml-2" size="sm" variant="secondary" onClick={onRetry}>
                刷新
            </Button>
        </Hint>
    );
}