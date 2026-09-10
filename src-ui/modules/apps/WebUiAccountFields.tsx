// 账号密码类 WebUI 的用户名 / 密码只读行：复制、显隐；密码未知或已过期时给出去向。

import { useCallback, useState } from 'react';
import { Check, Copy, Eye, EyeOff } from 'lucide-react';
import { Button } from '../../shared/ui';
import { cn } from '../../shared/utils/cn';
import type { AppWebUiAccount } from '../../core/ipc/types';

async function copyText(text: string): Promise<boolean> {
    try {
        await navigator.clipboard.writeText(text);
        return true;
    } catch {
        return false;
    }
}

const CopyButton: React.FC<{ text: string; label: string }> = ({ text, label }) => {
    const [copied, setCopied] = useState(false);
    const onCopy = useCallback(async () => {
        if (await copyText(text)) {
            setCopied(true);
            window.setTimeout(() => setCopied(false), 1600);
        }
    }, [text]);
    return (
        <Button
            type="button"
            size="icon"
            variant="ghost"
            className="h-7 w-7 shrink-0"
            aria-label={copied ? '已复制' : label}
            onClick={() => void onCopy()}
        >
            {copied ? <Check size={13} /> : <Copy size={13} />}
        </Button>
    );
};

const ValueRow: React.FC<{
    label: string;
    value: string;
    secret?: boolean;
    placeholder?: string;
}> = ({ label, value, secret = false, placeholder }) => {
    const [shown, setShown] = useState(!secret);
    const display = value ? (shown ? value : '•'.repeat(Math.min(value.length, 16))) : placeholder ?? '—';
    return (
        <div className="flex items-center gap-2">
            <span className="w-14 shrink-0 text-xs text-text-secondary">{label}</span>
            <code
                className={cn(
                    'min-w-0 flex-1 truncate rounded-sm bg-inset/80 px-2 py-1 font-mono text-xs',
                    value ? 'text-text' : 'text-text-tertiary',
                )}
                title={shown ? value : undefined}
            >
                {display}
            </code>
            {secret && value && (
                <Button
                    type="button"
                    size="icon"
                    variant="ghost"
                    className="h-7 w-7 shrink-0"
                    aria-label={shown ? '隐藏密码' : '显示密码'}
                    onClick={() => setShown((s) => !s)}
                >
                    {shown ? <EyeOff size={13} /> : <Eye size={13} />}
                </Button>
            )}
            {value && <CopyButton text={value} label={`复制${label}`} />}
        </div>
    );
};

export function webUiPasswordNote(account: AppWebUiAccount): string | null {
    if (!account.password) {
        return '密码只有桌面端设过才知道；忘了就停止实例后重置。';
    }
    if (account.password_matches === false) {
        return '这个密码已经不是当前密码（可能在 WebUI 里改过）；停止实例后可重置。';
    }
    return null;
}

export const WebUiAccountFields: React.FC<{ account: AppWebUiAccount; className?: string }> = ({
    account,
    className,
}) => {
    const note = webUiPasswordNote(account);
    return (
        <div className={cn('flex flex-col gap-2', className)}>
            <ValueRow label="用户名" value={account.username} placeholder="astrbot" />
            <ValueRow label="密码" value={account.password ?? ''} secret placeholder="未知" />
            {note && (
                <p
                    className={cn(
                        'text-2xs leading-snug',
                        account.password_matches === false ? 'text-warning' : 'text-text-tertiary',
                    )}
                >
                    {note}
                </p>
            )}
        </div>
    );
};
