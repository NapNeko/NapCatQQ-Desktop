// 删除应用实例的二次确认（列表与详情页共用）。导入项默认释放并还原快照。

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

    const imported = instance?.origin === 'imported';

    return (
        <Dialog open={instance !== null} onOpenChange={(o) => !o && !isRemoving && onClose()}>
            <DialogContent size="sm" dismissOnOutsideClick={!isRemoving}>
                <DialogHeader>
                    <DialogTitle>{imported ? '释放接管？' : '删除实例？'}</DialogTitle>
                    <DialogDescription>
                        {imported
                            ? `还原「${instance.display_name}」导入时的配置，恢复原来的 systemd（如有），然后注销控制台记录${instance.state === 'running' ? '，会先停止进程' : ''}${instance.link ? '，并移除协议 Bot 上对应的对接连接' : ''}。`
                            : `即将删除应用实例「${instance?.display_name}」${instance?.state === 'running' ? '，会先停止进程' : ''}${instance?.link ? '，并移除协议 Bot 上对应的对接连接' : ''}。`}
                    </DialogDescription>
                </DialogHeader>
                <Checkbox
                    label="同时删除安装目录"
                    hint={
                        imported
                            ? `勾选会删掉 ${instance.install_dir}，快照也无法再还原`
                            : instance?.install_dir
                    }
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
                        {imported && !removeFiles
                            ? '释放并还原'
                            : imported
                              ? '删除项目并注销'
                              : '确认删除'}
                    </Button>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    );
};
