import { useCallback, useState } from 'react';
import { Check, Copy } from 'lucide-react';
import { cn } from '../utils/cn';
import { Button } from './Button';

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
                'flex items-start gap-2 rounded-sm border border-border-subtle bg-inset/80 p-2',
                className,
            )}
        >
            <pre className="min-w-0 flex-1 overflow-x-auto whitespace-pre-wrap break-all font-mono text-xs leading-relaxed text-text">
                {command}
            </pre>
            <Button
                type="button"
                size="sm"
                variant="ghost"
                className="shrink-0"
                onClick={() => void onCopy()}
                aria-label={copied ? '已复制' : '复制命令'}
            >
                {copied ? <Check size={14} /> : <Copy size={14} />}
            </Button>
        </div>
    );
}