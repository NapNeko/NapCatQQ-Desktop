// Windows 托管和用户会话状态；安装复用组件任务。
import { useState } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { RefreshCw } from 'lucide-react';
import type { ServerProfile } from '../../core/ipc/generated/domain/ServerProfile';
import { errorText } from '../../core/domain/errors';
import { useComponentAction } from '../../hooks/components/useComponentAction';
import {
    useWindowsRemoteInfo,
    WINDOWS_REMOTE_INFO_KEY,
} from '../../hooks/remote/useWindowsRemoteInfo';
import { pushErrorBar } from '../../hooks/ui/pushErrorBar';
import {
    Button,
    Dialog,
    DialogContent,
    DialogHeader,
    DialogTitle,
    DialogDescription,
    DialogFooter,
} from '../../shared/ui';
import { CopyCodeBlock } from '../../shared/ui/CopyCodeBlock';
import {
    ActionMotionIcon,
    DialogStepTransition,
    ExpandPresence,
    refreshMotion,
} from '../../shared/ui/motion';
import {
    canInstallWindowsAgent,
    registerWindowsSessionCommand,
    windowsManagementLabel,
    windowsSessionState,
    windowsWorkerUpdatePending,
} from './windows-remote-state';

export function WindowsRemoteStatus({ server }: { server: ServerProfile }) {
    const query = useWindowsRemoteInfo(server);
    const [open, setOpen] = useState(false);
    const info = query.data;
    if (!info && server.platform !== 'windows') return null;
    return (
        <>
            <button
                type="button"
                className="flex items-center gap-1.5 self-start text-2xs text-text-secondary hover:text-brand focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-brand"
                onClick={() => {
                    setOpen(true);
                    if (server.state === 'connected') void query.refetch();
                }}
            >
                <span>Windows</span>
                <span aria-hidden>·</span>
                <span>
                    {query.isPending && server.state === 'connected'
                        ? '正在探测…'
                        : windowsManagementLabel(info, server.state === 'connected')}
                </span>
                <span className="text-brand">管理</span>
            </button>
            <WindowsRemoteDetails open={open} server={server} onClose={() => setOpen(false)} />
        </>
    );
}

function WindowsRemoteDetails({
    open,
    server,
    onClose,
}: {
    open: boolean;
    server: ServerProfile;
    onClose: () => void;
}) {
    const query = useWindowsRemoteInfo(server);
    const queryClient = useQueryClient();
    const { startAction, isInstalling, onTaskTerminal } = useComponentAction();
    const [starting, setStarting] = useState(false);
    const hostId = `remote:${server.id}`;
    const installing = starting || isInstalling('windows_agent', hostId);
    const info = query.data;
    const connected = server.state === 'connected';
    const sessions =
        info?.agent?.sessions.filter((session) => session.userSid === info.userSid) ?? [];
    const ready = !!info?.agent?.serviceRunning;
    const sessionState = windowsSessionState(info, connected);
    const workerUpdatePending = windowsWorkerUpdatePending(info, connected);
    const sessionLabels = {
        offline: '状态待刷新',
        agent_unavailable: '等待托管服务',
        waiting: '等待用户桌面会话',
        ready: '工作进程已就绪',
        locked: '桌面已锁定，扫码暂不可用',
        multiple: '存在多个会话，请保留一个目标用户会话',
    };

    const install = async () => {
        if (!canInstallWindowsAgent(info, connected, ready) || installing) return;
        setStarting(true);
        try {
            const taskId = await startAction(
                'windows_agent',
                hostId,
                ready ? 'force_install' : 'ensure_installed',
            );
            onTaskTerminal(taskId, (status) => {
                if (status === 'success') {
                    void queryClient.invalidateQueries({
                        queryKey: [WINDOWS_REMOTE_INFO_KEY, server.id],
                    });
                    void queryClient.invalidateQueries({
                        queryKey: ['componentDetect', 'windows_agent', hostId],
                    });
                }
            });
        } catch (error) {
            pushErrorBar({ title: '启用 Windows 托管失败', raw: errorText(error) });
        } finally {
            setStarting(false);
        }
    };

    return (
        <Dialog
            open={open}
            onOpenChange={(next) => {
                if (!next) onClose();
            }}
        >
            <DialogContent size="md">
                <DialogHeader>
                    <DialogTitle>Windows 远程管理</DialogTitle>
                    <DialogDescription>{server.name || '远端主机'}</DialogDescription>
                </DialogHeader>
                {!connected ? (
                    <p className="text-sm text-text-secondary">请先测试 SSH 连接。</p>
                ) : null}
                {query.error && connected ? (
                    <p role="alert" className="break-words text-xs text-danger">
                        {errorText(query.error)}
                    </p>
                ) : null}
                {query.isPending && connected ? (
                    <p className="text-sm text-text-secondary">正在读取主机状态…</p>
                ) : null}
                {info ? (
                    <DialogStepTransition
                        stepKey={`${ready}:${sessionState}`}
                        className="flex min-w-0 flex-col gap-4"
                    >
                        <dl className="grid grid-cols-[5rem_minmax(0,1fr)] gap-x-3 gap-y-2 text-xs">
                            <dt className="text-text-tertiary">系统</dt>
                            <dd className="min-w-0 break-words text-text">
                                {info.osVersion} · {info.architecture}
                            </dd>
                            <dt className="text-text-tertiary">SSH 账户</dt>
                            <dd className="break-all text-text">
                                {info.username} · {info.elevated ? '管理员' : '普通用户'}
                            </dd>
                            <dt className="text-text-tertiary">托管服务</dt>
                            <dd className="min-w-0 break-words text-text">
                                {connected
                                    ? ready
                                        ? `运行中 · ${info.agent?.version}`
                                        : '未就绪'
                                    : '状态待刷新'}
                            </dd>
                            <dt className="text-text-tertiary">QQ 会话</dt>
                            <dd className="min-w-0 break-words text-text">
                                {sessionLabels[sessionState]}
                            </dd>
                            <dt className="text-text-tertiary">用户目录</dt>
                            <dd className="break-all font-mono text-text-secondary">
                                {info.homeDir}
                            </dd>
                            <dt className="text-text-tertiary">数据目录</dt>
                            <dd className="break-all font-mono text-text-secondary">
                                {info.userRoot}
                            </dd>
                        </dl>
                        {sessions.length > 0 ? (
                            <ul
                                className="flex flex-wrap gap-2 text-2xs text-text-secondary"
                                aria-label="用户会话"
                            >
                                {sessions.map((session) => (
                                    <li
                                        key={session.sessionId}
                                        className="rounded-sm bg-inset px-2 py-1"
                                    >
                                        会话 {session.sessionId} ·{' '}
                                        {session.workerUpdatePending
                                            ? '等待重新登录更新'
                                            : session.workerReady
                                              ? '已就绪'
                                              : '未就绪'}
                                        {session.locked === true ? ' · 已锁定' : ''}
                                    </li>
                                ))}
                            </ul>
                        ) : null}
                        <ExpandPresence visible={workerUpdatePending}>
                            <p role="status" className="text-xs text-warning">
                                托管服务已更新；现有QQ继续运行，重新登录目标用户后更新会话工作进程。
                            </p>
                        </ExpandPresence>
                        {!ready && connected ? (
                            <div className="flex flex-col gap-2 text-xs text-text-secondary">
                                <p>1. 启用托管服务，后台任务可在 Desktop 退出后继续。</p>
                                {!info.elevated ? (
                                    <p className="text-warning">
                                        当前 SSH 账户没有管理员权限，请在目标机以管理员身份安装
                                        Agent 后刷新。
                                    </p>
                                ) : null}
                                <p>
                                    2. 在目标 Windows 使用 {info.username}{' '}
                                    登录桌面，再完成用户会话绑定。
                                </p>
                            </div>
                        ) : null}
                        {ready && connected && sessionState === 'waiting' ? (
                            <div className="flex flex-col gap-2 text-xs text-text-secondary">
                                <p>
                                    使用 {info.username} 登录目标机桌面，在该用户的普通 PowerShell
                                    中运行一次以下命令，然后刷新。此后登录会自动启动 QQ
                                    会话工作进程。
                                </p>
                                <CopyCodeBlock command={registerWindowsSessionCommand(info)} />
                            </div>
                        ) : null}
                        {ready && connected && sessionState === 'locked' ? (
                            <p className="text-xs text-text-secondary">
                                请在目标机解锁该用户桌面后扫码。
                            </p>
                        ) : null}
                        <ExpandPresence visible={installing}>
                            <p role="status" className="text-xs text-brand">
                                正在安装，进度见任务队列。
                            </p>
                        </ExpandPresence>
                    </DialogStepTransition>
                ) : null}
                <DialogFooter>
                    <Button size="sm" variant="ghost" onClick={onClose}>
                        关闭
                    </Button>
                    <Button
                        size="sm"
                        variant="secondary"
                        disabled={!connected || query.isFetching}
                        onClick={() => void query.refetch()}
                    >
                        <ActionMotionIcon
                            icon={RefreshCw}
                            size={14}
                            motion={refreshMotion(query.isFetching)}
                        />
                        刷新状态
                    </Button>
                    <Button
                        size="sm"
                        variant={ready ? 'secondary' : 'primary'}
                        disabled={!canInstallWindowsAgent(info, connected, ready) || installing}
                        onClick={() => void install()}
                    >
                        {installing ? '部署中…' : ready ? '重新部署托管' : '启用托管'}
                    </Button>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    );
}
