import { useCallback, useState } from 'react';
import { Check, Copy } from 'lucide-react';
import { cn } from '../utils/cn';

interface CopyCodeBlockProps {
    /** 单行或多行 shell 命令，展示在等宽块内。 */
    command: string;
    className?: string;
}

/** 可复制的命令块，用于 SSH 指纹核对等运维提示。 */
export function CopyCodeBlock({ command, className }: CopyCodeBlockProps) {
    const [copied, setCopied] = useState(false);

    const onCopy = useCallback(async () => {
        try {
            await navigator.clipboard.writeText(command);
            setCopied(true);
            window.setTimeout(() => setCopied(false), 2000);
        } catch {
            // webview 无 clipboard API 时静默失败
        }
    }, [command]);

    return (
        <div
            className={cn(
                'flex items-center gap-2 rounded-sm border border-border-subtle bg-inset/80 py-1 pl-2.5 pr-1',
                className,
            )}
        >
            <pre className="min-w-0 flex-1 overflow-x-auto whitespace-pre-wrap break-all font-mono text-xs leading-6 text-text">
                {command}
            </pre>
            <button
                type="button"
                onClick={() => void onCopy()}
                aria-label={copied ? '已复制' : '复制'}
                title={copied ? '已复制' : '复制'}
                className={cn(
                    'inline-flex h-6 w-6 shrink-0 items-center justify-center rounded-sm transition-colors',
                    copied ? 'text-success' : 'text-text-tertiary hover:bg-surface hover:text-text',
                )}
            >
                {copied ? <Check size={13} /> : <Copy size={13} />}
            </button>
        </div>
    );
}
