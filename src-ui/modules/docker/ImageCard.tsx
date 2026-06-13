// 单个镜像卡片：引用 + 大小 + 创建时间；删除走父级弹窗，footer 与容器卡同高。

import React from 'react';
import { Disc3, Trash2 } from 'lucide-react';
import { ActionMotionIcon, RESOURCE_MOTION } from '../../shared/ui/motion';
import { Badge, Button } from '../../shared/ui';
import { cn } from '../../shared/utils/cn';
import {
    imageDisplayRef,
    isDanglingImage,
    isManagedImageRef,
} from '../../core/domain/docker/status';
import type { ImageInfo } from '../../core/ipc/types';

export interface ImageRemoveRequest {
    imageRef: string;
    force?: boolean;
}

interface ImageCardProps {
    image: ImageInfo;
    isRemoving: boolean;
    onRequestRemove: () => void;
}

export const ImageCard: React.FC<ImageCardProps> = ({ image, isRemoving, onRequestRemove }) => {
    const displayRef = imageDisplayRef(image);
    const dangling = isDanglingImage(image);
    const managed = isManagedImageRef(image.repository);

    return (
        <article
            className={cn(
                'relative isolate flex h-full w-full min-w-0 flex-col overflow-hidden ' +
                    'rounded-md border border-border-subtle bg-surface shadow-card ' +
                    'transition-[box-shadow] duration-200 hover:shadow-popover',
            )}
        >
            <div className="flex min-h-0 flex-1 flex-col gap-2 px-3.5 pb-2 pt-3">
                <div className="flex min-w-0 items-start gap-2.5">
                    <div className="grid h-9 w-9 shrink-0 place-items-center rounded-md bg-brand-soft text-brand">
                        <ActionMotionIcon icon={Disc3} size={18} motion={RESOURCE_MOTION} />
                    </div>
                    <div className="min-w-0 flex-1">
                        <div className="flex min-w-0 items-center gap-2">
                            <h3
                                className="min-w-0 flex-1 truncate font-mono text-sm font-semibold leading-snug text-text"
                                title={displayRef}
                            >
                                {displayRef}
                            </h3>
                            {managed ? (
                                <Badge tone="brand" appearance="soft" className="shrink-0">
                                    托管
                                </Badge>
                            ) : null}
                            {dangling ? (
                                <Badge tone="warning" appearance="soft" className="shrink-0">
                                    悬空
                                </Badge>
                            ) : null}
                        </div>
                        <p className="mt-0.5 font-mono text-2xs text-text-tertiary">{image.id}</p>
                    </div>
                </div>

                <div className="flex flex-wrap items-center gap-2 text-xs text-text-secondary">
                    <span>{image.size}</span>
                    <span className="text-text-disabled">·</span>
                    <span>{image.createdSince}</span>
                </div>
            </div>

            <footer className="flex min-h-[2.75rem] shrink-0 flex-wrap items-center justify-end gap-1.5 border-t border-border-subtle bg-inset/40 px-3 py-2">
                <Button
                    size="sm"
                    variant="ghost"
                    onClick={onRequestRemove}
                    disabled={isRemoving}
                    className="text-danger hover:bg-danger-soft"
                >
                    <ActionMotionIcon icon={Trash2} size={13} />
                    删除
                </Button>
            </footer>
        </article>
    );
};