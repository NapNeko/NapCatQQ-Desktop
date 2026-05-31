// sudo 密码对话框：远端 Linux 以 SSH 密钥登录、未保存密码时，装 Docker 要提权。
// 后端探测到这种情况会回需要密码的信号，这里收集 sudo 密码交给上层带着重试安装。
// onConfirm 由上层真正发起安装并在成功时关闭；失败(密码错等)抛出，这里 catch 内联报错保持打开。

import React, { useState } from 'react';
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
    reason?: string;
    isSubmitting: boolean;
    onConfirm: (password: string, remember: boolean) => void | Promise<void>;
    onClose: () => void;
}

export const SudoPasswordDialog: React.FC<SudoPasswordDialogProps> = ({
    hostName,
    reason,
    isSubmitting,
    onConfirm,
    onClose,
}) => {
    const [password, setPassword] = useState('');
    const [remember, setRemember] = useState(false);
    const [error, setError] = useState<string | null>(null);

    const handleConfirm = async () => {
        if (!password) {
            setError('请输入 sudo 密码');
            return;
        }
        setError(null);
        try {
            // 上层负责真正下发安装并在成功时关闭对话框。失败(密码错 / 提权被拒等)它会抛出，
            // 这里就地显示原因，对话框保持打开让用户改密码重试，而不是默默关掉。
            await onConfirm(password, remember);
        } catch (e) {
            setError(errorText(e, '安装失败，请重试'));
        }
    };

    return (
        <Dialog open onOpenChange={(o) => !o && onClose()}>
            <DialogContent className="max-w-md">
                <DialogHeader>
                    <DialogTitle>需要 sudo 密码</DialogTitle>
                    <DialogDescription>
                        {hostName} 是以 SSH 密钥登录的远端机器，安装 Docker 需要管理员（sudo）权限。
                        请输入该登录用户的 sudo 密码以继续。
                    </DialogDescription>
                </DialogHeader>

                <div className="flex flex-col gap-4">
                    {reason && <p className="text-xs text-text-secondary">{reason}</p>}

                    <TextField
                        type="password"
                        label="sudo 密码"
                        autoFocus
                        placeholder="输入远端用户的 sudo 密码"
                        value={password}
                        onValueChange={setPassword}
                        disabled={isSubmitting}
                    />

                    <Checkbox
                        label="在此设备记住密码"
                        hint="密码会安全存入系统密钥串，下次安装 / 提权免输入。不勾选则仅本次使用。"
                        checked={remember}
                        onCheckedChange={setRemember}
                        disabled={isSubmitting}
                    />

                    <p className="text-2xs text-text-tertiary leading-snug">
                        密码仅用于在 {hostName} 上执行 sudo，通过加密的 SSH 通道传输，不会以明文写入任何配置文件。
                    </p>

                    {error && <p className="text-xs text-danger">{error}</p>}
                </div>

                <DialogFooter>
                    <Button variant="ghost" onClick={onClose} disabled={isSubmitting}>
                        取消
                    </Button>
                    <Button
                        onClick={() => void handleConfirm()}
                        disabled={isSubmitting || !password}
                    >
                        {isSubmitting && <Loader2 size={14} className="animate-spin" />}
                        安装 Docker
                    </Button>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    );
};
