// 右栏各处共用的小零件：出错只坏一条的边界、复制并提示「已复制」、头像、小图标按钮。

import {
    Component,
    forwardRef,
    useCallback,
    useEffect,
    useRef,
    useState,
    type ButtonHTMLAttributes,
    type ReactNode,
} from 'react';
import { cn } from '../utils/cn';
import { Tooltip, TooltipContent, TooltipTrigger } from '../ui/Tooltip';
import { avatarUrl, initialOf } from '../../core/domain/debug/chatFormat';
import { CACHE_MAX, ExpiringSet, FAILURE_TTL_MS } from './boundedCache';

// ---------------------------------------------------------------------------
// 坏数据只坏一条
// ---------------------------------------------------------------------------

interface SafeProps {
    /** 出错时显示什么；不给就是一小行「这条显示不了」 */
    fallback?: ReactNode;
    children: ReactNode;
}

/**
 * 上游事件的形状我们管不了：某一条（或某个消息段）渲染时抛错，只把它自己换成一行提示，
 * 不让整栏的错误边界把聊天全换掉。
 */
export class SafeBoundary extends Component<SafeProps, { failed: boolean }> {
    state = { failed: false };

    static getDerivedStateFromError(): { failed: boolean } {
        return { failed: true };
    }

    componentDidCatch(error: unknown): void {
        console.error('[debug] 聊天条目渲染失败:', error);
    }

    render(): ReactNode {
        if (this.state.failed) {
            return (
                this.props.fallback ?? (
                    <span className="text-2xs text-text-tertiary">[这条显示不了]</span>
                )
            );
        }
        return this.props.children;
    }
}

// ---------------------------------------------------------------------------
// 复制
// ---------------------------------------------------------------------------

/** 复制文字；成功后 `copied` 在 1.5 秒内等于传进来的 tag，按钮据此换成对勾 */
export function useCopy(): { copied: string | null; copy: (text: string, tag?: string) => void } {
    const [copied, setCopied] = useState<string | null>(null);
    const timer = useRef<number | null>(null);
    useEffect(
        () => () => {
            if (timer.current !== null) window.clearTimeout(timer.current);
        },
        [],
    );
    const copy = useCallback((text: string, tag = 'default') => {
        const done = () => {
            setCopied(tag);
            if (timer.current !== null) window.clearTimeout(timer.current);
            timer.current = window.setTimeout(() => setCopied(null), 1500);
        };
        try {
            void navigator.clipboard.writeText(text).then(done, () => {
                // webview 没有剪贴板权限时静默失败，和 JsonTree 的复制按钮一致
            });
        } catch {
            // 同上
        }
    }, []);
    return { copied, copy };
}

// ---------------------------------------------------------------------------
// 头像
// ---------------------------------------------------------------------------

// 拉失败过的号记住：虚拟列表的行卸了又挂，别每次都重新请求一遍、先闪一下图裂。
// 有上限，5 分钟后过期再试
const failedAvatars = new ExpiringSet(CACHE_MAX, FAILURE_TTL_MS);

export function Avatar({
    id,
    name,
    mine,
    className,
}: {
    id: number;
    name: string;
    mine?: boolean;
    className?: string;
}) {
    const url = avatarUrl(id);
    const [failed, setFailed] = useState(() => url === null || failedAvatars.has(String(id)));
    return (
        <span
            aria-hidden
            className={cn(
                'relative inline-flex h-8 w-8 shrink-0 select-none items-center justify-center overflow-hidden rounded-full text-[12px] font-medium ring-1 ring-border-subtle',
                mine ? 'bg-brand-soft text-brand' : 'bg-inset text-text-secondary',
                className,
            )}
        >
            {/* 首字一直垫在底下：图没到、图裂了都有东西看，而且尺寸固定，行高不会跳 */}
            {initialOf(name)}
            {!failed && url && (
                <img
                    src={url}
                    alt=""
                    loading="lazy"
                    decoding="async"
                    referrerPolicy="no-referrer"
                    draggable={false}
                    className="absolute inset-0 h-full w-full object-cover"
                    onError={() => {
                        failedAvatars.add(String(id));
                        setFailed(true);
                    }}
                />
            )}
        </span>
    );
}

// ---------------------------------------------------------------------------
// 小图标按钮（悬停工具条、工具栏）
// ---------------------------------------------------------------------------

export interface IconActionProps extends ButtonHTMLAttributes<HTMLButtonElement> {
    label: string;
    /** 悬停提示；不给就用 label */
    tip?: ReactNode;
    active?: boolean;
    tone?: 'default' | 'danger';
}

export const IconAction = forwardRef<HTMLButtonElement, IconActionProps>(function IconAction(
    { label, tip, active, tone = 'default', className, children, ...rest },
    ref,
) {
    return (
        <Tooltip>
            <TooltipTrigger asChild>
                <button
                    ref={ref}
                    type="button"
                    aria-label={label}
                    className={cn(
                        'inline-flex h-7 w-7 shrink-0 items-center justify-center rounded-sm transition-colors',
                        'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand',
                        'disabled:cursor-not-allowed disabled:opacity-45',
                        active
                            ? 'bg-brand-soft text-brand'
                            : tone === 'danger'
                              ? 'text-text-tertiary hover:bg-danger-soft hover:text-danger'
                              : 'text-text-tertiary hover:bg-inset hover:text-text',
                        className,
                    )}
                    {...rest}
                >
                    {children}
                </button>
            </TooltipTrigger>
            <TooltipContent side="bottom">{tip ?? label}</TooltipContent>
        </Tooltip>
    );
});
