// 组件页「更新日志」：展示 GitHub release body（Markdown）。
// 数据来自 release 快照（中转 / GitHub 已写入 release_notes），本组件只负责呈现。

import { ExternalLink, ScrollText } from 'lucide-react';
import {
    Button,
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
    SimpleMarkdown,
} from '../../shared/ui';
import { useOpenExternal } from '../../hooks/useOpenExternal';
import type { ReleaseInfoView } from '../../core/domain/release/normalize';

export interface ReleaseNotesDialogProps {
    open: boolean;
    onOpenChange: (open: boolean) => void;
    /** 组件展示名，如 NapCat / SnowLuma / NapCatQQ Desktop */
    componentLabel: string;
    release: ReleaseInfoView | null;
}

export function ReleaseNotesDialog({
    open,
    onOpenChange,
    componentLabel,
    release,
}: ReleaseNotesDialogProps) {
    const openExternal = useOpenExternal();
    const version = release?.version?.trim() || null;
    const notes = release?.releaseNotes ?? '';
    const htmlUrl = release?.htmlUrl?.trim() || null;
    const publishedAt = release?.publishedAt;

    const publishedLabel =
        publishedAt && publishedAt > 0
            ? new Date(publishedAt * 1000).toLocaleString(undefined, {
                year: 'numeric',
                month: '2-digit',
                day: '2-digit',
                hour: '2-digit',
                minute: '2-digit',
            })
            : null;

    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent size="lg">
                <DialogHeader>
                    <div className="flex items-start gap-3 pr-6">
                        <span className="mt-0.5 flex h-9 w-9 shrink-0 items-center justify-center rounded-md bg-brand/10 text-brand">
                            <ScrollText size={18} strokeWidth={1.75} aria-hidden />
                        </span>
                        <div className="min-w-0 flex-1">
                            <DialogTitle className="truncate">
                                更新日志
                                {version ? (
                                    <span className="ml-2 font-mono text-sm font-normal text-text-secondary">
                                        {componentLabel} · v{version.replace(/^[vV]/, '')}
                                    </span>
                                ) : (
                                    <span className="ml-2 text-sm font-normal text-text-secondary">
                                        {componentLabel}
                                    </span>
                                )}
                            </DialogTitle>
                            <DialogDescription className="mt-1">
                                {publishedLabel
                                    ? `发布于 ${publishedLabel}`
                                    : '来自远端 Release 快照（中转或 GitHub）'}
                            </DialogDescription>
                        </div>
                    </div>
                </DialogHeader>

                <div className="max-h-[min(58vh,28rem)] overflow-y-auto rounded-md border border-border-subtle bg-surface/60 p-4">
                    {release ? (
                        <SimpleMarkdown
                            text={notes}
                            emptyFallback="该版本暂未附带更新说明。可在 GitHub Release 页查看完整内容。"
                            onOpenLink={openExternal}
                        />
                    ) : (
                        <p className="text-sm text-text-tertiary">
                            暂无远端版本信息，请稍后刷新组件页再试。
                        </p>
                    )}
                </div>

                <DialogFooter>
                    {htmlUrl ? (
                        <Button
                            size="sm"
                            variant="secondary"
                            onClick={() => openExternal(htmlUrl)}
                        >
                            <ExternalLink size={14} strokeWidth={2} aria-hidden />
                            在浏览器打开
                        </Button>
                    ) : null}
                    <Button size="sm" variant="ghost" onClick={() => onOpenChange(false)}>
                        关闭
                    </Button>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    );
}
