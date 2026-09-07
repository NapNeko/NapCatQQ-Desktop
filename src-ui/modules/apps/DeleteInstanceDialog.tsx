// 删除应用实例的二次确认（列表与详情页共用）。

import React, { useState } from 'react';
import {
    Button,
    Checkbox,
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
    Spinner,
} from '../../shared/ui';
import type { AppInstance } from '../../core/ipc/types';

export const DeleteInstanceDialog: React.FC<{
    instance: AppInstance | null;
    isRemoving: boolean;
    onClose: () => void;
    onConfirm: (removeFiles: boolean) => Promise<void>;
}> = ({ instance, isRemoving, onClose, onConfirm }) => {
    const [removeFiles, setRemoveFiles] = useState(false);
    React.useEffect(() => {
        if (instance) setRemoveFiles(false);
    }, [instance]);

    return (
        <Dialog open={instance !== null} onOpenChange={(o) => !o && !isRemoving && onClose()}>
            <DialogContent size="sm" dismissOnOutsideClick={!isRemoving}>
                <DialogHeader>
                    <DialogTitle>删除实例？</DialogTitle>
                    <DialogDescription>
                        即将删除应用实例 "{instance?.display_name}"
                        {instance?.state === 'running' ? '（会先停止进程）' : ''}
                        {instance?.link ? '，并移除协议 Bot 上对应的对接连接' : ''}。此操作不可撤销。
                    </DialogDescription>
                </DialogHeader>
                <Checkbox
                    label="同时删除安装目录"
                    hint={instance?.install_dir}
                    checked={removeFiles}
                    onCheckedChange={setRemoveFiles}
                />
                <DialogFooter>
                    <Button variant="ghost" size="sm" onClick={onClose} disabled={isRemoving}>
                        取消
                    </Button>
                    <Button
                        variant="danger"
                        size="sm"
                        disabled={isRemoving}
                        onClick={() => void onConfirm(removeFiles).catch(() => undefined)}
                    >
                        {isRemoving && <Spinner size="sm" className="text-white" />}
                        确认删除
                    </Button>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    );
};
