// 应用端框架（Karin 等）IPC。
// 命令字面量只在此文件出现，后端改命令名、查前端调了哪些命令都只看这一处；浏览器预览走 mock。

import { invoke, isTauri } from '../ipc/transport';
import type {
    AppConfigDocument,
    AppConfigText,
    AppFrameworkManifest,
    AppInstance,
    AppInstanceWebUi,
    AppPendingTerms,
    AppProjectProbe,
    AppWebUiAccount,
    CreateAppInstanceRequest,
    ImportAppInstanceRequest,
    LogSnapshot,
    OneBotLinkPlan,
    PackageVersions,
} from '../ipc/types';
import { mockAppFrameworkApi } from '../ipc/mock/app-framework.mock';

export const appFrameworkService = {
    listConfigDocuments: async (instanceId: string): Promise<AppConfigDocument[]> => {
        if (!isTauri) return mockAppFrameworkApi.listConfigDocuments(instanceId);
        return invoke<AppConfigDocument[]>('list_app_config_documents', { instanceId });
    },

    readConfigText: async (instanceId: string, docId: string): Promise<AppConfigText> => {
        if (!isTauri) return mockAppFrameworkApi.readConfigText(instanceId, docId);
        return invoke<AppConfigText>('read_app_config_text', { instanceId, docId });
    },

    writeConfigText: async (
        instanceId: string,
        docId: string,
        text: string,
        baseRevision: string | null,
    ): Promise<AppConfigText> => {
        if (!isTauri)
            return mockAppFrameworkApi.writeConfigText(instanceId, docId, text, baseRevision);
        return invoke<AppConfigText>('write_app_config_text', {
            instanceId,
            docId,
            text,
            baseRevision,
        });
    },

    listFrameworks: async (): Promise<AppFrameworkManifest[]> => {
        if (!isTauri) return mockAppFrameworkApi.listFrameworks();
        return invoke<AppFrameworkManifest[]>('list_app_frameworks');
    },

    listInstances: async (): Promise<AppInstance[]> => {
        if (!isTauri) return mockAppFrameworkApi.listInstances();
        return invoke<AppInstance[]>('list_app_instances');
    },

    openDirectory: async (instanceId: string): Promise<void> => {
        if (!isTauri) throw new Error('只能在桌面客户端中打开目录');
        await invoke<void>('open_app_instance_directory', { instanceId });
    },

    create: async (request: CreateAppInstanceRequest): Promise<AppInstance> => {
        if (!isTauri) return mockAppFrameworkApi.create(request);
        return invoke<AppInstance>('create_app_instance', { request });
    },

    probeProject: async (
        hostId: string,
        frameworkId: string,
        path: string,
    ): Promise<AppProjectProbe> => {
        if (!isTauri) return mockAppFrameworkApi.probeProject(hostId, frameworkId, path);
        return invoke<AppProjectProbe>('probe_app_project', { hostId, frameworkId, path });
    },

    importInstance: async (request: ImportAppInstanceRequest): Promise<AppInstance> => {
        if (!isTauri) return mockAppFrameworkApi.importInstance(request);
        return invoke<AppInstance>('import_app_instance', { request });
    },

    previewInstallDir: async (hostId: string, frameworkId: string): Promise<string> => {
        if (!isTauri) return mockAppFrameworkApi.previewInstallDir(hostId, frameworkId);
        return invoke<string>('preview_app_install_dir', { hostId, frameworkId });
    },

    /**
     * 提交安装任务到部署队列，返回 task_id；完成后后端自动刷实例状态。
     * `version` 指定安装的版本（不传 = 最新正式版）；框架不支持按版本安装时忽略。
     */
    install: async (instanceId: string, version?: string | null): Promise<string> => {
        if (!isTauri) return mockAppFrameworkApi.install(instanceId, version);
        return invoke<string>('install_app_instance', { instanceId, version: version ?? null });
    },

    /**
     * 该框架支持的可安装版本；null = 不支持按版本安装（UI 隐藏版本选择器）。
     */
    listVersions: async (frameworkId: string): Promise<PackageVersions | null> => {
        if (!isTauri) return mockAppFrameworkApi.listVersions(frameworkId);
        return invoke<PackageVersions | null>('list_app_framework_versions', { frameworkId });
    },

    refresh: async (instanceId: string): Promise<AppInstance> => {
        if (!isTauri) return mockAppFrameworkApi.refresh(instanceId);
        return invoke<AppInstance>('refresh_app_instance', { instanceId });
    },

    tailLog: async (instanceId: string, lines = 1000): Promise<LogSnapshot> => {
        if (!isTauri) return mockAppFrameworkApi.tailLog(instanceId, lines);
        return invoke<LogSnapshot>('tail_app_instance_log', { instanceId, lines });
    },

    start: async (instanceId: string): Promise<AppInstance> => {
        if (!isTauri) return mockAppFrameworkApi.start(instanceId);
        return invoke<AppInstance>('start_app_instance', { instanceId });
    },

    stop: async (instanceId: string): Promise<AppInstance> => {
        if (!isTauri) return mockAppFrameworkApi.stop(instanceId);
        return invoke<AppInstance>('stop_app_instance', { instanceId });
    },

    /** 启动前要用户（重新）同意的上游条款；没有条款的框架恒为空 */
    pendingTerms: async (instanceId: string): Promise<AppPendingTerms[]> => {
        if (!isTauri) return mockAppFrameworkApi.pendingTerms(instanceId);
        return invoke<AppPendingTerms[]>('app_pending_terms', { instanceId });
    },

    acceptTerms: async (instanceId: string): Promise<void> => {
        if (!isTauri) return mockAppFrameworkApi.acceptTerms(instanceId);
        return invoke<void>('accept_app_terms', { instanceId });
    },

    delete: async (instanceId: string, removeFiles: boolean): Promise<void> => {
        if (!isTauri) return mockAppFrameworkApi.delete(instanceId);
        return invoke<void>('delete_app_instance', { instanceId, removeFiles });
    },

    previewLink: async (instanceId: string, botId: string): Promise<OneBotLinkPlan> => {
        if (!isTauri) return mockAppFrameworkApi.previewLink(instanceId, botId);
        return invoke<OneBotLinkPlan>('preview_app_link', { instanceId, botId });
    },

    applyLink: async (instanceId: string, botId: string): Promise<AppInstance> => {
        if (!isTauri) return mockAppFrameworkApi.applyLink(instanceId, botId);
        return invoke<AppInstance>('apply_app_link', { instanceId, botId });
    },

    unlink: async (instanceId: string): Promise<AppInstance> => {
        if (!isTauri) return mockAppFrameworkApi.unlink(instanceId);
        return invoke<AppInstance>('unlink_app_instance', { instanceId });
    },

    webui: async (instanceId: string, path?: string): Promise<AppInstanceWebUi> => {
        if (!isTauri) return mockAppFrameworkApi.webui(instanceId, path);
        return invoke<AppInstanceWebUi>('get_app_instance_webui', {
            instanceId,
            path: path ?? null,
        });
    },

    /** 只读账号，不建隧道；实例停着时查看 / 重置用。null = 该框架不是账号密码登录。 */
    webuiAccount: async (instanceId: string): Promise<AppWebUiAccount | null> => {
        if (!isTauri) return mockAppFrameworkApi.webuiAccount(instanceId);
        return invoke<AppWebUiAccount | null>('get_app_instance_webui_account', { instanceId });
    },

    /** `password` 传 null 让后端随机生成；实例须已停止。失败 reject 的是 AppConfigError。 */
    resetWebUiPassword: async (
        instanceId: string,
        password: string | null,
    ): Promise<AppWebUiAccount> => {
        if (!isTauri) return mockAppFrameworkApi.resetWebUiPassword(instanceId, password);
        return invoke<AppWebUiAccount>('reset_app_instance_webui_password', {
            instanceId,
            password,
        });
    },

    setInstanceAutoStart: async (instanceId: string, autoStart: boolean): Promise<AppInstance> => {
        if (!isTauri) return mockAppFrameworkApi.setInstanceAutoStart(instanceId, autoStart);
        return invoke<AppInstance>('set_app_instance_auto_start', { instanceId, autoStart });
    },
};
