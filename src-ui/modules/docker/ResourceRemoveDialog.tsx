// 删除前明确资源与副作用，失败时保留对话框。
import { useRef, useState } from 'react';
import { Trash2 } from 'lucide-react';
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

export type RemovalTarget = { kind: 'container' | 'network' | 'volume'; id: string; name: string };

export function ResourceRemoveDialog({
    target,
    busy,
    onClose,
    onConfirm,
}: {
    target: RemovalTarget;
    busy: boolean;
    onClose: () => void;
    onConfirm: (force: boolean) => Promise<void>;
}) {
    const [force, setForce] = useState(false);
    const [name, setName] = useState('');
    const submitting = useRef(false);
    const label = { container: '容器', network: '网络', volume: '数据卷' }[target.kind];
    const confirm = async () => {
        if (busy || submitting.current || (target.kind === 'volume' && name !== target.name))
            return;
        submitting.current = true;
        try {
            await onConfirm(force);
            onClose();
        } catch {
            // useDocker 已发送全局提示；保留确认内容供重试。
        } finally {
            submitting.current = false;
        }
    };
    return (
        <Dialog open onOpenChange={(open) => !open && !busy && !submitting.current && onClose()}>
            <DialogContent size="md">
                <DialogHeader>
                    <DialogTitle className="flex items-center gap-2">
                        <span className="inline-flex h-7 w-7 shrink-0 items-center justify-center rounded-md bg-danger-soft text-danger">
                            <Trash2 size={15} strokeWidth={2.2} aria-hidden />
                        </span>
                        删除{label}
                    </DialogTitle>
                    <DialogDescription>
                        {target.kind === 'container'
                            ? '容器可写层中的数据将丢失，挂载的数据卷和宿主机目录保留。'
                            : target.kind === 'volume'
                              ? '卷中的全部数据将永久删除。正在被容器引用的卷不能删除。'
                              : '正在被容器使用的网络不能删除。'}
                    </DialogDescription>
                </DialogHeader>
                <div className="rounded-md border border-border-subtle/60 bg-inset/25 px-3 py-2.5">
                    <p className="text-2xs uppercase tracking-wide text-text-tertiary">{label}</p>
                    <p className="mt-1 break-all font-mono text-sm text-text">{target.name}</p>
                </div>
                {target.kind === 'container' && (
                    <Checkbox
                        id="docker-container-force"
                        label="强制删除运行中的容器"
                        checked={force}
                        onCheckedChange={setForce}
                        disabled={busy}
                    />
                )}
                {target.kind === 'volume' && (
                    <label className="flex flex-col gap-2 text-sm text-text-secondary">
                        输入数据卷名称确认
                        <input
                            value={name}
                            onChange={(event) => setName(event.target.value)}
                            autoComplete="off"
                            disabled={busy}
                            className="rounded-sm border border-border-subtle bg-field px-3 py-2 text-text"
                        />
                    </label>
                )}
                <DialogFooter>
                    <Button variant="secondary" onClick={onClose} disabled={busy}>
                        取消
                    </Button>
                    <Button
                        variant="danger"
                        disabled={busy || (target.kind === 'volume' && name !== target.name)}
                        onClick={() => void confirm()}
                    >
                        {busy ? '删除中…' : `删除${label}`}
                    </Button>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    );
}
