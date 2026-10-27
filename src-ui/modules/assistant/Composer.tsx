// 新建页与会话共用的输入卡片：Enter 发送，输入法组字和 Shift+Enter 不发送。

import type { ReactNode } from 'react';
import { TextAreaField } from '../../shared/ui';

interface Props {
    label: string;
    value: string;
    placeholder: string;
    disabled?: boolean;
    autoFocus?: boolean;
    notice?: ReactNode;
    tools?: ReactNode;
    status?: ReactNode;
    action: ReactNode;
    onChange: (value: string) => void;
    onSubmit: () => void;
}

export function Composer({
    label,
    value,
    placeholder,
    disabled,
    autoFocus,
    notice,
    tools,
    status,
    action,
    onChange,
    onSubmit,
}: Props) {
    return (
        <div className="rounded-2xl border border-border bg-surface shadow-sm transition-[border-color,box-shadow] focus-within:border-brand/50 focus-within:ring-[3px] focus-within:ring-brand/10">
            {notice && (
                <div className="border-b border-border-subtle px-4 py-2.5 text-xs leading-5">
                    {notice}
                </div>
            )}
            <TextAreaField
                aria-label={label}
                className="px-1.5 pt-1.5 [&_textarea]:border-0 [&_textarea]:bg-transparent [&_textarea]:focus:ring-0"
                placeholder={placeholder}
                minRows={2}
                maxRows={8}
                maxLength={16_000}
                autoFocus={autoFocus}
                value={value}
                disabled={disabled}
                onValueChange={onChange}
                onKeyDown={(event) => {
                    if (
                        event.key === 'Enter' &&
                        !event.shiftKey &&
                        !event.nativeEvent.isComposing &&
                        event.keyCode !== 229
                    ) {
                        event.preventDefault();
                        onSubmit();
                    }
                }}
            />
            <div className="flex min-h-11 flex-wrap items-center gap-1 px-2 pb-2">
                {tools && <div className="flex min-w-0 flex-1 items-center gap-1">{tools}</div>}
                <div
                    className="min-w-0 flex-1 px-1 text-2xs leading-5 text-text-tertiary"
                    aria-live="polite"
                >
                    {status}
                </div>
                <div className="flex shrink-0 items-center">{action}</div>
            </div>
        </div>
    );
}
