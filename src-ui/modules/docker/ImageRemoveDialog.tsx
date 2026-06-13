// 删除镜像确认对话框：强制删除选项不塞进卡片 footer，避免网格单卡被撑高。
// 关闭时先播退场动画，onExited 后再 onDismiss，避免镜像名先消失导致布局跳动。

import React, { useEffect, useState } from 'react';
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
    Button,
    Checkbox,
} from '../../shared/ui';
import { imageDisplayRef, imageRemoveRef } from '../../core/domain/docker/status';
import type { ImageInfo } from '../../core/ipc/types';
import type { ImageRemoveRequest } from './ImageCard';

interface ImageRemoveDialogProps {
    /** 父级传入待删镜像；为 null 表示无待打开项。真正清空在 onDismiss（退场结束后）。 */
    image: ImageInfo | null;
    isRemoving: boolean;
    onDismiss: () => void;
    onConfirm: (req: ImageRemoveRequest) => void | Promise<void>;
}

export const ImageRemoveDialog: React.FC<ImageRemoveDialogProps> = ({
    image,
    isRemoving,
    onDismiss,
    onConfirm,
}) => {
    const [open, setOpen] = useState(false);
    const [shownImage, setShownImage] = useState<ImageInfo | null>(null);
    const [force, setForce] = useState(false);

    useEffect(() => {
        if (image) {
            setShownImage(image);
            setOpen(true);
        }
    }, [image]);

    const displayRef = shownImage ? imageDisplayRef(shownImage) : '';

    const requestClose = () => {
        if (isRemoving) return;
        setOpen(false);
    };

    const handleExited = () => {
        setShownImage(null);
        setForce(false);
        onDismiss();
    };

    const handleConfirm = async () => {
        if (!shownImage) return;
        try {
            await onConfirm({
                imageRef: imageRemoveRef(shownImage),
                force: force || undefined,
            });
            setOpen(false);
        } catch {
            /* 失败保持打开；InfoBar 由 useDocker 处理 */
        }
    };

    if (!shownImage && !open) {
        return null;
    }

    return (
        <Dialog open={open} onOpenChange={(o) => !o && requestClose()}>
            <DialogContent size="md" onExited={handleExited}>
                <DialogHeader>
                    <DialogTitle>删除镜像</DialogTitle>
                    <DialogDescription>
                        将从当前远端主机移除该镜像。若仍有容器在使用，普通删除会失败。
                    </DialogDescription>
                </DialogHeader>

                <div className="flex flex-col gap-4">
                    <div className="rounded-md border border-border-subtle bg-inset/40 px-3 py-2.5">
                        <p className="text-2xs uppercase tracking-wide text-text-tertiary">镜像</p>
                        <p className="mt-1 break-all font-mono text-sm text-text">{displayRef}</p>
                    </div>

                    <Checkbox
                        id="docker-image-remove-force"
                        checked={force}
                        onCheckedChange={setForce}
                        disabled={isRemoving}
                        label="强制删除"
                        hint="仍有容器引用时执行 docker rmi -f。若仍失败，请先到「容器」页删除或停止相关容器后再试。"
                    />
                </div>

                <DialogFooter>
                    <Button variant="secondary" onClick={requestClose} disabled={isRemoving}>
                        取消
                    </Button>
                    <Button variant="danger" onClick={() => void handleConfirm()} disabled={isRemoving}>
                        删除镜像
                    </Button>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    );
};