// 面板凭据：请用户填一次面板密码。
//
// 为什么需要用户手填：NeoBot 的面板口令由它自己管——auth.json 里存的是哈希，
// 桌面端既读不出也写不了，所以既不能自动登录、也不能替它重置。桌面端只做一件事：
// 把它存进**本机密钥库**，之后用它调面板接口（模型、提示词、日志这些）。
// 它不写回框架，也不参与「账号密码类 WebUI」的登录接管。

import { useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { Button, TextField } from '../../../../shared/ui';
import { appFrameworkService } from '../../../../core/services/app-framework.service';

export const PanelCredentialCard: React.FC<{ instanceId: string }> = ({ instanceId }) => {
    const queryClient = useQueryClient();
    const [password, setPassword] = useState('');
    const queryKey = ['appPanelPassword', instanceId] as const;

    const state = useQuery<boolean, Error>({
        queryKey,
        queryFn: () => appFrameworkService.panelPasswordSet(instanceId),
    });

    const save = useMutation({
        mutationFn: (value: string) => appFrameworkService.setPanelPassword(instanceId, value),
        onSuccess: () => {
            setPassword('');
            void queryClient.invalidateQueries({ queryKey });
        },
    });

    const remembered = state.data === true;

    return (
        <section className="rounded-md border border-border-subtle bg-surface px-4 py-3">
            <div className="flex items-baseline justify-between gap-2">
                <h3 className="text-sm font-semibold text-text">面板凭据</h3>
                <span className={remembered ? 'text-2xs text-brand' : 'text-2xs text-warning'}>
                    {state.isLoading ? '读取中…' : remembered ? '已记住' : '未设置'}
                </span>
            </div>

            <p className="mt-1 text-xs leading-relaxed text-text-secondary">
                面板口令由 NeoBot 自己保管（落盘是哈希），桌面端读不到也改不了，所以要你填一次。
                桌面端只把它存进本机密钥库，用来调面板接口——不写回 NeoBot，也不参与登录接管。
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

            {save.isError && (
                <p className="mt-2 text-2xs text-danger">保存失败：{save.error.message}</p>
            )}
            {state.isError && (
                <p className="mt-2 text-2xs text-danger">读取失败：{state.error.message}</p>
            )}
        </section>
    );
};
