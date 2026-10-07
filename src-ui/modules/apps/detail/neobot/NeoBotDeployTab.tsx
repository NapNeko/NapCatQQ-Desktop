// NeoBot 详情「部署」页：把「还差什么」列出来，并把最容易卡住的一步——把 NeoBot 和 QQ 连起来——做成一次点击。
//
// 与面板「快捷部署」的分工：那边的表单在面板里，这边不做重复的表单（改人设、密钥去面板更顺手），
// 但建立 OneBot 对接必须在这边做，因为 Bot 配置归桌面端管（面板看不到桌面端有哪些 Bot）。
//
// QQ 从哪儿来：面板的快捷部署里填过，deploy_status 的 values.bot_account 就是它。桌面端的
// Bot id 就是 QQ，所以能直接对上——对上了就能一键链接（端口与 token 由链接流程一并写进两边）。

import { useMemo, useState } from 'react';
import { Button, Spinner } from '../../../../shared/ui';
import { cn } from '../../../../shared/utils/cn';
import { useBotSnapshots } from '../../../../hooks/bot/useBotSnapshots';
import { useBotConfigsMap } from '../../../../hooks/bot/useBotConfigsMap';
import { useAppLinkPlan, useApplyAppLink } from '../../../../hooks/apps/useAppLink';
import { isDockerBot } from '../../../../core/domain/apps/appLinkTopology';
import { AppLinkDialog } from '../../AppLinkDialog';
import type { AppInstance } from '../../../../core/ipc/types';
import type { AppRoute } from '../../../../shared/components/next/Sidebar';
import { versionRequirementText } from './neobotCapabilities';
import { isBotAccountUnset, missingRequiredSteps } from './neobotDeploy';
import { PanelStateView } from './PanelStateView';
import { useNeoBotDeploy } from '../../../../hooks/apps/useNeoBotDeploy';

/** 一行「标签 + 值 + 复制」——地址与 token 都要能整段复制走 */
const CopyRow: React.FC<{ label: string; value: string; hint?: string }> = ({
    label,
    value,
    hint,
}) => {
    const [copied, setCopied] = useState(false);
    const copy = async () => {
        try {
            await navigator.clipboard.writeText(value);
            setCopied(true);
            window.setTimeout(() => setCopied(false), 1500);
        } catch {
            setCopied(false);
        }
    };
    return (
        <div className="flex items-baseline gap-2 rounded-sm border border-border-subtle bg-inset/40 px-3 py-2">
            <span className="w-20 shrink-0 text-2xs text-text-tertiary">{label}</span>
            <span className="min-w-0 flex-1 break-all font-mono text-2xs text-text">
                {value || '—'}
            </span>
            {hint && <span className="shrink-0 text-2xs text-text-tertiary">{hint}</span>}
            <Button size="sm" variant="ghost" disabled={!value} onClick={copy}>
                {copied ? '已复制' : '复制'}
            </Button>
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
            <section className="rounded-md border border-border-subtle bg-surface px-4 py-3">
                <h3 className="text-sm font-semibold text-text">这个 NeoBot 版本没有快捷部署</h3>
                <p className="mt-1 text-xs leading-relaxed text-text-secondary">
                    {versionRequirementText(instance.installed_version, 'deployApi')}
                    。旧版面板没有这个菜单，桌面端也就读不到「还差哪几项」与 OneBot 的监听信息。
                </p>
                <p className="mt-2 text-xs leading-relaxed text-text-secondary">
                    现在能做的：升级 NeoBot 之后回到本页；或先在「Web 控制台」打开面板、按面板自己的
                    说明配好，再点下面的「直接建立对接」——对接本身不依赖这个菜单。
                </p>
                <div className="mt-3 flex items-center gap-2">
                    <Button size="sm" variant="primary" onClick={() => setDialogOpen(true)}>
                        直接建立对接
                    </Button>
                    <Button size="sm" variant="secondary" onClick={() => onGoTab('console')}>
                        打开面板
                    </Button>
                </div>
                <AppLinkDialog
                    open={dialogOpen}
                    onOpenChange={setDialogOpen}
                    instanceId={instanceId}
                />
            </section>
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
                            <section className="rounded-md border border-border-subtle bg-surface px-4 py-3">
                                <div className="flex items-baseline justify-between gap-2">
                                    <h3 className="text-sm font-semibold text-text">部署进度</h3>
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
                                </div>
                                <ul className="mt-2 flex flex-col gap-1.5">
                                    {data.steps.map((s) => (
                                        <li key={s.key} className="flex items-baseline gap-2">
                                            <span
                                                className={cn(
                                                    'shrink-0 text-2xs',
                                                    s.done ? 'text-brand' : 'text-text-tertiary',
                                                )}
                                            >
                                                {s.done ? '✓' : '○'}
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
                                    人设、密钥、昵称这些在面板里改更顺手——
                                    <button
                                        type="button"
                                        className="ml-1 text-brand hover:underline"
                                        onClick={() => onGoTab('console')}
                                    >
                                        到「Web 控制台」打开面板
                                    </button>
                                </p>
                            </section>

                            <section className="flex flex-col gap-2">
                                <div className="flex items-baseline justify-between gap-2">
                                    <h3 className="text-sm font-semibold text-text">
                                        OneBot 连接（NapCat 侧要填的）
                                    </h3>
                                    <Button
                                        size="sm"
                                        variant="secondary"
                                        disabled={generateToken.isPending}
                                        onClick={() => generateToken.mutate()}
                                    >
                                        {generateToken.isPending
                                            ? '生成中…'
                                            : onebot.tokenEnabled
                                              ? '重新生成 access token'
                                              : '生成 access token'}
                                    </Button>
                                </div>
                                <p className="text-xs leading-relaxed text-text-secondary">
                                    NeoBot 是反向 WS 服务端，由 NapCat 连过来。路径由 NapCat
                                    侧自己配， 服务端不限制（面板给的惯例是{' '}
                                    {onebot.pathHint || '/onebot'}）。 下面三行里，两个地址按 NapCat
                                    在哪台机器<span className="text-text-secondary">二选一</span>，
                                    再加一个 access token——NapCat 的「反向 WS」就这两个字段。
                                </p>
                                <p className="text-2xs leading-snug text-text-tertiary">
                                    下面「一键连上 QQ」会把这些写进 NeoBot 的 [adapter]：面板里
                                    token 为空时它会生成一个并写回，已有则
                                    <span className="text-text-secondary">沿用</span>
                                    （所以先生成过 token
                                    的话不会被换掉）；端口按实例口写回，通常与上面一致。
                                </p>
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
                                    label="Access Token"
                                    value={onebot.token}
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
                                <p className="text-2xs text-text-tertiary">
                                    重新生成后，已经对接过的 NapCat 侧还是旧
                                    token——要在下面重新建立一次对接才会同步。
                                </p>
                            </section>

                            <section className="flex flex-col gap-2 rounded-md border border-border-subtle bg-surface px-4 py-3">
                                <h3 className="text-sm font-semibold text-text">一键连上 QQ</h3>
                                {matchedBotId ? (
                                    <>
                                        <p className="text-xs leading-relaxed text-text-secondary">
                                            面板的快捷部署里填的机器人 QQ 是{' '}
                                            <span className="font-mono text-text">{deployQq}</span>
                                            ，桌面端已加载的 Bot
                                            里有它。点下面的按钮会把连接建到两边： 桌面端这边建一条
                                            OneBot 连接，NeoBot 那边写进 [adapter] 的 端口与 token。
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
                                                    !plan || previewing || applyLink.isPending
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
                                                ? '面板的快捷部署还没填机器人 QQ，所以桌面端不知道该连哪个 Bot。'
                                                : '面板里填的 QQ 是 ' +
                                                  deployQq +
                                                  '，但桌面端还没有这个 Bot，所以没法一键建立对接。'}
                                        </p>
                                        <p className="text-xs leading-relaxed text-text-secondary">
                                            {qqUnset
                                                ? '先在面板的快捷部署里填上机器人 QQ，回来就能一键连；也可以现在就新建或选一个已有的 Bot。'
                                                : '两种做法：到桌面端「机器人」页新建一个 QQ 为 ' +
                                                  deployQq +
                                                  ' 的协议 Bot，回来就能一键连；或者直接手动选一个已有的 Bot。'}
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
                                    <p className="text-2xs text-danger">
                                        建立失败：
                                        {applyLink.error instanceof Error
                                            ? applyLink.error.message
                                            : String(applyLink.error)}
                                    </p>
                                )}
                                {applyLink.isSuccess && (
                                    <p className="text-2xs text-brand">
                                        已建立。两边配置都写好了，NapCat 侧的连接可以直接用。
                                    </p>
                                )}
                            </section>
                        </div>
                    );
                }}
            </PanelStateView>

            {/* 手动选择 Bot：复用对接对话框，它有 Bot 下拉、拓扑校验与计划预览 */}
            <AppLinkDialog open={dialogOpen} onOpenChange={setDialogOpen} instanceId={instanceId} />
        </>
    );
};
