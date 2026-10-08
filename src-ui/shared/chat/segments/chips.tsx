// 行内小标签：戳一戳、语音 / 视频的降级展示。

import type { ReactNode } from 'react';
import { Check, Copy } from 'lucide-react';
import { useCopy } from '../rightParts';

export function Chip({
    icon,
    title,
    children,
}: {
    icon?: ReactNode;
    title?: string;
    children: ReactNode;
}) {
    return (
        <span
            title={title}
            className="mx-0.5 inline-flex items-center gap-1 rounded-xs bg-inset px-1.5 py-px align-[1px] text-2xs text-text-secondary"
        >
            {icon}
            {children}
        </span>
    );
}

export function MediaChip({ icon, label, url }: { icon: ReactNode; label: string; url: string }) {
    const { copied, copy } = useCopy();
    return (
        <span className="mx-0.5 inline-flex items-center gap-1 rounded-xs bg-inset px-1.5 py-px align-[1px] text-2xs text-text-secondary">
            {icon}
            {label}
            {url && (
                <button
                    type="button"
                    aria-label={`复制${label}地址`}
                    title={url}
                    onClick={(e) => {
                        e.stopPropagation();
                        copy(url);
                    }}
                    className="ml-0.5 inline-flex h-4 w-4 items-center justify-center rounded-xs text-text-tertiary hover:bg-surface hover:text-text"
                >
                    {copied ? <Check size={10} aria-hidden /> : <Copy size={10} aria-hidden />}
                </button>
            )}
        </span>
    );
}
