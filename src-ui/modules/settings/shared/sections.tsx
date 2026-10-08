// 设置 Tab 的分组骨架：Tab 内多分组容器 + 单个分组 + 标准设置行。

import type { ReactNode } from 'react';

/** Tab 内多个分组：组间大留白，不用横线切（避免和行内分隔叠在一起显得乱）。 */
export function SettingsTabSections({ children }: { children: ReactNode }) {
    return <div className="flex w-full flex-col gap-14">{children}</div>;
}

function SettingsSectionHeader({
    title,
    description,
}: {
    title: ReactNode;
    description?: ReactNode;
}) {
    return (
        <div className="space-y-1.5">
            <div className="flex items-center gap-2.5">
                <span className="h-3.5 w-0.5 shrink-0 rounded-full bg-brand/45" aria-hidden />
                <h2 className="text-[13.5px] font-semibold leading-none tracking-tight text-text">
                    {title}
                </h2>
            </div>
            {description && (
                <p className="pl-3 text-[12px] leading-relaxed text-text-tertiary">{description}</p>
            )}
        </div>
    );
}

/** Tab 内分组：子标题（带轻标记）+ 左侧引导线下的平铺字段。 */
export function SettingsSection({
    title,
    description,
    children,
    layout = 'fields',
}: {
    title: ReactNode;
    description?: ReactNode;
    children: ReactNode;
    /** fields：左引导线 + FieldRow 分隔；panel：全宽内容区（日志/大面板，勿套竖线）。 */
    layout?: 'fields' | 'panel';
}) {
    return (
        <section className="space-y-4">
            <SettingsSectionHeader title={title} description={description} />
            {layout === 'panel' ? (
                <div className="min-w-0">{children}</div>
            ) : (
                <div className="border-l border-border-subtle/80 pl-4 sm:pl-5">
                    <div className="flex flex-col divide-y divide-border-subtle/70">{children}</div>
                </div>
            )}
        </section>
    );
}

/// 标准设置行。组内行间分隔由 SettingsSection 内 divide-y 统一处理，勿再叠 border-b。
/// layout="inline"（默认）：左标签右控件；layout="stacked"：标签在上、内容全宽在下。
export function FieldRow({
    label,
    description,
    isLast: _isLast,
    layout = 'inline',
    children,
}: {
    label: string;
    description?: ReactNode;
    /** 保留以兼容调用方；组内最后一行由 divide-y 自然收尾，无需再传。 */
    isLast?: boolean;
    /** inline: 左标签右控件（默认）；stacked: 标签在上、内容全宽在下。 */
    layout?: 'inline' | 'stacked';
    children?: ReactNode;
}) {
    if (layout === 'stacked') {
        return (
            <div className="flex flex-col gap-2 py-5 first:pt-1 last:pb-1">
                <div className="space-y-1">
                    <label className="block text-[13px] font-medium leading-snug text-text">
                        {label}
                    </label>
                    {description && (
                        <p className="text-[12px] leading-relaxed text-text-tertiary">
                            {description}
                        </p>
                    )}
                </div>
                {children && <div>{children}</div>}
            </div>
        );
    }
    return (
        <div className="flex items-center justify-between gap-6 py-5 first:pt-1 last:pb-1">
            <div className="min-w-0 flex-1 space-y-1">
                <label className="block text-[13px] font-medium leading-snug text-text">
                    {label}
                </label>
                {description && (
                    <p className="text-[12px] leading-relaxed text-text-tertiary">{description}</p>
                )}
            </div>
            {children && <div className="flex shrink-0 items-center gap-2">{children}</div>}
        </div>
    );
}
