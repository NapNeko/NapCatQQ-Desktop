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
    AppPluginConfigSchema,
    AppStoreInstalled,
    AppStoreMarketEntry,
    AppStoreResource,
    AppProjectProbe,
    AppWebUiAccount,
    AstrBotAbconfInfo,
    AstrBotDashboardStatus,
    AstrBotKbCreate,
    AstrBotKnowledgeBase,
    AstrBotPersona,
    AstrBotSessionRule,
    CreateAppInstanceRequest,
    ImportAppInstanceRequest,
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

    /** 提交安装任务到部署队列，返回 task_id；完成后后端自动刷实例状态。 */
    install: async (instanceId: string): Promise<string> => {
        if (!isTauri) return mockAppFrameworkApi.install(instanceId);
        return invoke<string>('install_app_instance', { instanceId });
    },

    refresh: async (instanceId: string): Promise<AppInstance> => {
        if (!isTauri) return mockAppFrameworkApi.refresh(instanceId);
        return invoke<AppInstance>('refresh_app_instance', { instanceId });
    },

    tailLog: async (
        instanceId: string,
        lines = 1000,
    ): Promise<{ lines: string[]; total_lines: number }> => {
        if (!isTauri) return mockAppFrameworkApi.tailLog(instanceId, lines);
        return invoke('tail_app_instance_log', { instanceId, lines });
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
        confId?: string | null,
    ): Promise<AppConfigWriteResult> => {
        if (!isTauri) return mockAppFrameworkApi.writeConfig(instanceId, config, baseRevision, confId);
        return invoke<AppConfigWriteResult>('write_app_instance_config', {
            instanceId,
            config,
            baseRevision,
            confId: confId ?? null,
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

    listStore: async (
        frameworkId: string,
        resource: AppStoreResource,
    ): Promise<AppStoreMarketEntry[]> => {
        if (!isTauri) return mockAppFrameworkApi.listStore(frameworkId, resource);
        return invoke<AppStoreMarketEntry[]>('list_app_store', { frameworkId, resource });
    },

    listStoreInstalled: async (
        instanceId: string,
        resource: AppStoreResource,
    ): Promise<AppStoreInstalled[]> => {
        if (!isTauri) return mockAppFrameworkApi.listStoreInstalled(instanceId, resource);
        return invoke<AppStoreInstalled[]>('list_app_store_installed', { instanceId, resource });
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

    /** null = 该插件没有表单 schema，只能改原文。 */
    pluginConfigSchema: async (
        instanceId: string,
        pluginName: string,
    ): Promise<AppPluginConfigSchema | null> => {
        if (!isTauri) return mockAppFrameworkApi.pluginConfigSchema(instanceId, pluginName);
        return invoke<AppPluginConfigSchema | null>('get_app_plugin_config_schema', {
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
        resource?: AppStoreResource,
    ): Promise<string> => {
        if (!isTauri) {
            return mockAppFrameworkApi.submitPluginOp(instanceId, pluginName, action, resource);
        }
        return invoke<string>('submit_app_plugin_op', {
            instanceId,
            pluginName,
            action,
            resource: resource ?? null,
        });
    },

    setPluginEnabled: async (
        instanceId: string,
        pluginName: string,
        enabled: boolean,
        overwrite?: boolean,
        resource?: AppStoreResource,
    ): Promise<AppConfigWriteResult> => {
        if (!isTauri) {
            return mockAppFrameworkApi.setPluginEnabled(
                instanceId,
                pluginName,
                enabled,
                overwrite,
                resource,
            );
        }
        return invoke<AppConfigWriteResult>('set_app_plugin_enabled', {
            instanceId,
            pluginName,
            enabled,
            overwrite: overwrite ?? null,
            resource: resource ?? null,
        });
    },

    astrbotDashboardStatus: async (instanceId: string): Promise<AstrBotDashboardStatus> => {
        if (!isTauri) return mockAppFrameworkApi.astrbotDashboardStatus(instanceId);
        return invoke<AstrBotDashboardStatus>('astrbot_dashboard_status', { instanceId });
    },

    astrbotListPersonas: async (instanceId: string): Promise<AstrBotPersona[]> => {
        if (!isTauri) return mockAppFrameworkApi.astrbotListPersonas(instanceId);
        return invoke<AstrBotPersona[]>('astrbot_list_personas', { instanceId });
    },

    astrbotUpsertPersona: async (
        instanceId: string,
        persona: AstrBotPersona,
        creating: boolean,
    ): Promise<AstrBotPersona[]> => {
        if (!isTauri) return mockAppFrameworkApi.astrbotUpsertPersona(instanceId, persona, creating);
        return invoke<AstrBotPersona[]>('astrbot_upsert_persona', { instanceId, persona, creating });
    },

    astrbotDeletePersona: async (instanceId: string, personaId: string): Promise<AstrBotPersona[]> => {
        if (!isTauri) return mockAppFrameworkApi.astrbotDeletePersona(instanceId, personaId);
        return invoke<AstrBotPersona[]>('astrbot_delete_persona', { instanceId, personaId });
    },

    astrbotListKbs: async (instanceId: string): Promise<AstrBotKnowledgeBase[]> => {
        if (!isTauri) return mockAppFrameworkApi.astrbotListKbs(instanceId);
        return invoke<AstrBotKnowledgeBase[]>('astrbot_list_kbs', { instanceId });
    },

    astrbotCreateKb: async (
        instanceId: string,
        request: AstrBotKbCreate,
    ): Promise<AstrBotKnowledgeBase[]> => {
        if (!isTauri) return mockAppFrameworkApi.astrbotCreateKb(instanceId, request);
        return invoke<AstrBotKnowledgeBase[]>('astrbot_create_kb', { instanceId, request });
    },

    astrbotDeleteKb: async (instanceId: string, kbId: string): Promise<AstrBotKnowledgeBase[]> => {
        if (!isTauri) return mockAppFrameworkApi.astrbotDeleteKb(instanceId, kbId);
        return invoke<AstrBotKnowledgeBase[]>('astrbot_delete_kb', { instanceId, kbId });
    },

    astrbotListSessionRules: async (instanceId: string): Promise<AstrBotSessionRule[]> => {
        if (!isTauri) return mockAppFrameworkApi.astrbotListSessionRules(instanceId);
        return invoke<AstrBotSessionRule[]>('astrbot_list_session_rules', { instanceId });
    },

    astrbotUpdateSessionRule: async (
        instanceId: string,
        rule: AstrBotSessionRule,
    ): Promise<AstrBotSessionRule[]> => {
        if (!isTauri) return mockAppFrameworkApi.astrbotUpdateSessionRule(instanceId, rule);
        return invoke<AstrBotSessionRule[]>('astrbot_update_session_rule', { instanceId, rule });
    },

    astrbotDeleteSessionRule: async (
        instanceId: string,
        umo: string,
        ruleKey: string,
    ): Promise<AstrBotSessionRule[]> => {
        if (!isTauri) return mockAppFrameworkApi.astrbotDeleteSessionRule(instanceId, umo, ruleKey);
        return invoke<AstrBotSessionRule[]>('astrbot_delete_session_rule', {
            instanceId,
            umo,
            ruleKey,
        });
    },

    astrbotListAbconfs: async (instanceId: string): Promise<AstrBotAbconfInfo[]> => {
        if (!isTauri) return mockAppFrameworkApi.astrbotListAbconfs(instanceId);
        return invoke<AstrBotAbconfInfo[]>('astrbot_list_abconfs', { instanceId });
    },

    astrbotCreateAbconf: async (instanceId: string, name: string): Promise<AstrBotAbconfInfo[]> => {
        if (!isTauri) return mockAppFrameworkApi.astrbotCreateAbconf(instanceId, name);
        return invoke<AstrBotAbconfInfo[]>('astrbot_create_abconf', { instanceId, name });
    },

    astrbotDeleteAbconf: async (instanceId: string, abconfId: string): Promise<AstrBotAbconfInfo[]> => {
        if (!isTauri) return mockAppFrameworkApi.astrbotDeleteAbconf(instanceId, abconfId);
        return invoke<AstrBotAbconfInfo[]>('astrbot_delete_abconf', { instanceId, abconfId });
    },

    astrbotListSourceModels: async (instanceId: string, sourceId: string): Promise<string[]> => {
        if (!isTauri) return mockAppFrameworkApi.astrbotListSourceModels(instanceId, sourceId);
        return invoke<string[]>('astrbot_list_source_models', { instanceId, sourceId });
    },

    astrbotListSubagentTools: async (instanceId: string): Promise<string[]> => {
        if (!isTauri) return mockAppFrameworkApi.astrbotListSubagentTools(instanceId);
        return invoke<string[]>('astrbot_list_subagent_tools', { instanceId });
    },
};
