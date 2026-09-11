// AstrBot 各 Tab 共用的展示件：实体行、空态、药丸列表、跳转链接、编辑 / 删除对话框。
// 所有页统一「紧凑列表 + 对话框编辑」，页面上不摆一排排输入框。

import type { ComponentType, ReactNode } from 'react';
import { ArrowRight, Check, ChevronRight } from 'lucide-react';
import type { LucideProps } from 'lucide-react';
import {
    Button,
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
    Spinner,
    type DialogSize,
} from '../../../../shared/ui';
import { cn } from '../../../../shared/utils/cn';

export const TAB_LABEL: Record<string, string> = {
    connections: '连接',
    models: '模型',
    talk: '对话',
    persona: '人格',
    kb: '知识库',
    subagent: '子代理',
    rules: '规则',
};

/** 「去『模型』页」这类内联跳转。空态 / 占位文字里的死胡同都换成它。 */
export const JumpLink: React.FC<{
    tab: string;
    onGo: (tab: string) => void;
    children?: ReactNode;
    className?: string;
}> = ({ tab, onGo, children, className }) => (
    <button
        type="button"
        onClick={() => onGo(tab)}
        className={cn(
            'inline-flex items-center gap-0.5 rounded-xs text-brand underline-offset-2 hover:underline',
            'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand/40',
            className,
        )}
    >
        {children ?? `去「${TAB_LABEL[tab] ?? tab}」页`}
        <ArrowRight size={11} strokeWidth={2.2} />
    </button>
);

/** 服务端即时删除的二次确认。表单内的删除不用它——那能撤销。 */
export const ConfirmDelete: React.FC<{
    open: boolean;
    title: string;
    description?: ReactNode;
    busy?: boolean;
    confirmLabel?: string;
    onCancel: () => void;
    onConfirm: () => void;
}> = ({ open, title, description, busy, confirmLabel = '删除', onCancel, onConfirm }) => (
    <Dialog open={open} onOpenChange={(o) => !o && !busy && onCancel()}>
        <DialogContent size="sm" hideClose dismissOnOutsideClick={!busy}>
            <DialogHeader>
                <DialogTitle>{title}</DialogTitle>
                {description && <DialogDescription>{description}</DialogDescription>}
            </DialogHeader>
            <DialogFooter>
                <Button variant="ghost" size="sm" onClick={onCancel} disabled={busy}>
                    取消
                </Button>
                <Button variant="danger" size="sm" onClick={onConfirm} disabled={busy}>
                    {busy && <Spinner size="sm" className="text-white" />}
                    {confirmLabel}
                </Button>
            </DialogFooter>
        </DialogContent>
    </Dialog>
);

/**
 * 从已知集合里多选（备用模型、挂载知识库）。value 里有但 options 里没有的值照样显示，
 * 能取消不能新增——不让人凭空敲 id。
 */
export const PickList: React.FC<{
    label: ReactNode;
    options: readonly { value: string; label: ReactNode }[];
    value: readonly string[];
    onChange: (next: string[]) => void;
    disabled?: boolean;
    /** options 为空时显示什么 */
    empty?: ReactNode;
    hint?: ReactNode;
}> = ({ label, options, value, onChange, disabled, empty, hint }) => {
    const known = new Set(options.map((o) => o.value));
    const orphans = value.filter((v) => !known.has(v));
    const toggle = (v: string) =>
        onChange(value.includes(v) ? value.filter((x) => x !== v) : [...value, v]);
    return (
        <div className="flex flex-col gap-1.5">
            <span className="text-xs font-medium text-text-secondary">{label}</span>
            {options.length === 0 && orphans.length === 0 ? (
                <p className="text-xs text-text-tertiary">{empty}</p>
            ) : (
                <div className="flex flex-wrap gap-1.5">
                    {options.map((o) => {
                        const on = value.includes(o.value);
                        return (
                            <button
                                key={o.value}
                                type="button"
                                disabled={disabled}
                                aria-pressed={on}
                                onClick={() => toggle(o.value)}
                                className={cn(
                                    'inline-flex items-center gap-1 rounded-pill border px-2.5 py-0.5 font-mono text-2xs transition-colors disabled:opacity-50',
                                    on
                                        ? 'border-brand/40 bg-brand-soft text-brand'
                                        : 'border-border-subtle bg-surface text-text-secondary hover:border-border hover:text-text',
                                )}
                            >
                                {on && <Check size={10} strokeWidth={3} />}
                                {o.label}
                            </button>
                        );
                    })}
                    {orphans.map((v) => (
                        <button
                            key={v}
                            type="button"
                            disabled={disabled}
                            aria-pressed
                            title="当前配置里找不到这一项，点一下移除"
                            onClick={() => toggle(v)}
                            className="inline-flex items-center gap-1 rounded-pill border border-dashed border-warning/50 bg-warning-soft/40 px-2.5 py-0.5 font-mono text-2xs text-text-secondary line-through disabled:opacity-50"
                        >
                            {v}
                        </button>
                    ))}
                </div>
            )}
            {hint && <p className="text-xs text-text-tertiary">{hint}</p>}
        </div>
    );
};

/**
 * 列表行。左图标槽 + 标题行 + 副标题 + 右侧操作。
 * 给了 onOpen 整行可点（进对话框编辑）；操作区放在按钮外面，Switch / 删除不会连带触发打开。
 */
export const EntityRow: React.FC<{
    icon: ComponentType<LucideProps>;
    title: ReactNode;
    /** 标题右侧的徽章位 */
    tags?: ReactNode;
    subtitle?: ReactNode;
    /** 副标题用等宽（id / umo 这类技术值） */
    mono?: boolean;
    /** 副标题下再来一行（模型药丸这类） */
    extra?: ReactNode;
    actions?: ReactNode;
    onOpen?: () => void;
    /** 整行变灰（停用的提供商） */
    muted?: boolean;
    className?: string;
}> = ({ icon: Icon, title, tags, subtitle, mono, extra, actions, onOpen, muted, className }) => {
    const body = (
        <>
            <span className="flex h-8 w-8 shrink-0 items-center justify-center rounded-md bg-inset text-text-tertiary">
                <Icon size={15} />
            </span>
            <span className="min-w-0 flex-1">
                <span className="flex min-w-0 items-center gap-2">
                    <span className="truncate text-[13px] font-medium text-text">{title}</span>
                    {tags}
                </span>
                {subtitle && (
                    <span className={cn('block truncate text-xs text-text-tertiary', mono && 'font-mono')}>
                        {subtitle}
                    </span>
                )}
                {extra && <span className="mt-1.5 block">{extra}</span>}
            </span>
        </>
    );
    return (
        <div
            className={cn(
                'group flex items-center gap-3 rounded-md border border-border-subtle bg-surface px-3 py-2.5',
                'transition-colors hover:border-border',
                onOpen && 'hover:bg-inset/30',
                muted && 'opacity-60 hover:opacity-100',
                className,
            )}
        >
            {onOpen ? (
                <button
                    type="button"
                    onClick={onOpen}
                    className="flex min-w-0 flex-1 items-center gap-3 rounded-sm text-left focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand/40"
                >
                    {body}
                </button>
            ) : (
                <div className="flex min-w-0 flex-1 items-center gap-3">{body}</div>
            )}
            {(actions || onOpen) && (
                <div className="flex shrink-0 items-center gap-0.5">
                    {actions}
                    {onOpen && (
                        <ChevronRight
                            size={14}
                            className="ml-1 text-text-disabled transition-colors group-hover:text-text-secondary"
                        />
                    )}
                </div>
            )}
        </div>
    );
};

/** 编辑一块草稿 / 一条实体的对话框。取消丢掉局部改动，确定才写回。 */
export const FormDialog: React.FC<{
    open: boolean;
    title: ReactNode;
    description?: ReactNode;
    size?: DialogSize;
    confirmLabel?: string;
    confirmDisabled?: boolean;
    busy?: boolean;
    /** 头部右侧（启用开关这类整块的总闸） */
    headerActions?: ReactNode;
    onCancel: () => void;
    onConfirm: () => void;
    children: ReactNode;
}> = ({
    open,
    title,
    description,
    size = 'md',
    confirmLabel = '确定',
    confirmDisabled,
    busy,
    headerActions,
    onCancel,
    onConfirm,
    children,
}) => (
    <Dialog open={open} onOpenChange={(o) => !o && !busy && onCancel()}>
        <DialogContent size={size} hideClose dismissOnOutsideClick={false}>
            <DialogHeader className={headerActions ? 'flex-row items-start justify-between gap-4' : undefined}>
                <div className="flex min-w-0 flex-col gap-1">
                    <DialogTitle>{title}</DialogTitle>
                    {description && <DialogDescription>{description}</DialogDescription>}
                </div>
                {headerActions && <div className="shrink-0">{headerActions}</div>}
            </DialogHeader>
            <div className="flex flex-col gap-3">{children}</div>
            <DialogFooter>
                <Button variant="ghost" size="sm" onClick={onCancel} disabled={busy}>
                    取消
                </Button>
                <Button variant="primary" size="sm" onClick={onConfirm} disabled={confirmDisabled || busy}>
                    {busy && <Spinner size="sm" className="text-white" />}
                    {confirmLabel}
                </Button>
            </DialogFooter>
        </DialogContent>
    </Dialog>
);

/** 列表为空时的占位。虚线框跟实心的实体行区分开，一眼能看出「这里还没东西」。 */
export const EmptyHint: React.FC<{
    icon: ComponentType<LucideProps>;
    title: ReactNode;
    action?: ReactNode;
}> = ({ icon: Icon, title, action }) => (
    <div className="flex flex-col items-center gap-2.5 rounded-md border border-dashed border-border-subtle px-6 py-8 text-center">
        <Icon size={22} className="text-text-disabled" strokeWidth={1.75} />
        <p className="text-[13px] text-text-secondary">{title}</p>
        {action}
    </div>
);

/** 只读的等宽药丸，用于工具名、拉到的模型名这类枚举值。 */
export const Pill: React.FC<{ children: ReactNode; className?: string }> = ({
    children,
    className,
}) => (
    <span
        className={cn(
            'inline-flex items-center rounded-pill bg-inset px-2 py-0.5 font-mono text-2xs text-text-secondary',
            className,
        )}
    >
        {children}
    </span>
);

/** 卡片内的次级分区标题，比 FormSection 轻一档。 */
export const SubHeader: React.FC<{ title: ReactNode; actions?: ReactNode }> = ({
    title,
    actions,
}) => (
    <div className="flex items-center justify-between gap-3">
        <p className="text-2xs font-medium uppercase tracking-wider text-text-tertiary">{title}</p>
        {actions && <div className="flex shrink-0 items-center gap-1.5">{actions}</div>}
    </div>
);
