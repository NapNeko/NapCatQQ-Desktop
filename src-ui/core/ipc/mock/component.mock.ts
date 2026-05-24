// 浏览器预览模式下的 Component 假数据库 + 假装安装动画。
// 真 IPC 实装在 `core/services/component.service.ts`。
//
// 本 mock 模拟 6 个组件 × N 台主机的 detect 结果。"安装 / 取消"动作通过
// 内置 setInterval 一帧一帧吐 ProgressEvent 给 mock 事件总线，让前端
// UI 在浏览器预览时也能看到"装到 50%、暂停、继续"的完整动画。

import type {
    ComponentDetectResult,
    ComponentId,
    ComponentInfo,
    DomainEvent,
    ProgressEvent,
    StepKind,
} from '../types';
import { emitMockEvent } from './events.mock';

// ─── 静态元数据 ─────────────────────────────────────────────────────────

export const mockComponentCatalog: ComponentInfo[] = [
    {
        id: 'napcat',
        display_name: 'NapCat',
        description: 'OneBot v11 协议实现，主流 QQ Bot 框架',
        repo_url: 'https://github.com/NapNeko/NapCatQQ',
        supported_targets: [
            { os: 'windows', locality: 'local' },
            { os: 'linux', locality: 'remote' },
        ],
        category: 'framework',
    },
    {
        id: 'snowluma',
        display_name: 'SnowLuma',
        description: 'NTQQ 注入式后端，OneBot 替代实现',
        repo_url: 'https://github.com/SnowLuma/SnowLuma',
        supported_targets: [
            { os: 'windows', locality: 'local' },
            { os: 'linux', locality: 'remote' },
        ],
        category: 'framework',
    },
    {
        id: 'linuxqq',
        display_name: 'LinuxQQ',
        description: 'QQ for Linux 桌面客户端，远端部署的运行时基础',
        repo_url: 'https://im.qq.com/linuxqq/',
        supported_targets: [{ os: 'linux', locality: 'remote' }],
        category: 'runtime_dep',
    },
    {
        id: 'nodejs',
        display_name: 'Node.js',
        description: 'NapCat / SnowLuma 的 JavaScript 运行时',
        repo_url: 'https://nodejs.org',
        supported_targets: [
            { os: 'windows', locality: 'local' },
            { os: 'linux', locality: 'remote' },
        ],
        category: 'runtime_dep',
    },
    {
        id: 'novnc',
        display_name: 'noVNC',
        description: '远端图形栈，便于桌面 QQ 扫码登录',
        repo_url: 'https://github.com/novnc/noVNC',
        supported_targets: [{ os: 'linux', locality: 'remote' }],
        category: 'runtime_dep',
    },
    {
        id: 'desktop_self',
        display_name: 'NapCatQQ Desktop',
        description: '桌面端自身（自更新）',
        repo_url: null,
        supported_targets: [
            { os: 'windows', locality: 'local' },
            { os: 'mac_os', locality: 'local' },
            { os: 'linux', locality: 'local' },
        ],
        category: 'self_app',
    },
];

// ─── 假主机注册表 ──────────────────────────────────────────────────────
//
// 真后端的 host_id 约定是 "local" / "remote:<id>"。mock 里我们模拟一台本机
// + 两台远端，让前端的多主机 UI 能在浏览器预览。

interface MockHost {
    host_id: string;
    display_name: string;
    os: 'windows' | 'linux' | 'mac_os';
    locality: 'local' | 'remote';
}

export const mockHosts: MockHost[] = [
    { host_id: 'local', display_name: '本机', os: 'windows', locality: 'local' },
    { host_id: 'remote:production', display_name: 'remote · production', os: 'linux', locality: 'remote' },
    { host_id: 'remote:dev', display_name: 'remote · dev', os: 'linux', locality: 'remote' },
];

// ─── detect 假数据 ─────────────────────────────────────────────────────
//
// 用一张二维表表达"哪个主机上装了哪个组件 + 哪个版本"，前端调
// detectComponent(id, host_id) 时 mock 查表返回。

interface InstalledEntry {
    version: string;
    source: string;
}

const installedMatrix: Record<ComponentId, Record<string, InstalledEntry | null>> = {
    napcat: {
        local: { version: '4.18.1', source: 'napcat.mjs' },
        'remote:production': { version: '4.20.0', source: 'napcat.mjs' },
        'remote:dev': null,
    },
    snowluma: {
        local: null,
        'remote:production': null,
        'remote:dev': null,
    },
    linuxqq: {
        local: null,
        'remote:production': { version: '3.2.25-45758', source: 'qq --version' },
        'remote:dev': null,
    },
    nodejs: {
        local: { version: 'v20.10.0', source: 'node -v' },
        'remote:production': { version: 'v20.10.0', source: 'node -v' },
        'remote:dev': { version: 'v18.19.0', source: 'node -v' },
    },
    novnc: {
        local: null,
        'remote:production': null,
        'remote:dev': null,
    },
    desktop_self: {
        local: { version: '0.1.0-alpha.1', source: 'package.json' },
        'remote:production': null,
        'remote:dev': null,
    },
};

export function mockDetect(
    componentId: ComponentId,
    hostId: string,
): ComponentDetectResult {
    const info = mockComponentCatalog.find((c) => c.id === componentId);
    const host = mockHosts.find((h) => h.host_id === hostId);
    if (!info || !host) {
        return {
            component_id: componentId,
            host_id: hostId,
            detected: null,
            supported: false,
        };
    }
    // 检查 supported_targets 是否覆盖该 host
    const supported = info.supported_targets.some(
        (t) => t.os === host.os && t.locality === host.locality,
    );
    if (!supported) {
        return {
            component_id: componentId,
            host_id: hostId,
            detected: null,
            supported: false,
        };
    }
    const entry = installedMatrix[componentId][hostId] ?? null;
    return {
        component_id: componentId,
        host_id: hostId,
        detected: entry,
        supported: true,
    };
}

// ─── 假装跑 action：吐进度事件 ─────────────────────────────────────────
//
// 真后端是 mpsc → broadcast → Tauri event。mock 里直接走 events.mock 的
// emitMockEvent，让前端订阅 component_action_progress 时能收到。

const activeMockTasks = new Map<string, ReturnType<typeof setInterval>>();
let taskCounter = 0;

export function mockRunAction(
    componentId: ComponentId,
    hostId: string,
    kind: StepKind,
): string {
    taskCounter += 1;
    const taskId = `mock-task-${Date.now()}-${taskCounter}`;
    const startedAt = Date.now();

    const emit = (kindEvt: ProgressEvent) => {
        emitMockEvent({
            kind: 'component_action_progress',
            task_id: taskId,
            event: kindEvt,
        } as DomainEvent);
    };

    // started
    setTimeout(() => {
        emit({ v: 1, timestamp_ms: Date.now(), kind: 'started', total_steps: 1 });
        emit({
            v: 1,
            timestamp_ms: Date.now(),
            kind: 'step_begin',
            step: 1,
            message: stepLabel(componentId, kind),
        });
    }, 50);

    // 进度推进：300ms 一帧，10 帧到 100%
    let progress = 0;
    const intervalId = setInterval(() => {
        progress += 10;
        if (progress >= 100) {
            clearInterval(intervalId);
            activeMockTasks.delete(taskId);

            emit({
                v: 1,
                timestamp_ms: Date.now(),
                kind: 'step_progress',
                step: 1,
                percent: 100,
                message: '完成',
            });
            emit({
                v: 1,
                timestamp_ms: Date.now(),
                kind: 'step_end',
                step: 1,
                ok: true,
            });
            emit({ v: 1, timestamp_ms: Date.now(), kind: 'finished', ok: true });

            // 完成后更新 installedMatrix（让下次 detect 拿到新状态）
            applyMockOutcome(componentId, hostId, kind);
            return;
        }
        emit({
            v: 1,
            timestamp_ms: Date.now(),
            kind: 'step_progress',
            step: 1,
            percent: progress,
            message: `${stepLabel(componentId, kind)} ${progress}%`,
        });
    }, 300);
    activeMockTasks.set(taskId, intervalId);

    void startedAt; // mute lint
    return taskId;
}

export function mockCancelAction(taskId: string): void {
    const interval = activeMockTasks.get(taskId);
    if (!interval) return;
    clearInterval(interval);
    activeMockTasks.delete(taskId);
    emitMockEvent({
        kind: 'component_action_progress',
        task_id: taskId,
        event: { v: 1, timestamp_ms: Date.now(), kind: 'finished', ok: false },
    } as DomainEvent);
}

function stepLabel(componentId: ComponentId, kind: StepKind): string {
    const info = mockComponentCatalog.find((c) => c.id === componentId);
    const name = info?.display_name ?? componentId;
    switch (kind) {
        case 'ensure_installed':
        case 'force_install':
            return `正在安装 ${name}`;
        case 'update':
            return `正在更新 ${name}`;
        case 'uninstall':
            return `正在卸载 ${name}`;
        case 'verify':
            return `正在校验 ${name}`;
    }
}

function applyMockOutcome(
    componentId: ComponentId,
    hostId: string,
    kind: StepKind,
): void {
    const matrix = installedMatrix[componentId];
    if (!matrix) return;
    if (kind === 'uninstall') {
        matrix[hostId] = null;
        return;
    }
    if (kind === 'ensure_installed' || kind === 'force_install' || kind === 'update') {
        matrix[hostId] = {
            version: kind === 'update' ? '4.20.0' : '4.18.1',
            source: 'mock',
        };
    }
}
