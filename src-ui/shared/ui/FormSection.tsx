// 表单分组：与设置页 SettingsSection 同款小节标题（品牌竖条 + 左侧引导线）。
//
// 内部仍用 vertical / grid-2 排字段；连接列表等复杂块也包在这一层里。
// 父级 Tab 决定整体 padding，本组件不画外框卡片。

import { Children, forwardRef, type HTMLAttributes, type ReactNode } from 'react';
import { cn } from '../utils/cn';
import { ExpandPresence } from './motion';

export interface FormSectionProps extends Omit<HTMLAttributes<HTMLElement>, 'title'> {
    title?: ReactNode;
    description?: ReactNode;
    /** 标题行右侧（计数、快捷操作等）。 */
    actions?: ReactNode;
    /**
     * children 排版。default vertical；grid-2 两列并排（如 QQ + 实例名）。
     */
    layout?: 'vertical' | 'grid-2' | 'none';
    /** 保留标题与操作，只展开内容；曾展开的编辑器在收起后保留草稿。 */
    contentVisible?: boolean;
}

export const FormSection = forwardRef<HTMLElement, FormSectionProps>(
    (
        {
            title,
            description,
            actions,
            layout = 'vertical',
            contentVisible,
            children,
            className,
            ...rest
        },
        ref,
    ) => {
        const hasHeader = !!(title || actions);
        const content = Children.toArray(children).length > 0 && (
            <div className="box-border min-w-0 w-full max-w-full border-l border-border-subtle/80 pl-4 sm:pl-5">
                <div
                    className={cn(
                        layout === 'vertical' && 'flex min-w-0 w-full flex-col gap-3',
                        layout === 'grid-2' &&
                            'grid min-w-0 w-full grid-cols-1 gap-3 sm:grid-cols-2',
                        layout === 'none' && 'min-w-0 w-full max-w-full',
                    )}
                >
                    {children}
                </div>
            </div>
        );

        return (
            <section
                ref={ref}
                className={cn('min-w-0', contentVisible === undefined && 'space-y-4', className)}
                {...rest}
            >
                {hasHeader && (
                    <div className="space-y-1.5">
                        <div className="flex flex-wrap items-center gap-x-2.5 gap-y-2">
                            <span
                                className="h-3.5 w-0.5 shrink-0 rounded-full bg-brand/45"
                                aria-hidden
                            />
                            {title && (
                                <h2 className="min-w-0 max-w-full flex-[1_1_10rem] text-[13.5px] font-semibold leading-none tracking-tight text-text">
                                    {title}
                                </h2>
                            )}
                            {!title && actions && <div className="min-w-0 flex-1" />}
                            {actions && (
                                <div className="flex min-w-0 max-w-full flex-wrap items-center gap-2">
                                    {actions}
                                </div>
                            )}
                        </div>
                        {description && (
                            <p className="pl-3 text-[12px] leading-relaxed text-text-tertiary">
                                {description}
                            </p>
                        )}
                    </div>
                )}
                {/* 间距随内容一起收起，只有标题时不留引导线或空白。 */}
                {contentVisible === undefined ? (
                    content
                ) : (
                    <ExpandPresence visible={contentVisible} keepMounted>
                        <div className={hasHeader && content ? 'pt-4' : undefined}>{content}</div>
                    </ExpandPresence>
                )}
            </section>
        );
    },
);
FormSection.displayName = 'FormSection';
