// 面板凭据：告诉用户「面板现在缺什么」，并把密码存进本机密钥库。
//
// 为什么不做「桌面端替你设密码」：那是用户对面板的所有权，桌面端只该提示。
// 但也不能只说「请填密码」——面板可能**还没设过**密码，那时用户手上有密码可填才是怪事。
// 所以先探 /api/auth/status（未设密码时它也是公开的），据 configured / setup_allowed 分三种说法：
//   没设密码 + 本机可设 → 请去面板完成设置（给「打开面板」直达）
//   没设密码 + 本机不可设 → 设置只能在那台机器上做（远端实例）
//   已设密码 → 把密码填进来

import { useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { Button, TextField } from '../../../../shared/ui';
import { appFrameworkService } from '../../../../core/services/app-framework.service';
import { parseNeoBotAuthStatus } from './neobotPanels';
import { usePanelJson } from './useNeoBotPanel';

export const PanelCredentialCard: React.FC<{
    instanceId: string;
    onOpenWebUi: () => void;
}> = ({ instanceId, onOpenWebUi }) => {
    const queryClient = useQueryClient();
    const [password, setPassword] = useState('');
    const queryKey = ['appPanelPassword', instanceId] as const;

    const state = useQuery<boolean, Error>({
        queryKey,
        queryFn: () => appFrameworkService.panelPasswordSet(instanceId),
    });
    // 探面板自己的登录状态；探不到（面板没起来）就退回「填密码」的说法，不挡用户
    const auth = usePanelJson(instanceId, 'authStatus', '/api/auth/status', parseNeoBotAuthStatus);

    const save = useMutation({
        mutationFn: (value: string) => appFrameworkService.setPanelPassword(instanceId, value),
        onSuccess: () => {
            setPassword('');
            void queryClient.invalidateQueries({ queryKey });
        },
    });

    const remembered = state.data === true;
    const status = auth.data?.kind === 'ok' ? auth.data.data : null;
    const needsSetup = status !== null && !status.configured;

    return (
        <section className="rounded-md border border-border-subtle bg-surface px-4 py-3">
            <div className="flex items-baseline justify-between gap-2">
                <h3 className="text-sm font-semibold text-text">面板凭据</h3>
                <span className={remembered ? 'text-2xs text-brand' : 'text-2xs text-warning'}>
                    {state.isLoading ? '读取中…' : remembered ? '已记住' : '未设置'}
                </span>
            </div>

            {needsSetup ? (
                <div className="mt-1 flex flex-col gap-2">
                    <p className="text-xs leading-relaxed text-text-secondary">
                        面板还没有设置登录密码，所以桌面的面板页现在读不到数据。
                        密码要由你来设——面板把它存成哈希，桌面端读不出也写不了，
                        <span className="text-text"> 也更不该替你设</span>。
                    </p>
                    {status.setupAllowed ? (
                        <p className="text-xs leading-relaxed text-text-secondary">
                            你现在这台机器就能设：点下面的按钮打开面板，按提示设一个密码，
                            然后回到这里把它填进来。
                        </p>
                    ) : (
                        <p className="text-xs leading-relaxed text-text-secondary">
                            只能在 NeoBot 所在的那台机器上设（面板只允许本机完成首次设置）。
                            设好之后回到这里把密码填进来。
                        </p>
                    )}
                    <div>
                        <Button size="sm" variant="primary" onClick={onOpenWebUi}>
                            打开面板去设置
                        </Button>
                    </div>
                </div>
            ) : (
                <>
                    <p className="mt-1 text-xs leading-relaxed text-text-secondary">
                        面板接口要登录才能读。桌面端只把这个密码存进本机密钥库，用来调面板接口——
                        不写回 NeoBot，也不参与登录接管。
                    </p>
                    <div className="mt-3 flex items-end gap-2">
                        <TextField
                            className="flex-1"
                            type="password"
                            label="面板密码"
                            placeholder="与你在面板登录时输入的一致"
                            value={password}
                            autoComplete="off"
                            disabled={save.isPending}
                            onValueChange={setPassword}
                            onKeyDown={(e) => {
                                if (e.key === 'Enter' && password.trim()) save.mutate(password);
                            }}
                        />
                        <Button
                            variant="primary"
                            size="sm"
                            disabled={save.isPending || !password.trim()}
                            onClick={() => save.mutate(password)}
                        >
                            {save.isPending ? '保存中…' : '保存'}
                        </Button>
                        {remembered && (
                            <Button
                                variant="ghost"
                                size="sm"
                                disabled={save.isPending}
                                onClick={() => save.mutate('')}
                            >
                                清除
                            </Button>
                        )}
                    </div>
                </>
            )}

            {save.isError && (
                <p className="mt-2 text-2xs text-danger">保存失败：{save.error.message}</p>
            )}
            {state.isError && (
                <p className="mt-2 text-2xs text-danger">读取失败：{state.error.message}</p>
            )}
        </section>
    );
};
