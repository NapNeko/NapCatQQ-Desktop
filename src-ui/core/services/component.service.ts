// Component 安装 / 探测 / 操作 IPC 服务。
// 唯一持有 list_components / detect_component / run_component_action /
// cancel_component_action 这 4 个命令名字符串的位置（R3：单一字面量来源）。
//
// 后端工单 3：4 个 command 暴露 ncd-deploy DeployPlan 单 step 路径。
// 浏览器预览模式直接走 mock，给前端 UI 实时进度动画。

import { invoke, isTauri } from '../ipc/transport';
import type {
    ComponentDetectResult,
    ComponentId,
    ComponentInfo,
    StepKind,
} from '../ipc/types';
import type { QqDependencyReport } from '../ipc/generated/qq/QqDependencyReport';
import type { InstallDependenciesResult } from '../ipc/generated/qq/InstallDependenciesResult';
import {
    mockCancelAction,
    mockComponentCatalog,
    mockDetect,
    mockRunAction,
} from '../ipc/mock/component.mock';
import { withMockDelay } from '../ipc/mock/bootstrap.mock';

export const componentService = {
    listComponents: async (): Promise<ComponentInfo[]> => {
        if (isTauri) return invoke<ComponentInfo[]>('list_components');
        return withMockDelay(mockComponentCatalog, 200);
    },

    detectComponent: async (
        componentId: ComponentId,
        hostId: string,
    ): Promise<ComponentDetectResult> => {
        if (isTauri) {
            return invoke<ComponentDetectResult>('detect_component', {
                componentId,
                hostId,
            });
        }
        return withMockDelay(mockDetect(componentId, hostId), 150);
    },

    runComponentAction: async (
        componentId: ComponentId,
        hostId: string,
        kind: StepKind,
        taskId?: string,
    ): Promise<string> => {
        if (isTauri) {
            return invoke<string>('run_component_action', {
                componentId,
                hostId,
                kind,
                taskId: taskId ?? null,
            });
        }
        return withMockDelay(mockRunAction(componentId, hostId, kind), 50);
    },

    cancelComponentAction: async (taskId: string): Promise<void> => {
        if (isTauri) {
            return invoke<void>('cancel_component_action', { taskId });
        }
        mockCancelAction(taskId);
    },

    // QQ 系统依赖检测（仅 Linux 远端）。
    detectQqDependencies: async (
        hostId: string,
    ): Promise<QqDependencyReport> => {
        return invoke<QqDependencyReport>('detect_qq_dependencies', { hostId });
    },

    // QQ 系统依赖安装（仅 Linux 远端）。
    // sudoPassword: 前端弹框收集到的 sudo 密码，传给后端注入 Host 执行安装。
    // None 时后端自动从 keyring 找缓存密码。
    installQqDependencies: async (
        hostId: string,
        packages: string[],
        sudoPassword?: string,
    ): Promise<InstallDependenciesResult> => {
        return invoke<InstallDependenciesResult>('install_qq_dependencies', {
            hostId,
            packages,
            sudoPassword: sudoPassword ?? null,
        });
    },

    // 记住远端服务器的 sudo 密码（用于提权操作）。
    rememberSudoPassword: async (
        serverId: string,
        password: string,
    ): Promise<void> => {
        return invoke<void>('remember_sudo_password', {
            serverId,
            password,
        });
    },
};
