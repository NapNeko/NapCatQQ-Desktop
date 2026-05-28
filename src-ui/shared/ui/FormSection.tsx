// 表单分组：标题 + 副标题 + 字段内容。
//
// 视觉设计哲学：在长表单里不画分组容器，也不画分隔线。段落之间靠固定间距
// 区分；标题靠字重 + 字号区分。这样整个表单读起来是连贯的长文档，而不是
// 一堆"卡中卡"或"被 hairline 切碎"的独立块。
//
// 父级 (Tab content) 已经决定整体 padding / 大卡背景，FormSection 只管自身
// 内部排版，不持有自己的边界。

import { forwardRef, type HTMLAttributes, type ReactNode } from 'react';
import { cn } from '../utils/cn';

export interface FormSectionProps extends Omit<HTMLAttributes<HTMLElement>, 'title'> {
    title?: ReactNode;
    description?: ReactNode;
    /** 标题右侧动作位（如计数徽章 / "全选" 按钮）。 */
    actions?: ReactNode;
    /**
     * children 排版方式。default vertical 适合 90% 字段；
     * 'grid-2' 让两列字段并排（QQID + 实例名）。
     */
    layout?: 'vertical' | 'grid-2' | 'none';
}

export const FormSection = forwardRef<HTMLElement, FormSectionProps>(
    (
        { title, description, actions, layout = 'vertical', children, className, ...rest },
        ref,
    ) => (
        <section
            ref={ref}
            className={cn('flex flex-col gap-3', className)}
            {...rest}
        >
            {(title || actions) && (
                <header className="flex items-baseline justify-between gap-3">
                    <div className="flex min-w-0 flex-col gap-0.5">
                        {title && (
                            <h3 className="font-display text-sm font-semibold text-text">
                                {title}
                            </h3>
                        )}
                        {description && (
                            <p className="text-2xs text-text-tertiary leading-snug">
                                {description}
                            </p>
                        )}
                    </div>
                    {actions && (
                        <div className="flex shrink-0 items-center gap-2">{actions}</div>
                    )}
                </header>
            )}
            <div
                className={cn(
                    layout === 'vertical' && 'flex flex-col gap-3',
                    layout === 'grid-2' && 'grid grid-cols-1 gap-3 sm:grid-cols-2',
                    layout === 'none' && '',
                )}
            >
                {children}
            </div>
        </section>
    ),
);
FormSection.displayName = 'FormSection';
