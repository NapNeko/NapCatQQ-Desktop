// 打开账号密码类 WebUI 后弹出的账号框；全应用挂一次（AppsPageNext）。

import { ExternalLink } from 'lucide-react';
import {
    Button,
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from '../../shared/ui';
import { ActionMotionIcon } from '../../shared/ui/motion';
import { openExternalUrl } from '../../core/ipc/transport';
import {
    closeWebUiAccountDialog,
    useWebUiAccountDialog,
} from '../../hooks/apps/webuiAccountDialogStore';
import { WebUiAccountFields } from './WebUiAccountFields';

export const WebUiAccountDialogHost: React.FC = () => {
    const state = useWebUiAccountDialog();
    return (
        <Dialog open={state !== null} onOpenChange={(o) => !o && closeWebUiAccountDialog()}>
            <DialogContent size="sm">
                {state && (
                    <>
                        <DialogHeader>
                            <DialogTitle>WebUI 登录账号</DialogTitle>
                            <DialogDescription className="truncate">
                                {state.instanceName} · {state.url}
                            </DialogDescription>
                        </DialogHeader>
                        <WebUiAccountFields account={state.account} />
                        <DialogFooter>
                            <Button
                                variant="ghost"
                                size="sm"
                                onClick={() => void openExternalUrl(state.url)}
                            >
                                <ActionMotionIcon icon={ExternalLink} size={13} />
                                再打开一次
                            </Button>
                            <Button variant="primary" size="sm" onClick={closeWebUiAccountDialog}>
                                关闭
                            </Button>
                        </DialogFooter>
                    </>
                )}
            </DialogContent>
        </Dialog>
    );
};
