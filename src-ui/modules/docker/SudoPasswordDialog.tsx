// sudo 密码对话框：远端 Linux 以 SSH 密钥登录、未保存密码时，装 Docker 要提权。
// 后端探测到这种情况会回需要密码的信号，这里收集 sudo 密码交给上层带着重试安装。
// onConfirm 由上层真正发起安装并在成功时关闭；失败(密码错等)抛出，这里 catch 内联报错保持打开。

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

interface SudoPasswordDialogProps {
    hostName: string;
    actionLabel?: string;
    reason?: string;
    isSubmitting: boolean;
    onConfirm: (password: string, remember: boolean) => void | Promise<void>;
    onClose: () => void;
}

export const SudoPasswordDialog: React.FC<SudoPasswordDialogProps> = ({
    hostName,
    actionLabel = '安装 Docker',
    reason,
    isSubmitting,
    onConfirm,
    onClose,
}) => {
    const [password, setPassword] = useState('');
    const [remember, setRemember] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const [pending, setPending] = useState(false);
    const pendingRef = useRef(false);
    const busy = isSubmitting || pending;

    const handleConfirm = async () => {
        if (isSubmitting || pendingRef.current) return;
        if (!password) {
            setError('请输入 sudo 密码');
            return;
        }
        setError(null);
        pendingRef.current = true;
        setPending(true);
        try {
            // 上层负责真正下发安装并在成功时关闭对话框。失败(密码错 / 提权被拒等)它会抛出，
            // 这里就地显示原因，对话框保持打开让用户改密码重试，而不是默默关掉。
            await onConfirm(password, remember);
        } catch (e) {
            setError(errorText(e, '安装失败'));
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

                    <Checkbox
                        label="在此设备记住密码"
                        hint="密码会安全存入系统密钥串，下次安装 / 提权免输入。不勾选则仅本次使用。"
                        checked={remember}
                        onCheckedChange={setRemember}
                        disabled={busy}
                    />

                    {error && <p className="text-xs text-danger">{error}</p>}
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
