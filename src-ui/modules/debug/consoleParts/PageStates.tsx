// 调试台三种「还不能用」的整页占位：Bot 列表读挂了、一台 Bot 都还没有、还在打开。
import { Bot, FlaskConical, RefreshCw } from 'lucide-react';
import { cn } from '../../../shared/utils/cn';
import { Button, PagePlaceholder, Spinner } from '../../../shared/ui';
import type { AppRoute } from '../../../shared/components/next/Sidebar';

function EmptyIcon({ tone }: { tone: 'brand' | 'danger' }) {
    return (
        <span
            className={cn(
                'inline-flex h-12 w-12 items-center justify-center rounded-lg',
                tone === 'brand' ? 'bg-brand-soft text-brand' : 'bg-danger-soft text-danger',
            )}
        >
            <FlaskConical size={22} strokeWidth={1.8} aria-hidden />
        </span>
    );
}

/** Bot 列表拉挂了：给出原因和重试出口 */
export function ConsoleTargetsError({
    message,
    onRetry,
}: {
    message?: string;
    onRetry: () => void;
}) {
    return (
        <PagePlaceholder>
            <EmptyIcon tone="danger" />
            <p className="font-display text-md font-semibold text-text">读不到 Bot 列表</p>
            <p className="max-w-sm text-xs text-text-secondary">{message}</p>
            <Button size="sm" variant="secondary" onClick={onRetry}>
                <RefreshCw size={13} aria-hidden />
                重试
            </Button>
        </PagePlaceholder>
    );
}

/** 还没有任何 Bot：把用户送去「机器人」页；没有导航出口（独立调试窗）就不画按钮 */
export function ConsoleNoTargets({ onNavigate }: { onNavigate?: (route: AppRoute) => void }) {
    return (
        <PagePlaceholder>
            <EmptyIcon tone="brand" />
            <p className="font-display text-md font-semibold text-text">还没有 Bot</p>
            <p className="max-w-sm text-xs leading-relaxed text-text-secondary">
                调试台对着 Bot 发请求、看事件。先到「机器人」页添加一个 NapCat 或 SnowLuma 的 Bot
                并启动，再回来这里。
            </p>
            {onNavigate && (
                <Button size="sm" variant="primary" onClick={() => onNavigate('bots')}>
                    <Bot size={14} aria-hidden />
                    去「机器人」页
                </Button>
            )}
        </PagePlaceholder>
    );
}

/** 工作区或 Bot 列表还没就绪 */
export function ConsoleLoading() {
    return (
        <PagePlaceholder>
            <Spinner size="md" tone="brand" label="正在打开调试台" />
            <p className="text-[13px] text-text-secondary">正在打开调试台…</p>
        </PagePlaceholder>
    );
}
