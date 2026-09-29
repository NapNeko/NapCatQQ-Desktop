// 远端网络配置回读后的确认：列出替换会带来的增删改，确认后只改表单，保存仍由保存条负责。

import {
    Badge,
    Button,
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from '../../../shared/ui';
import { getKindMeta } from '../../../core/domain/bot/connections';
import type {
    ConnectionRef,
    ImportedNetworkPreview,
} from '../../../core/domain/bot/imported-network';

interface RemoteNetworkPullDialogProps {
    preview: ImportedNetworkPreview | null;
    onConfirm: () => void;
    onCancel: () => void;
}

export function RemoteNetworkPullDialog({
    preview,
    onConfirm,
    onCancel,
}: RemoteNetworkPullDialogProps) {
    return (
        <Dialog open={preview !== null} onOpenChange={(open) => !open && onCancel()}>
            <DialogContent size="md">
                <DialogHeader>
                    <DialogTitle>用远端的网络配置替换</DialogTitle>
                    <DialogDescription>
                        替换后要保存才生效，不保存的话下次启动仍会按原配置覆盖远端文件。
                    </DialogDescription>
                </DialogHeader>

                {preview && (
                    <div className="flex flex-col gap-3 text-sm">
                        <ChangeGroup label="远端新增" tone="success" items={preview.added} />
                        <ChangeGroup label="远端有改动" tone="warning" items={preview.changed} />
                        <ChangeGroup label="远端没有，将删除" tone="danger" items={preview.removed} />
                        {preview.otherChanged && (
                            <p className="text-xs text-text-secondary">
                                音乐签名、状态命令等连接以外的设置也和远端不同，会一并替换。
                            </p>
                        )}
                    </div>
                )}

                <DialogFooter>
                    <Button variant="ghost" size="sm" onClick={onCancel}>
                        取消
                    </Button>
                    <Button variant="primary" size="sm" onClick={onConfirm}>
                        替换到表单
                    </Button>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    );
}

function ChangeGroup({
    label,
    tone,
    items,
}: {
    label: string;
    tone: 'success' | 'warning' | 'danger';
    items: ConnectionRef[];
}) {
    if (items.length === 0) return null;
    return (
        <div className="flex flex-col gap-1.5">
            <span className="text-xs font-medium text-text-secondary">{label}</span>
            <div className="flex flex-wrap gap-1.5">
                {items.map((item) => (
                    <Badge
                        key={`${item.kind}-${item.name}`}
                        tone={tone}
                        appearance="soft"
                        className="max-w-full"
                    >
                        <span className="opacity-70">{getKindMeta(item.kind).title}</span>
                        <span className="truncate font-mono">{item.name}</span>
                    </Badge>
                ))}
            </div>
        </div>
    );
}
