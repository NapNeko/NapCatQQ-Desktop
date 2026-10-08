// 组件页 Docker 安装与 sudo 提权弹框：装 Docker、QQ 依赖修复共用一个密码框，
// purpose 分流重试路径。QQ 任务因缺提权失败的后端标记也在这里监听并转成弹框。
// 从 ComponentsPage.next 外提，页面只拿 handle* 回调与弹框状态。

import { useCallback, useEffect, useState, useSyncExternalStore } from 'react';
import { globalInfoBarStore } from '../../hooks/ui/globalInfoBarStore';
import { pushErrorBar } from '../../hooks/ui/pushErrorBar';
import { componentActionStore } from '../../hooks/components/componentActionStore';
import { errorText } from '../../core/domain/errors';
import { findQqSudoElevationTask } from '../../core/domain/components/qqProbe';
import type { SudoPromptTarget } from '../../core/domain/components/qqProbe';
import type { ComponentId, DockerFlavor, DockerInstallReport } from '../../core/ipc/types';
import type { DockerInstallOptions } from '../../hooks/docker/useDockerHosts';

// componentActionStore 跨路由存活,提权提示的去重状态也必须保持同样生命周期。
const qqSudoPromptedTaskIds = new Set<string>();

export interface DockerSudoOpsInput {
    hostNameOf: (hostId: string) => string;
    installDocker: (hostId: string, options?: DockerInstallOptions) => Promise<DockerInstallReport>;
    rememberSudoPassword: (serverId: string, password: string) => Promise<void>;
    startAction: (
        componentId: ComponentId,
        hostId: string,
        stepKind: 'ensure_dependencies',
    ) => Promise<string>;
    onTaskTerminal: (
        taskId: string,
        cb: (status: 'success' | 'failed' | 'cancelled') => void,
    ) => void;
    refetch: () => void;
    probeQqDependencies: (hostId: string, force?: boolean) => Promise<void>;
}

export interface DockerSudoOps {
    sudoPrompt: SudoPromptTarget | null;
    closeSudoPrompt: () => void;
    handleInstallDocker: (hostId: string) => Promise<void>;
    handleDockerDeployError: (hostId: string, flavor: DockerFlavor, err: unknown) => void;
    startQqDepsRepair: (hostId: string) => Promise<void>;
    handleSudoConfirm: (password: string, remember: boolean) => Promise<void>;
}

export function useDockerSudoOps(args: DockerSudoOpsInput): DockerSudoOps {
    const {
        hostNameOf,
        installDocker,
        rememberSudoPassword,
        startAction,
        onTaskTerminal,
        refetch,
        probeQqDependencies,
    } = args;

    // Docker / QQ 依赖补全：需要 sudo 时弹密码框，记住后重试。
    const [sudoPrompt, setSudoPrompt] = useState<SudoPromptTarget | null>(null);

    // 执行一次安装并按 status 分流。返回 report 给调用方(弹框重试时要据此判断
    // 是否仍需密码)。底层 IPC 失败(连接断等)会抛,交给调用方处理。
    const runInstall = useCallback(
        async (hostId: string, options?: DockerInstallOptions): Promise<DockerInstallReport> => {
            const hostName = hostNameOf(hostId);
            const report = await installDocker(hostId, options);
            switch (report.status) {
                case 'installed':
                case 'alreadyInstalled':
                    globalInfoBarStore.push({
                        key: `docker-install:${hostId}`,
                        tone: 'success',
                        title: `Docker · ${hostName}`,
                        content: report.message,
                        autoDismissMs: 8000,
                    });
                    break;
                case 'manualRequired':
                    globalInfoBarStore.push({
                        key: `docker-install:${hostId}`,
                        tone: 'danger',
                        title: `Docker 未就绪 · ${hostName}`,
                        content: report.message,
                        autoDismissMs: 0,
                    });
                    break;
                case 'needSudoPassword':
                    // 弹框(或更新已开弹框的提示文案)向用户要 sudo 密码。
                    break;
            }
            return report;
        },
        [installDocker, hostNameOf],
    );

    // 组件卡片上的"安装 Docker"按钮入口:首次尝试不带密码(后端会自己探 root/
    // 免密/keyring 缓存密码)。只有探下来确实要密码且无缓存时才弹框。
    const handleInstallDocker = useCallback(
        async (hostId: string) => {
            try {
                const report = await runInstall(hostId);
                if (report.status === 'needSudoPassword') {
                    setSudoPrompt({
                        hostId,
                        hostName: hostNameOf(hostId),
                        reason: report.message,
                        purpose: 'docker',
                    });
                }
            } catch (err) {
                pushErrorBar({
                    key: `docker-install:${hostId}`,
                    title: `Docker 安装失败 · ${hostNameOf(hostId)}`,
                    raw: errorText(err, 'Docker 安装失败，请手动安装后重试'),
                });
            }
        },
        [runInstall, hostNameOf],
    );

    const handleDockerDeployError = useCallback(
        (hostId: string, flavor: DockerFlavor, err: unknown) => {
            const framework = flavor === 'napcat' ? 'NapCat' : 'SnowLuma';
            pushErrorBar({
                key: `docker-deploy:${hostId}:${flavor}`,
                title: `${framework} Docker 部署失败 · ${hostNameOf(hostId)}`,
                raw: errorText(err, 'Docker 部署失败，请检查 Docker 状态、镜像源与端口占用后重试'),
            });
        },
        [hostNameOf],
    );

    const startQqDepsRepair = useCallback(
        async (hostId: string) => {
            try {
                const taskId = await startAction('qq', hostId, 'ensure_dependencies');
                onTaskTerminal(taskId, (status) => {
                    if (status === 'success') {
                        refetch();
                        void probeQqDependencies(hostId, true);
                    }
                });
            } catch (err) {
                pushErrorBar({
                    key: `qq-deps-repair:${hostId}`,
                    title: `QQ 依赖修复失败 · ${hostNameOf(hostId)}`,
                    raw: errorText(err, '无法启动修复任务'),
                });
            }
        },
        [startAction, onTaskTerminal, refetch, hostNameOf, probeQqDependencies],
    );

    // 弹框确认:带用户输入的密码重试。装成功就关弹框;密码不对(后端再次返回
    // needSudoPassword)就抛出去,让弹框内联显示"密码不正确"并保持打开。
    const handleSudoConfirm = useCallback(
        async (password: string, remember: boolean) => {
            if (!sudoPrompt) return;
            if (sudoPrompt.purpose === 'qq_deps') {
                const serverId = sudoPrompt.hostId.replace(/^remote:/, '');
                if (remember) {
                    await rememberSudoPassword(serverId, password);
                }
                setSudoPrompt(null);
                await startQqDepsRepair(sudoPrompt.hostId);
                return;
            }
            const report = await runInstall(sudoPrompt.hostId, {
                sudoPassword: password,
                rememberSudo: remember,
            });
            if (report.status === 'needSudoPassword') {
                throw new Error(report.message);
            }
            setSudoPrompt(null);
        },
        [sudoPrompt, runInstall, startQqDepsRepair, rememberSudoPassword],
    );

    const componentActionSnap = useSyncExternalStore(
        componentActionStore.subscribe,
        componentActionStore.getSnapshot,
        componentActionStore.getSnapshot,
    );
    useEffect(() => {
        for (const taskId of Array.from(qqSudoPromptedTaskIds)) {
            if (!(taskId in componentActionSnap.tasks)) {
                qqSudoPromptedTaskIds.delete(taskId);
            }
        }
        const hit = findQqSudoElevationTask(
            componentActionSnap.tasks,
            componentActionSnap.taskTargets,
            qqSudoPromptedTaskIds,
        );
        if (!hit) return;
        qqSudoPromptedTaskIds.add(hit.taskId);
        globalInfoBarStore.push({
            key: `qq-deps-sudo:${hit.hostId}`,
            tone: 'warning',
            title: `需要 sudo 密码 · ${hostNameOf(hit.hostId)}`,
            content: '安装 QQ 系统依赖需要提权，请输入密码后重试。',
            autoDismissMs: 0,
        });
        // 原实现用 setSudoPrompt(p => p ?? {...}) 保住已打开的 docker 弹框。
        setSudoPrompt(
            (p) =>
                p ?? {
                    hostId: hit.hostId,
                    hostName: hostNameOf(hit.hostId),
                    reason: hit.reason,
                    purpose: 'qq_deps',
                },
        );
    }, [componentActionSnap, hostNameOf]);

    return {
        sudoPrompt,
        closeSudoPrompt: () => setSudoPrompt(null),
        handleInstallDocker,
        handleDockerDeployError,
        startQqDepsRepair,
        handleSudoConfirm,
    };
}
