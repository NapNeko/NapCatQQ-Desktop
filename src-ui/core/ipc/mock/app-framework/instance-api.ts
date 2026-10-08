// 实例生命周期假 API：列表 / 新建 / 探测导入 / 装卸 / 开关 / 条款 / WebUI。
// 实例表在 state.ts（含条款文本）；安装进度在 install-task.ts；条款清单项在 manifests.ts。
import type {
    AppInstance,
    AppInstanceWebUi,
    AppPendingTerms,
    AppProjectProbe,
    AppWebUiAccount,
    CreateAppInstanceRequest,
    ImportAppInstanceRequest,
    LogSnapshot,
    PackageVersions,
} from '../../types';
import { withMockDelay } from '../bootstrap.mock';
import { emitMockEvent } from '../events.mock';
import { mockAppLogTail, playMockAppRun } from '../app-log.mock';
import { createMockAppConfigApi, peekKarinHttpAuthKey } from '../app-config.mock';
import { instances, mockAcceptedTerms, MOCK_TERMS_TEXT, publish, require } from './state';
import { MOCK_NEOBOT_VERSIONS, simulateInstallTask } from './install-task';
import { mockAccountView, mockGeneratePassword, mockWebUiAccounts } from './webui-accounts';
import { mockAppFrameworks } from './manifests';

export const instanceApi = {
    /** 预览里给一份与真机同形的 Bot 侧配置，好让「原始文件」页的两段都能看到 */
    linkBotDocuments: async (_id: string) =>
        withMockDelay([
            {
                name: 'onebot11_10001.json',
                path: 'C:/ProgramData/NapCatQQ Desktop/components/NapCatQQ/config/onebot11_10001.json',
                text: JSON.stringify(
                    {
                        network: {
                            websocketClients: [
                                {
                                    name: 'ncd-app:dc59a8f1',
                                    enable: true,
                                    url: 'ws://127.0.0.1:36909/',
                                    token: 'preview-token',
                                    messagePostFormat: 'array',
                                },
                            ],
                        },
                    },
                    null,
                    4,
                ),
            },
        ]),

    listFrameworks: () => withMockDelay(mockAppFrameworks),
    listInstances: () => withMockDelay(instances.slice()),

    previewInstallDir: async (hostId: string, frameworkId: string): Promise<string> =>
        withMockDelay(
            hostId === 'local'
                ? `D:/NapCatQQ/apps/${frameworkId}`
                : `/home/ubuntu/ncd/apps/${frameworkId}`,
        ),

    create: async (req: CreateAppInstanceRequest): Promise<AppInstance> => {
        const id = Math.random().toString(16).slice(2, 10);
        const manifest = mockAppFrameworks.find((m) => m.id === req.framework_id);
        const fwName = manifest?.display_name ?? req.framework_id;
        const created: AppInstance = {
            id,
            framework_id: req.framework_id,
            display_name: req.display_name || `${fwName} · ${id}`,
            placement: req.host_id === 'local' ? 'local_native' : 'remote_native',
            host_id: req.host_id,
            install_dir:
                req.install_dir ||
                (req.host_id === 'local'
                    ? `D:/NapCatQQ/apps/${req.framework_id}/${id}`
                    : `/home/ubuntu/ncd/apps/${req.framework_id}/${id}`),
            port: req.port ?? 20000 + Math.floor(Math.random() * 29152),
            state: 'not_installed',
            created_at_ms: Date.now(),
            install_renderer: req.install_renderer ?? true,
            origin: 'created',
            auto_start: true,
        };
        if (manifest?.webui_auth === 'user_password') {
            mockWebUiAccounts.set(id, {
                username: req.webui_username?.trim() || 'astrbot',
                password: req.webui_password || mockGeneratePassword(),
            });
        }
        instances.push(created);
        emitMockEvent({ kind: 'app_instance_changed', instance: created, reason: 'created' });
        return withMockDelay(created);
    },

    probeProject: async (
        hostId: string,
        frameworkId: string,
        path: string,
    ): Promise<AppProjectProbe> => {
        const trimmed = path.trim();
        if (!trimmed) throw new Error('请填写项目目录');
        if (hostId.startsWith('remote:') && !trimmed.startsWith('/')) {
            throw new Error('远端路径必须是绝对路径');
        }
        if (/nope|not-a-project/i.test(trimmed)) {
            throw new Error('这里不是可导入的项目');
        }
        const name = trimmed.replace(/\\/g, '/').split('/').filter(Boolean).pop() ?? frameworkId;
        const isNonebot = frameworkId === 'nonebot2';
        const isAstrbot = frameworkId === 'astrbot';
        const emptyOnebot = /no-onebot|empty-platform/i.test(trimmed);
        const ambiguousOnebot = /ambiguous-onebot/i.test(trimmed);
        return withMockDelay({
            framework_id: frameworkId,
            path: trimmed,
            display_name: name,
            port:
                emptyOnebot || ambiguousOnebot
                    ? undefined
                    : isNonebot
                      ? 13120
                      : isAstrbot
                        ? 6199
                        : 7777,
            version: isNonebot ? '2.5.1' : isAstrbot ? '4.0.0' : '1.17.0',
            env_rel_path: isAstrbot ? 'data/cmd_config.json' : isNonebot ? '.env' : '.env',
            environment: isNonebot ? 'prod' : '',
            ready: true,
            running: hostId.startsWith('remote:'),
            supervisors: hostId.startsWith('remote:') && isNonebot ? ['bot-xiuxian'] : [],
            warnings: ambiguousOnebot
                ? [
                      '有多条 OneBot v11（aiocqhttp），无法唯一认领。到 AstrBot WebUI 或原文指定要对接的那条',
                  ]
                : emptyOnebot
                  ? ['还没有 OneBot v11，对接时会加一条']
                  : hostId.startsWith('remote:') && isNonebot
                    ? ['现在由 systemd 在跑（bot-xiuxian）。导入后改由这边开关，不要了可以还回去。']
                    : [],
            detected_bot_id: hostId.startsWith('remote:') && isNonebot ? '10001' : undefined,
        });
    },

    importInstance: async (req: ImportAppInstanceRequest): Promise<AppInstance> => {
        const probe = await instanceApi.probeProject(req.host_id, req.framework_id, req.path);
        const id = Math.random().toString(16).slice(2, 10);
        const imported: AppInstance = {
            id,
            framework_id: req.framework_id,
            display_name: req.display_name.trim() || probe.display_name,
            placement: req.host_id === 'local' ? 'local_native' : 'remote_native',
            host_id: req.host_id,
            install_dir: probe.path,
            port: probe.port ?? 0,
            state: probe.ready ? 'running' : 'not_installed',
            installed_version: probe.version,
            created_at_ms: Date.now(),
            install_renderer: false,
            origin: 'imported',
            auto_start: true,
            link: probe.detected_bot_id
                ? {
                      bot_id: probe.detected_bot_id,
                      mode: 'reverse_ws',
                      connection_name: 'ncd-adopt-forward',
                      linked_at_ms: Date.now(),
                  }
                : undefined,
        };
        instances.push(imported);
        emitMockEvent({ kind: 'app_instance_changed', instance: imported, reason: 'imported' });
        return withMockDelay(imported);
    },

    install: async (id: string, version?: string | null): Promise<string> => {
        const inst = require(id);
        publish({ ...inst, state: 'installing' }, 'installing');
        return withMockDelay(simulateInstallTask(inst, version ?? null));
    },

    /** 真实链路只有 NeoBot 实现 available_versions，trait 默认 None；UI 拿到 null 就隐藏选择器 */
    listVersions: async (frameworkId: string): Promise<PackageVersions | null> => {
        if (frameworkId !== 'neobot') return withMockDelay(null);
        return withMockDelay({
            name: 'neobot-app',
            versions: [...MOCK_NEOBOT_VERSIONS],
            latest: MOCK_NEOBOT_VERSIONS[0] ?? null,
            has_prerelease: MOCK_NEOBOT_VERSIONS.some((v) => /[a-zA-Z]/.test(v.slice(1))),
        });
    },

    refresh: async (id: string): Promise<AppInstance> => withMockDelay(require(id)),

    tailLog: async (id: string, _lines = 1000): Promise<LogSnapshot> => {
        const lines = mockAppLogTail(require(id));
        return withMockDelay({ lines, total_lines: lines.length });
    },

    start: async (id: string): Promise<AppInstance> => {
        const next: AppInstance = { ...require(id), state: 'running', last_error: undefined };
        publish(next, 'started');
        playMockAppRun(next, () => instances.find((i) => i.id === id)?.state === 'running');
        return withMockDelay(next);
    },

    stop: async (id: string): Promise<AppInstance> => {
        const next: AppInstance = { ...require(id), state: 'stopped' };
        publish(next, 'stopped');
        return withMockDelay(next);
    },

    delete: async (id: string): Promise<void> => {
        const at = instances.findIndex((i) => i.id === id);
        if (at >= 0) instances.splice(at, 1);
        return withMockDelay(undefined);
    },

    pendingTerms: async (instanceId: string): Promise<AppPendingTerms[]> => {
        const inst = require(instanceId);
        const manifest = mockAppFrameworks.find((m) => m.id === inst.framework_id);
        if (!manifest?.terms.length || mockAcceptedTerms.has(instanceId)) return withMockDelay([]);
        return withMockDelay(
            manifest.terms.map((t) => ({ ...t, text: MOCK_TERMS_TEXT[t.id] ?? '' })),
        );
    },

    acceptTerms: async (instanceId: string): Promise<void> => {
        mockAcceptedTerms.add(instanceId);
        return withMockDelay(undefined);
    },

    webui: async (instanceId: string, path?: string): Promise<AppInstanceWebUi> => {
        const inst = require(instanceId);
        const base =
            inst.framework_id === 'astrbot'
                ? `http://127.0.0.1:6185`
                : inst.framework_id === 'maibot' || inst.framework_id === 'koishi'
                  ? `http://127.0.0.1:${inst.port}/`
                  : `http://127.0.0.1:${inst.port}/web`;
        const suffix = path?.trim() ? (path.startsWith('/') ? path : `/${path}`) : '';
        return withMockDelay({
            url: `${base.replace(/\/$/, '')}${suffix}`,
            authKey:
                inst.framework_id === 'karin'
                    ? peekKarinHttpAuthKey(instanceId)
                    : inst.framework_id === 'maibot'
                      ? 'Ncd_mockMockMockMockMock'
                      : '',
            account: mockAccountView(inst) ?? undefined,
        });
    },

    webuiAccount: async (instanceId: string): Promise<AppWebUiAccount | null> =>
        withMockDelay(mockAccountView(require(instanceId))),

    resetWebUiPassword: async (
        instanceId: string,
        password: string | null,
    ): Promise<AppWebUiAccount> => {
        const inst = require(instanceId);
        if (inst.state === 'running') throw new Error('实例运行中，先停止再重置密码');
        const current = mockWebUiAccounts.get(instanceId) ?? {
            username: 'astrbot',
            password: null,
        };
        mockWebUiAccounts.set(instanceId, {
            username: current.username,
            password: password?.trim() || mockGeneratePassword(),
        });
        const view = mockAccountView(inst);
        if (!view) throw new Error('该应用端不是账号密码登录');
        return withMockDelay(view);
    },

    setInstanceAutoStart: async (instanceId: string, autoStart: boolean): Promise<AppInstance> => {
        const next: AppInstance = { ...require(instanceId), auto_start: autoStart };
        publish(next, 'auto_start_changed');
        return withMockDelay(next);
    },

    ...createMockAppConfigApi({ require, publish }),
};
