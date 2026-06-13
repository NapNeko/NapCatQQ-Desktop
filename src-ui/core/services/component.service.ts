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
};
