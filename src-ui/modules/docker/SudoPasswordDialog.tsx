// sudo 验证失败时保留弹窗，供用户修改密码后重试。

import React, { useRef, useState } from 'react';
import { Loader2 } from 'lucide-react';
import {
    Dialog,
    DialogContent,
    DialogHeader,
    DialogTitle,
    DialogDescription,
    DialogFooter,
    Button,
    TextField,
    Checkbox,
} from '../../shared/ui';
import { errorText } from '../../core/domain/errors';
import { pushErrorBar } from '../../hooks/ui/pushErrorBar';

interface SudoPasswordDialogProps {
    hostName: string;
    actionLabel?: string;
    reason?: string;
    isSubmitting: boolean;
    allowRemember?: boolean;
    onConfirm: (password: string, remember: boolean) => void | Promise<void>;
    onClose: () => void;
}

export const SudoPasswordDialog: React.FC<SudoPasswordDialogProps> = ({
    hostName,
    actionLabel = '准备容器环境',
    reason,
    isSubmitting,
    allowRemember = true,
    onConfirm,
    onClose,
}) => {
    const [password, setPassword] = useState('');
    const [remember, setRemember] = useState(false);
    const [pending, setPending] = useState(false);
    const pendingRef = useRef(false);
    const busy = isSubmitting || pending;

    const handleConfirm = async () => {
        if (isSubmitting || pendingRef.current || !password) return;
        pendingRef.current = true;
        setPending(true);
        try {
            await onConfirm(password, allowRemember && remember);
        } catch (e) {
            pushErrorBar({
                key: `sudo:${hostName}:${actionLabel}`,
                title: `${actionLabel}失败 · ${hostName}`,
                raw: errorText(e),
            });
        } finally {
            pendingRef.current = false;
            setPending(false);
        }
    };

    return (
        <Dialog open onOpenChange={(o) => !o && !busy && onClose()}>
            <DialogContent size="md">
                <DialogHeader>
                    <DialogTitle>需要 sudo 密码</DialogTitle>
                    <DialogDescription>
                        在 {hostName} 上{actionLabel}需要 sudo 权限。
                    </DialogDescription>
                </DialogHeader>

                <div className="flex flex-col gap-4">
                    {reason && <p className="text-xs text-text-secondary">{reason}</p>}

                    <TextField
                        type="password"
                        label="sudo 密码"
                        autoFocus
                        placeholder="输入目标主机用户的 sudo 密码"
                        value={password}
                        onValueChange={setPassword}
                        disabled={busy}
                    />

                    {allowRemember && (
                        <Checkbox
                            label="在此设备记住密码"
                            hint="密码会安全存入系统密钥串，下次安装 / 提权免输入。不勾选则仅本次使用。"
                            checked={remember}
                            onCheckedChange={setRemember}
                            disabled={busy}
                        />
                    )}
                </div>

                <DialogFooter>
                    <Button variant="ghost" onClick={onClose} disabled={busy}>
                        取消
                    </Button>
                    <Button onClick={() => void handleConfirm()} disabled={busy || !password}>
                        {busy && <Loader2 size={14} className="animate-spin" />}
                        {actionLabel}
                    </Button>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    );
};
