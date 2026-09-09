// 应用端详情各 Tab 共用的读取中 / 失败占位。

import { RefreshCw } from 'lucide-react';
import { Button, PagePlaceholder, Spinner } from '../../../shared/ui';
import { ActionMotionIcon } from '../../../shared/ui/motion';

export function PaneLoading({ text }: { text: string }) {
    return (
        <PagePlaceholder className="gap-3 py-16">
            <Spinner size="lg" tone="brand" label={text} />
            <p className="text-sm text-text-tertiary">{text}</p>
        </PagePlaceholder>
    );
}

export function PaneLoadError({
    message,
    onRetry,
}: {
    message: string;
    onRetry: () => void;
}) {
    return (
        <PagePlaceholder className="gap-2 py-16">
            <p className="text-sm text-text-secondary">{message}</p>
            <Button size="sm" variant="secondary" onClick={onRetry}>
                <ActionMotionIcon icon={RefreshCw} size={13} />
                重试
            </Button>
        </PagePlaceholder>
    );
}
