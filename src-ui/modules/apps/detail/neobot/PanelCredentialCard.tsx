// 面板凭据只保留在当前实例的表单中，写入密钥库后验证登录。

import { useRef, useState } from 'react';
import { Badge, Button, TextField } from '../../../../shared/ui';
import {
    useNeoBotPanelCredential,
    type VerifyOutcome,
} from '../../../../hooks/apps/useNeoBotPanelCredential';
import type { AppInstance } from '../../../../core/ipc/types';
import { versionRequirementText } from '../../../../core/domain/apps/neobotCapabilities';
import { ConfirmAction, InlineNote, PanelSection } from './workspaceParts';

const OUTCOME_TONE = {
    ok: 'success',
    bad: 'danger',
    unknown: 'info',
} as const;

interface CredentialProps {
    instance: AppInstance;
    onOpenWebUi: () => void;
}

export function PanelCredentialCard(props: CredentialProps) {
    return <CredentialForm key={props.instance.id} {...props} />;
}

function CredentialForm({ instance, onOpenWebUi }: CredentialProps) {
    const [password, setPassword] = useState('');
    const [confirmation, setConfirmation] = useState('');
    const [outcome, setOutcome] = useState<VerifyOutcome | null>(null);
    const { state, auth, loginSupported, save, setup, verify } = useNeoBotPanelCredential(instance);
    const actionLock = useRef(false);
    const busy = save.isPending || setup.isPending || verify.isPending;

    // 同一轮按键和点击也不能并发，避免旧验证覆盖新凭据的结果。
    const run = (start: (done: () => void) => void) => {
        if (busy || actionLock.current) return;
        actionLock.current = true;
        setOutcome(null);
        start(() => {
            actionLock.current = false;
        });
    };

    // 四个入口（回车 / 保存 / 清除 / 验证）共用同一块结果区
    const report = (result: VerifyOutcome) => setOutcome(result);
    const saveAndReport = (value: string) =>
        run((done) =>
            save.mutate(value, {
                onSuccess: (result) => {
                    report(result);
                    setPassword('');
                },
                onError: (err) =>
                    report({
                        state: 'unknown',
                        text: '保存失败：' + (err instanceof Error ? err.message : String(err)),
                    }),
                onSettled: done,
            }),
        );

    const remembered = state.data === true;
    const status = auth.data?.kind === 'ok' ? auth.data.data : null;
    const needsSetup = loginSupported && status !== null && !status.configured;

    const badgeTone = !loginSupported
        ? 'neutral'
        : needsSetup
          ? 'warning'
          : remembered
            ? 'success'
            : 'warning';
    const badgeText = !loginSupported
        ? '不支持桌面端登录'
        : needsSetup
          ? '待首次设置'
          : state.isLoading
            ? '读取中'
            : remembered
              ? '已记住'
              : '未保存';

    return (
        <PanelSection
            title={
                <span className="flex flex-wrap items-center gap-2">
                    面板凭据
                    <Badge tone={badgeTone}>{badgeText}</Badge>
                </span>
            }
            meta={loginSupported ? '密码仅保存在本机密钥库' : undefined}
            actions={
                loginSupported && remembered ? (
                    <Button
                        size="sm"
                        variant="ghost"
                        disabled={busy}
                        onClick={() =>
                            run((done) =>
                                verify.mutate(undefined, {
                                    onSuccess: report,
                                    onError: (err) =>
                                        report({ state: 'unknown', text: err.message }),
                                    onSettled: done,
                                }),
                            )
                        }
                    >
                        {verify.isPending ? '验证中…' : '验证已存密码'}
                    </Button>
                ) : undefined
            }
        >
            <div className="flex max-w-xl flex-col gap-3">
                {!loginSupported ? (
                    <>
                        <InlineNote tone="warning">
                            {versionRequirementText(instance.installed_version, 'panelLogin')}
                            。可通过浏览器使用面板。
                        </InlineNote>
                        <div>
                            <Button size="sm" variant="primary" onClick={onOpenWebUi}>
                                打开控制台
                            </Button>
                        </div>
                    </>
                ) : needsSetup ? (
                    status?.setupAllowed ? (
                        <>
                            <p className="text-xs leading-relaxed text-text-secondary">
                                首次设置会同时写入 NeoBot 和本机密钥库。
                            </p>
                            <TextField
                                label="新面板密码"
                                type="password"
                                autoComplete="new-password"
                                value={password}
                                disabled={busy}
                                onValueChange={setPassword}
                            />
                            <TextField
                                label="确认密码"
                                type="password"
                                autoComplete="new-password"
                                value={confirmation}
                                disabled={busy}
                                error={
                                    confirmation && password !== confirmation
                                        ? '两次输入的密码不一致'
                                        : undefined
                                }
                                onValueChange={setConfirmation}
                            />
                            <div>
                                <Button
                                    size="sm"
                                    variant="primary"
                                    disabled={busy || !password.trim() || password !== confirmation}
                                    onClick={() =>
                                        run((done) =>
                                            setup.mutate(password, {
                                                onSuccess: (result) => {
                                                    setPassword('');
                                                    setConfirmation('');
                                                    report(result);
                                                },
                                                onError: (error) =>
                                                    report({ state: 'bad', text: error.message }),
                                                onSettled: done,
                                            }),
                                        )
                                    }
                                >
                                    {setup.isPending ? '设置中…' : '设置并记住'}
                                </Button>
                            </div>
                        </>
                    ) : (
                        <InlineNote tone="info">
                            需先在 NeoBot 所在主机完成首次设置，再回到这里保存密码。
                        </InlineNote>
                    )
                ) : (
                    <>
                        <div className="flex flex-wrap items-end gap-2">
                            <TextField
                                className="w-[18rem] max-w-full"
                                type="password"
                                label="面板密码"
                                placeholder="与面板登录时输入的一致"
                                value={password}
                                autoComplete="off"
                                disabled={busy}
                                onValueChange={setPassword}
                                onKeyDown={(e) => {
                                    if (
                                        e.key === 'Enter' &&
                                        !e.nativeEvent.isComposing &&
                                        password.trim()
                                    )
                                        saveAndReport(password);
                                }}
                            />
                            <Button
                                size="sm"
                                variant="primary"
                                disabled={busy || !password.trim()}
                                onClick={() => saveAndReport(password)}
                            >
                                {save.isPending ? '验证中…' : '保存并验证'}
                            </Button>
                            {remembered && (
                                <ConfirmAction
                                    label="清除已存密码"
                                    description="仅清除本机保存的凭据，不修改 NeoBot 面板密码。再次读取面板时需要重新填写。"
                                    disabled={busy}
                                    onConfirm={() => saveAndReport('')}
                                />
                            )}
                        </div>
                        {outcome && (
                            <InlineNote tone={OUTCOME_TONE[outcome.state]}>
                                {outcome.text}
                            </InlineNote>
                        )}
                        {!outcome && remembered && (
                            <p className="text-2xs leading-relaxed text-text-tertiary">
                                面板密码变更后需在此更新。
                            </p>
                        )}
                    </>
                )}

                {loginSupported && needsSetup && outcome && (
                    <InlineNote tone={OUTCOME_TONE[outcome.state]}>{outcome.text}</InlineNote>
                )}

                {state.isError && (
                    <InlineNote tone="danger">读取失败：{state.error.message}</InlineNote>
                )}
            </div>
        </PanelSection>
    );
}
