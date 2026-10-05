// 面板凭据：告诉用户「面板现在缺什么」，把密码存进本机密钥库，并**当场验证能不能登录**。
//
// 三条实测出来的教训（都不是想当然能想对的）：
//   1. 面板没设密码时 /api/* 一律 403，回环也不放行——所以不能只说「请填密码」，
//      得先探 /api/auth/status 分清「还没设密码」与「已设密码等你填」。
//   2. 登录接口本身是 1.2.4a1 才有的。更老的版本（例如 1.2.3）桌面端**代不了登录**，
//      这时要说清楚「这个版本不支持」，而不是让用户对着一个永远失败的输入框反复试。
//   3. 存完必须验一次并把结果说出来。上一版存了就走，密码错了也没有任何反馈——
//      用户只会在别的页看到「先填面板密码」，却不知道自己已经填过（实测反馈）。

import { useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { Button, TextField } from '../../../../shared/ui';
import { appFrameworkService } from '../../../../core/services/app-framework.service';
import type { AppInstance } from '../../../../core/ipc/types';
import { meetsNeoBotVersion, versionRequirementText } from './neobotCapabilities';
import { parseNeoBotAuthStatus } from './neobotPanels';
import { usePanelJson } from './useNeoBotPanel';

/** 一次「保存并验证」的结果 */
type VerifyOutcome =
    | { state: 'ok'; text: string }
    | { state: 'bad'; text: string }
    | { state: 'unknown'; text: string };

export const PanelCredentialCard: React.FC<{
    instance: AppInstance;
    onOpenWebUi: () => void;
}> = ({ instance, onOpenWebUi }) => {
    const instanceId = instance.id;
    const queryClient = useQueryClient();
    const [password, setPassword] = useState('');
    const [outcome, setOutcome] = useState<VerifyOutcome | null>(null);
    const queryKey = ['appPanelPassword', instanceId] as const;

    // 登录是 1.2.4a1 起才有的能力；更老的版本连登录接口都没有
    const loginSupported = meetsNeoBotVersion(instance.installed_version, 'panelLogin');

    const state = useQuery<boolean, Error>({
        queryKey,
        queryFn: () => appFrameworkService.panelPasswordSet(instanceId),
    });
    // 探面板自己的登录状态；探不到（面板没起来）就退回「填密码」的说法，不挡用户
    const auth = usePanelJson(
        instanceId,
        'authStatus',
        '/api/auth/status',
        parseNeoBotAuthStatus,
        loginSupported,
    );

    const save = useMutation({
        mutationFn: async (value: string): Promise<VerifyOutcome> => {
            // 先存：panelCall 要从密钥库取凭据才能发起登录
            await appFrameworkService.setPanelPassword(instanceId, value);
            if (!value.trim()) return { state: 'unknown', text: '已清除保存的面板密码。' };
            // 立刻用需要登录的接口验一次，把结果说出来
            const probe = await appFrameworkService.panelCall(instanceId, 'GET', '/api/overview');
            if (probe === null) {
                return { state: 'unknown', text: '该框架不支持面板转发，密码已保存。' };
            }
            switch (probe.kind) {
                case 'ok':
                    return { state: 'ok', text: '登录成功：面板已接受这个密码。' };
                case 'unauthorized':
                    return {
                        state: 'bad',
                        text: '登录失败：面板不接受这个密码，请核对后重填（面板登录时用的那个）。',
                    };
                case 'not_found':
                    return {
                        state: 'unknown',
                        text: '这个 NeoBot 版本没有 /api/overview，验证不了；密码已保存。',
                    };
                case 'unreachable':
                    return {
                        state: 'unknown',
                        text: '面板打不通，没法验证：' + (probe.message ?? ''),
                    };
                default:
                    return {
                        state: 'unknown',
                        text: '没能验证：' + (probe.message ?? '面板拒绝了这次请求'),
                    };
            }
        },
        onSuccess: (result) => {
            setOutcome(result);
            if (result.state === 'ok') setPassword('');
            void queryClient.invalidateQueries({ queryKey });
        },
        onError: (err) => {
            setOutcome({
                state: 'unknown',
                text: '保存失败：' + (err instanceof Error ? err.message : String(err)),
            });
        },
    });

    const remembered = state.data === true;
    const status = auth.data?.kind === 'ok' ? auth.data.data : null;
    const needsSetup = loginSupported && status !== null && !status.configured;

    const outcomeColor =
        outcome?.state === 'ok'
            ? 'text-brand'
            : outcome?.state === 'bad'
              ? 'text-danger'
              : 'text-text-secondary';

    return (
        <section className="rounded-md border border-border-subtle bg-surface px-4 py-3">
            <div className="flex items-baseline justify-between gap-2">
                <h3 className="text-sm font-semibold text-text">面板凭据</h3>
                <span className={remembered ? 'text-2xs text-brand' : 'text-2xs text-warning'}>
                    {state.isLoading ? '读取中…' : remembered ? '已记住' : '未设置'}
                </span>
            </div>

            {!loginSupported ? (
                // 1.2.4a1 之前没有登录接口，桌面端代不了——这一点必须说清楚
                <div className="mt-1 flex flex-col gap-2">
                    <p className="text-xs leading-relaxed text-text-secondary">
                        这个 NeoBot 版本的面板不支持桌面端登录：
                        {versionRequirementText(instance.installed_version, 'panelLogin')}
                        。桌面端因此读不到需要登录的面板数据（概览、模型、提示词等页会提示未授权）。
                    </p>
                    <p className="text-xs leading-relaxed text-text-secondary">
                        两条路：升级 NeoBot 之后回到本页填一次密码；或者现在就用「打开控制台」
                        在浏览器里直接使用面板——面板自己不受影响。
                    </p>
                    <div>
                        <Button size="sm" variant="primary" onClick={onOpenWebUi}>
                            打开控制台
                        </Button>
                    </div>
                </div>
            ) : needsSetup ? (
                <div className="mt-1 flex flex-col gap-2">
                    <p className="text-xs leading-relaxed text-text-secondary">
                        面板还没有设置登录密码，所以桌面的面板页现在读不到数据。
                        密码要由你来设——面板把它存成哈希，桌面端读不出也写不了，
                        <span className="text-text"> 也更不该替你设</span>。
                    </p>
                    {status?.setupAllowed ? (
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
                        不写回 NeoBot，也不参与登录接管。保存时会立刻用它登录一次，把结果告诉你。
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
                            {save.isPending ? '验证中…' : '保存并验证'}
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

            {outcome && <p className={'mt-2 text-2xs ' + outcomeColor}>{outcome.text}</p>}
            {state.isError && (
                <p className="mt-2 text-2xs text-danger">读取失败：{state.error.message}</p>
            )}
        </section>
    );
};
