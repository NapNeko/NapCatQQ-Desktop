// 应用端框架（Karin 等）IPC。
// 命令字面量只在此文件出现（R3）；浏览器预览走 mock。

import { invoke, isTauri } from '../ipc/transport';
import type {
    AppConfigDocument,
    AppConfigText,
    AppConfigWriteResult,
    AppFrameworkManifest,
    AppInstance,
    AppInstanceConfig,
    AppInstanceConfigEnvelope,
    AppInstanceWebUi,
    AppPluginAction,
    CreateAppInstanceRequest,
    KarinPluginInstalled,
    KarinPluginMarketEntry,
    OneBotLinkPlan,
} from '../ipc/types';
import { mockAppFrameworkApi } from '../ipc/mock/app-framework.mock';

export const appFrameworkService = {
    listFrameworks: async (): Promise<AppFrameworkManifest[]> => {
        if (!isTauri) return mockAppFrameworkApi.listFrameworks();
        return invoke<AppFrameworkManifest[]>('list_app_frameworks');
    },

    listInstances: async (): Promise<AppInstance[]> => {
        if (!isTauri) return mockAppFrameworkApi.listInstances();
        return invoke<AppInstance[]>('list_app_instances');
    },

    create: async (request: CreateAppInstanceRequest): Promise<AppInstance> => {
        if (!isTauri) return mockAppFrameworkApi.create(request);
        return invoke<AppInstance>('create_app_instance', { request });
    },

    previewInstallDir: async (hostId: string, frameworkId: string): Promise<string> => {
        if (!isTauri) return mockAppFrameworkApi.previewInstallDir(hostId, frameworkId);
        return invoke<string>('preview_app_install_dir', { hostId, frameworkId });
    },

    /** 提交安装任务到部署队列，返回 task_id；完成后后端自动刷实例状态。 */
    install: async (instanceId: string): Promise<string> => {
        if (!isTauri) return mockAppFrameworkApi.install(instanceId);
        return invoke<string>('install_app_instance', { instanceId });
    },

    refresh: async (instanceId: string): Promise<AppInstance> => {
        if (!isTauri) return mockAppFrameworkApi.refresh(instanceId);
        return invoke<AppInstance>('refresh_app_instance', { instanceId });
    },

    start: async (instanceId: string): Promise<AppInstance> => {
        if (!isTauri) return mockAppFrameworkApi.start(instanceId);
        return invoke<AppInstance>('start_app_instance', { instanceId });
    },

    stop: async (instanceId: string): Promise<AppInstance> => {
        if (!isTauri) return mockAppFrameworkApi.stop(instanceId);
        return invoke<AppInstance>('stop_app_instance', { instanceId });
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

    webui: async (instanceId: string): Promise<AppInstanceWebUi> => {
        if (!isTauri) return mockAppFrameworkApi.webui(instanceId);
        return invoke<AppInstanceWebUi>('get_app_instance_webui', { instanceId });
    },

    // ---- 实例配置（类型化 + 原始文件）。失败 reject 的是 AppConfigError 结构体，见 isAppConfigError ----

    readConfig: async (instanceId: string): Promise<AppInstanceConfigEnvelope> => {
        if (!isTauri) return mockAppFrameworkApi.readConfig(instanceId);
        return invoke<AppInstanceConfigEnvelope>('read_app_instance_config', { instanceId });
    },

    /** `baseRevision` 传 null 表示用户选择覆盖别处的改动。 */
    writeConfig: async (
        instanceId: string,
        config: AppInstanceConfig,
        baseRevision: string | null,
    ): Promise<AppConfigWriteResult> => {
        if (!isTauri) return mockAppFrameworkApi.writeConfig(instanceId, config, baseRevision);
        return invoke<AppConfigWriteResult>('write_app_instance_config', {
            instanceId,
            config,
            baseRevision,
        });
    },

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
        if (!isTauri) {
            return mockAppFrameworkApi.writeConfigText(instanceId, docId, text, baseRevision);
        }
        return invoke<AppConfigText>('write_app_config_text', {
            instanceId,
            docId,
            text,
            baseRevision,
        });
    },

    listPluginMarket: async (): Promise<KarinPluginMarketEntry[]> => {
        if (!isTauri) return mockAppFrameworkApi.listPluginMarket();
        return invoke<KarinPluginMarketEntry[]>('list_karin_plugin_market');
    },

    listPluginConfigDocs: async (
        instanceId: string,
        pluginName: string,
    ): Promise<AppConfigDocument[]> => {
        if (!isTauri) return mockAppFrameworkApi.listPluginConfigDocs(instanceId, pluginName);
        return invoke<AppConfigDocument[]>('list_app_plugin_config_docs', {
            instanceId,
            pluginName,
        });
    },

    listPlugins: async (instanceId: string): Promise<KarinPluginInstalled[]> => {
        if (!isTauri) return mockAppFrameworkApi.listPlugins(instanceId);
        return invoke<KarinPluginInstalled[]>('list_app_instance_plugins', { instanceId });
    },

    submitPluginOp: async (
        instanceId: string,
        pluginName: string,
        action: AppPluginAction,
    ): Promise<string> => {
        if (!isTauri) return mockAppFrameworkApi.submitPluginOp(instanceId, pluginName, action);
        return invoke<string>('submit_app_plugin_op', { instanceId, pluginName, action });
    },

    setPluginEnabled: async (
        instanceId: string,
        pluginName: string,
        enabled: boolean,
        overwrite?: boolean,
    ): Promise<AppConfigWriteResult> => {
        if (!isTauri) {
            return mockAppFrameworkApi.setPluginEnabled(instanceId, pluginName, enabled, overwrite);
        }
        return invoke<AppConfigWriteResult>('set_app_plugin_enabled', {
            instanceId,
            pluginName,
            enabled,
            overwrite: overwrite ?? null,
        });
    },
};
