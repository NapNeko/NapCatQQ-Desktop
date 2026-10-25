// NeoBot 部署缺项与 OneBot 对接；具体字段由配置、模型和供应商页面编辑。

import { useEffect, useMemo, useRef, useState } from 'react';
import { CheckCircle2, Circle, Eye, EyeOff } from 'lucide-react';
import { Button, Spinner } from '../../../../shared/ui';
import { cn } from '../../../../shared/utils/cn';
import { useBotSnapshots } from '../../../../hooks/bot/useBotSnapshots';
import { useBotConfigsMap } from '../../../../hooks/bot/useBotConfigsMap';
import { useAppLinkPlan, useApplyAppLink } from '../../../../hooks/apps/useAppLink';
import { isDockerBot } from '../../../../core/domain/apps/appLinkTopology';
import { AppLinkDialog } from '../../AppLinkDialog';
import type { AppInstance } from '../../../../core/ipc/types';
import type { AppRoute } from '../../../../shared/components/next/Sidebar';
import { versionRequirementText } from '../../../../core/domain/apps/neobotCapabilities';
import { isBotAccountUnset, missingRequiredSteps } from '../../../../core/domain/apps/neobotDeploy';
import { PanelStateView } from './PanelStateView';
import { useNeoBotDeploy } from '../../../../hooks/apps/useNeoBotDeploy';
import { useClipboardText } from '../../../../hooks/ui/useClipboardText';
import { ConfirmAction, InlineNote, PanelSection } from './workspaceParts';
import { PanelDisclosure } from './PanelDisclosure';

const CopyRow: React.FC<{ label: string; value: string; hint?: string; sensitive?: boolean }> = ({
    label,
    value,
    hint,
    sensitive = false,
}) => {
    const [copied, setCopied] = useState(false);
    const [visible, setVisible] = useState(false);
    const [error, setError] = useState('');
    const writeText = useClipboardText();
    const generation = useRef(0);
    const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
    useEffect(() => {
        generation.current += 1;
        setCopied(false);
        setVisible(false);
        setError('');
        return () => {
            generation.current += 1;
            if (timer.current) clearTimeout(timer.current);
        };
    }, [value]);
    const copy = async () => {
        const current = ++generation.current;
        if (timer.current) clearTimeout(timer.current);
        try {
            await writeText(value);
            if (current !== generation.current) return;
            setError('');
            setCopied(true);
            timer.current = setTimeout(() => setCopied(false), 1500);
        } catch (cause) {
            if (current !== generation.current) return;
            setCopied(false);
            setError(cause instanceof Error ? cause.message : '复制失败，请重试');
        }
    };
    return (
        <div>
            <div className="flex flex-wrap items-center gap-2 border-b border-border-subtle py-2">
                <span className="w-20 shrink-0 text-2xs text-text-tertiary">{label}</span>
                <span className="min-w-0 flex-1 break-all font-mono-strict text-xs text-text">
                    {value ? (sensitive && !visible ? '••••••••••••' : value) : '—'}
                </span>
                {sensitive && value && (
                    <Button
                        size="sm"
                        variant="ghost"
                        aria-label={visible ? '隐藏令牌' : '显示令牌'}
                        onClick={() => setVisible(!visible)}
                    >
                        {visible ? <EyeOff size={14} /> : <Eye size={14} />}
                    </Button>
                )}
                <Button size="sm" variant="ghost" disabled={!value} onClick={copy}>
                    {copied ? '已复制' : '复制'}
                </Button>
            </div>
            {hint && <p className="mt-1 text-2xs text-text-tertiary">{hint}</p>}
            {error && (
                <p role="alert" className="mt-1 text-xs text-danger">
                    复制失败：{error}
                </p>
            )}
        </div>
    );
};

export const NeoBotDeployTab: React.FC<{
    instance: AppInstance;
    onGoTab: (tab: string) => void;
    /** 跳到机器人页去新建 Bot；外壳没给就不显示那个按钮 */
    onNavigate?: (route: AppRoute) => void;
}> = ({ instance, onGoTab, onNavigate }) => {
    const instanceId = instance.id;
    // 部署进度查询与 OneBot token 生成都在 hook 里：版本不够时整条查询不开（enabled=false）
    const { query, deploySupported, generateToken } = useNeoBotDeploy(instance);
    const [dialogOpen, setDialogOpen] = useState(false);

    const status = query.data?.kind === 'ok' ? query.data.data : null;
    const deployQq = status?.values.botAccount.trim() ?? '';
    // 「有没有填」对着面板的出厂默认值判，不写死 '0'（见 isBotAccountUnset）
    const qqUnset = status === null || isBotAccountUnset(status);

    // 桌面端有哪些 Bot（id 就是 QQ），用来把部署里填的 QQ 直接对上一个 Bot
    const { data: snapshots = [] } = useBotSnapshots({ disablePolling: true });
    const configs = useBotConfigsMap(snapshots);
    const matchedBotId = useMemo(() => {
        if (qqUnset) return null;
        const hit = snapshots.find((s) => s.bot_id === deployQq);
        if (!hit) return null;
        // Docker 部署的 Bot 连不上宿主机的回环地址，链接流程本身也会拒绝，这里先排除
        return isDockerBot(configs[hit.bot_id]?.bot.deploymentType) ? null : hit.bot_id;
    }, [deployQq, qqUnset, snapshots, configs]);

    const { plan, previewing } = useAppLinkPlan(instanceId, matchedBotId ?? '', !!matchedBotId);
    const applyLink = useApplyAppLink();

    const link = () => {
        if (!plan || !matchedBotId) return;
        applyLink.mutate({
            instanceId,
            botId: matchedBotId,
            connectionName: plan.connection.name,
        });
    };

    // 版本不够就整页换成说明：不去打那个不存在的接口
    if (!deploySupported) {
        return (
            <PanelSection title="当前版本不支持快捷部署">
                <p className="text-xs leading-relaxed text-text-secondary">
                    {versionRequirementText(instance.installed_version, 'deployApi')}
                    。可以升级 NeoBot，或在控制台完成配置后直接建立对接。
                </p>
                <div className="mt-3 flex items-center gap-2">
                    <Button size="sm" variant="primary" onClick={() => setDialogOpen(true)}>
                        直接建立对接
                    </Button>
                    <Button size="sm" variant="secondary" onClick={() => onGoTab('console')}>
                        面板凭据
                    </Button>
                </div>
                <AppLinkDialog
                    open={dialogOpen}
                    onOpenChange={setDialogOpen}
                    instanceId={instanceId}
                />
            </PanelSection>
        );
    }

    return (
        <>
            <PanelStateView
                state={query.data}
                isError={query.isError}
                errorMessage={query.error?.message}
                onRetry={() => void query.refetch()}
                onGoTab={onGoTab}
            >
                {(data) => {
                    const missing = missingRequiredSteps(data);
                    const onebot = data.onebot;
                    return (
                        <div className="flex flex-col gap-4">
                            <PanelSection
                                title="部署检查"
                                actions={
                                    <div className="flex items-center gap-2">
                                        <span
                                            className={cn(
                                                'text-2xs',
                                                data.ready ? 'text-brand' : 'text-warning',
                                            )}
                                        >
                                            {data.ready
                                                ? '必填项已就绪'
                                                : '还差 ' + missing.length + ' 项'}
                                        </span>
                                        <Button
                                            size="sm"
                                            variant="ghost"
                                            onClick={() => void query.refetch()}
                                        >
                                            刷新
                                        </Button>
                                    </div>
                                }
                            >
                                <ul className="mt-2 flex flex-col gap-1.5">
                                    {data.steps.map((s) => (
                                        <li key={s.key} className="flex items-baseline gap-2">
                                            <span
                                                className={cn(
                                                    'shrink-0 text-2xs',
                                                    s.done ? 'text-brand' : 'text-text-tertiary',
                                                )}
                                            >
                                                {s.done ? (
                                                    <CheckCircle2 size={14} aria-label="已完成" />
                                                ) : (
                                                    <Circle size={14} aria-label="待完成" />
                                                )}
                                            </span>
                                            <span className="shrink-0 text-xs text-text">
                                                {s.label}
                                            </span>
                                            {!s.required && (
                                                <span className="shrink-0 text-2xs text-text-tertiary">
                                                    选填
                                                </span>
                                            )}
                                            <span className="min-w-0 text-2xs text-text-tertiary">
                                                {s.hint}
                                            </span>
                                        </li>
                                    ))}
                                </ul>
                                <p className="mt-2 text-2xs text-text-tertiary">
                                    <button
                                        type="button"
                                        className="ml-1 text-brand hover:underline"
                                        onClick={() => onGoTab('config')}
                                    >
                                        本体配置
                                    </button>
                                    <button
                                        type="button"
                                        className="ml-3 text-brand hover:underline"
                                        onClick={() => onGoTab('env')}
                                    >
                                        供应商与密钥
                                    </button>
                                </p>
                            </PanelSection>

                            <PanelSection
                                title="OneBot 连接"
                                actions={
                                    onebot.tokenEnabled ? (
                                        <ConfirmAction
                                            label={
                                                generateToken.isPending ? '生成中…' : '重新生成令牌'
                                            }
                                            description="会替换当前令牌。已有协议 Bot 需要重新建立对接才能同步。"
                                            disabled={
                                                generateToken.isPending || applyLink.isPending
                                            }
                                            onConfirm={() => generateToken.mutate()}
                                        />
                                    ) : (
                                        <Button
                                            size="sm"
                                            variant="secondary"
                                            disabled={
                                                generateToken.isPending || applyLink.isPending
                                            }
                                            onClick={() => generateToken.mutate()}
                                        >
                                            {generateToken.isPending ? '生成中…' : '生成令牌'}
                                        </Button>
                                    )
                                }
                            >
                                {onebot.warning && (
                                    <p className="text-2xs text-warning">{onebot.warning}</p>
                                )}
                                <CopyRow
                                    label="同机地址"
                                    value={onebot.urlLocal}
                                    hint="NapCat 与 NeoBot 同机"
                                />
                                <CopyRow
                                    label="局域网地址"
                                    value={onebot.urlLan}
                                    hint="NapCat 在另一台机器"
                                />
                                <CopyRow
                                    label="访问令牌"
                                    value={onebot.token}
                                    sensitive
                                    hint={onebot.tokenEnabled ? undefined : '为空：不校验握手'}
                                />
                                {generateToken.isError && (
                                    <p className="text-2xs text-danger">
                                        生成失败：
                                        {generateToken.error instanceof Error
                                            ? generateToken.error.message
                                            : String(generateToken.error)}
                                    </p>
                                )}
                                <PanelDisclosure label="手动连接说明" className="mt-3">
                                    <p className="text-xs leading-relaxed text-text-secondary">
                                        在协议 Bot 的反向 WebSocket
                                        中填写同机或局域网地址，以及访问令牌。路径可使用{' '}
                                        {onebot.pathHint || '/onebot'}。
                                        下方建立对接会同时写入双方配置，沿用已有令牌；未设置时自动生成。
                                    </p>
                                </PanelDisclosure>
                            </PanelSection>

                            <PanelSection title="连接协议 Bot">
                                {matchedBotId ? (
                                    <>
                                        <p className="text-xs leading-relaxed text-text-secondary">
                                            机器人 QQ：{' '}
                                            <span className="font-mono text-text">{deployQq}</span>
                                            。建立对接会同步双方的连接地址与令牌。
                                        </p>
                                        {previewing && (
                                            <p className="flex items-center gap-2 text-xs text-text-secondary">
                                                <Spinner size="sm" />
                                                正在生成对接计划…
                                            </p>
                                        )}
                                        {plan && !previewing && (
                                            <p className="break-all font-mono text-2xs text-text-tertiary">
                                                将写入协议 Bot 的连接地址：
                                                {plan.connection.kind === 'ws_client'
                                                    ? plan.connection.url
                                                    : plan.connection.host +
                                                      ':' +
                                                      plan.connection.port}
                                            </p>
                                        )}
                                        <div className="flex items-center gap-2">
                                            <Button
                                                variant="primary"
                                                size="sm"
                                                disabled={
                                                    !plan ||
                                                    previewing ||
                                                    applyLink.isPending ||
                                                    generateToken.isPending
                                                }
                                                onClick={link}
                                            >
                                                {applyLink.isPending
                                                    ? '建立中…'
                                                    : '建立对接（QQ ' + deployQq + '）'}
                                            </Button>
                                            <Button
                                                size="sm"
                                                variant="ghost"
                                                disabled={
                                                    applyLink.isPending || generateToken.isPending
                                                }
                                                onClick={() => setDialogOpen(true)}
                                            >
                                                手动选择 Bot
                                            </Button>
                                        </div>
                                    </>
                                ) : (
                                    <>
                                        <p className="text-xs leading-relaxed text-text-secondary">
                                            {qqUnset
                                                ? '尚未指定机器人 QQ。先在本体配置中填写，或直接选择已有 Bot。'
                                                : `没有找到可直接对接的 QQ ${deployQq}。可以新建机器人，或手动选择已有 Bot。`}
                                        </p>
                                        <div className="flex items-center gap-2">
                                            {onNavigate && (
                                                <Button
                                                    size="sm"
                                                    variant="primary"
                                                    onClick={() => onNavigate('bots')}
                                                >
                                                    去新建机器人
                                                </Button>
                                            )}
                                            <Button
                                                size="sm"
                                                variant="secondary"
                                                onClick={() => setDialogOpen(true)}
                                            >
                                                手动选择 Bot
                                            </Button>
                                        </div>
                                    </>
                                )}
                                {applyLink.isError && (
                                    <p role="alert" className="mt-3 text-xs text-danger">
                                        建立失败：
                                        {applyLink.error instanceof Error
                                            ? applyLink.error.message
                                            : String(applyLink.error)}
                                    </p>
                                )}
                                {applyLink.isSuccess && (
                                    <div className="mt-3">
                                        <InlineNote tone="success">
                                            对接已建立，双方配置已同步。
                                        </InlineNote>
                                    </div>
                                )}
                            </PanelSection>
                        </div>
                    );
                }}
            </PanelStateView>

            {/* 手动选择 Bot：复用对接对话框，它有 Bot 下拉、拓扑校验与计划预览 */}
            <AppLinkDialog open={dialogOpen} onOpenChange={setDialogOpen} instanceId={instanceId} />
        </>
    );
};
