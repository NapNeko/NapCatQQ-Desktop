import {
    Button,
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from '../../../../shared/ui';

interface ExternalAccessDialogProps {
    open: boolean;
    docker?: boolean;
    onOpenChange: (open: boolean) => void;
    onConfirm: () => void;
}

export function ExternalAccessDialog({
    open,
    docker = false,
    onOpenChange,
    onConfirm,
}: ExternalAccessDialogProps) {
    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent size="sm">
                <DialogHeader>
                    <DialogTitle>开放主机外部访问</DialogTitle>
                    <DialogDescription>
                        {docker
                            ? '这会把此 Bot 的所有 Docker 端口发布到 0.0.0.0，OneBot、WebUI 和 VNC 等服务将可从主机外部访问。'
                            : '此 OneBot 服务将监听主机外部地址，允许其它设备连接。'}
                        如果主机有公网地址，服务也可能暴露到公网。
                    </DialogDescription>
                </DialogHeader>
                <p className="text-xs leading-relaxed text-text-secondary">
                    开放前请设置可靠的鉴权 Token
                    和管理密码，并使用主机防火墙或安全组仅允许可信来源。 此操作不会自动调整防火墙。
                </p>
                <DialogFooter>
                    <Button variant="ghost" onClick={() => onOpenChange(false)}>
                        取消
                    </Button>
                    <Button variant="danger" onClick={onConfirm}>
                        确认开放
                    </Button>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    );
}
