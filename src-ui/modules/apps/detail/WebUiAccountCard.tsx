// 账号密码类 WebUI 的账号卡：看 / 复制 / 停止后重置。运行中改配置会被进程覆盖，所以只在停止时开放重置。

import { useState } from 'react';
import { KeyRound, RefreshCw } from 'lucide-react';
import {
    Button,
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
    FormSection,
    Spinner,
    TextField,
} from '../../../shared/ui';
import { ActionMotionIcon } from '../../../shared/ui/motion';
import { useWebUiAccount } from '../../../hooks/apps/useWebUiAccount';
import { pushInfoBar } from '../../../hooks/ui/globalInfoBarStore';
import { pushAppErrorBar } from '../../../hooks/apps/pushAppErrorBar';
import { validateWebUiPassword } from '../../../core/domain/apps/webuiAccount';
import { WebUiAccountFields } from '../WebUiAccountFields';
import type { AppInstance } from '../../../core/ipc/types';

export const WebUiAccountCard: React.FC<{ instance: AppInstance }> = ({ instance }) => {
    const acct = useWebUiAccount(instance.id, true, instance.state);
    const [resetOpen, setResetOpen] = useState(false);
    const [custom, setCustom] = useState('');
    const customError = validateWebUiPassword(custom);

    if (!acct.account && !acct.isLoading) return null;

    const submitReset = async () => {
        if (customError) return;
        try {
            const next = await acct.reset(custom || null);
            setResetOpen(false);
            setCustom('');
            pushInfoBar({
                key: `webui-account-reset:${instance.id}`,
                tone: 'success',
                title: 'WebUI 密码已重置',
                content: next.password ? `新密码：${next.password}` : undefined,
                autoDismissMs: 12_000,
            });
        } catch (e) {
            pushAppErrorBar({
                key: `webui-account-reset:${instance.id}`,
                title: '重置密码失败',
                raw: (e as { message?: string }).message ?? String(e),
            });
        }
    };

    return (
        <FormSection
            title="WebUI 账号"
            actions={
                acct.account && (
                    <Button
                        size="sm"
                        variant="ghost"
                        disabled={!acct.account.can_reset || acct.isResetting}
                        title={acct.account.can_reset ? undefined : '停止实例后才能重置'}
                        onClick={() => setResetOpen(true)}
                    >
                        <ActionMotionIcon icon={KeyRound} size={13} />
                        重置密码
                    </Button>
                )
            }
        >
            {acct.account ? (
                <WebUiAccountFields account={acct.account} />
            ) : (
                <div className="flex items-center gap-2 text-xs text-text-tertiary">
                    <Spinner size="xs" /> 正在读取账号…
                </div>
            )}

            <Dialog open={resetOpen} onOpenChange={(o) => !o && !acct.isResetting && setResetOpen(false)}>
                <DialogContent size="sm">
                    <DialogHeader>
                        <DialogTitle>重置 WebUI 密码</DialogTitle>
                        <DialogDescription>写入配置，下次启动生效。</DialogDescription>
                    </DialogHeader>
                    <TextField
                        label="新密码"
                        type="password"
                        autoComplete="new-password"
                        autoFocus
                        value={custom}
                        onValueChange={setCustom}
                        error={customError ?? undefined}
                        hint="留空随机生成；8 位以上，含大小写和数字"
                        disabled={acct.isResetting}
                    />
                    <DialogFooter>
                        <Button
                            variant="ghost"
                            size="sm"
                            onClick={() => setResetOpen(false)}
                            disabled={acct.isResetting}
                        >
                            取消
                        </Button>
                        <Button
                            variant="primary"
                            size="sm"
                            disabled={!!customError || acct.isResetting}
                            onClick={() => void submitReset()}
                        >
                            {acct.isResetting ? (
                                <Spinner size="xs" className="text-white" />
                            ) : (
                                <ActionMotionIcon icon={RefreshCw} size={13} />
                            )}
                            重置
                        </Button>
                    </DialogFooter>
                </DialogContent>
            </Dialog>
        </FormSection>
    );
};
