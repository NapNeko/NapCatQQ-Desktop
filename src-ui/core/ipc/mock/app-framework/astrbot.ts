// AstrBot 的假数据与 WebUI 能力转发：市场清单在本文件，
// 面板数据本体在 astrbot-dashboard.mock，这里只补实例与账号视图。
import type {
    AppStoreMarketEntry,
    AstrBotAbconfInfo,
    AstrBotDashboardStatus,
    AstrBotKbCreate,
    AstrBotKnowledgeBase,
    AstrBotPersona,
    AstrBotSessionRule,
} from '../../types';
import { mockAstrBotDashboard } from '../astrbot-dashboard.mock';
import { require } from './state';
import { mockAccountView } from './webui-accounts';

export const mockAstrBotPlugins: AppStoreMarketEntry[] = [
    {
        resource: 'plugin',
        id: 'soulter/helloworld',
        name: 'helloworld',
        description: '示例插件',
        version: '1.2.0',
        author: 'soulter',
        homepage: 'https://github.com/Soulter/helloworld',
        time: '',
        package: 'https://github.com/Soulter/helloworld',
        module_name: '',
        flavor: 'git',
        is_official: false,
        valid: true,
        tags: [],
        supported_adapters: ['aiocqhttp'],
        authors: [],
        repos: [],
        files: [],
        allow_build: [],
    },
];

export const mockAstrBotApi = {
    astrbotDashboardStatus: (instanceId: string): Promise<AstrBotDashboardStatus> => {
        const inst = require(instanceId);
        return mockAstrBotDashboard.status(inst, mockAccountView(inst));
    },
    astrbotListPersonas: (instanceId: string): Promise<AstrBotPersona[]> => {
        const inst = require(instanceId);
        return mockAstrBotDashboard.listPersonas(inst, mockAccountView(inst));
    },
    astrbotUpsertPersona: (
        instanceId: string,
        persona: AstrBotPersona,
        creating: boolean,
    ): Promise<AstrBotPersona[]> => {
        const inst = require(instanceId);
        return mockAstrBotDashboard.upsertPersona(inst, mockAccountView(inst), persona, creating);
    },
    astrbotDeletePersona: (instanceId: string, personaId: string): Promise<AstrBotPersona[]> => {
        const inst = require(instanceId);
        return mockAstrBotDashboard.deletePersona(inst, mockAccountView(inst), personaId);
    },
    astrbotListKbs: (instanceId: string): Promise<AstrBotKnowledgeBase[]> => {
        const inst = require(instanceId);
        return mockAstrBotDashboard.listKbs(inst, mockAccountView(inst));
    },
    astrbotCreateKb: (
        instanceId: string,
        request: AstrBotKbCreate,
    ): Promise<AstrBotKnowledgeBase[]> => {
        const inst = require(instanceId);
        return mockAstrBotDashboard.createKb(inst, mockAccountView(inst), request);
    },
    astrbotDeleteKb: (instanceId: string, kbId: string): Promise<AstrBotKnowledgeBase[]> => {
        const inst = require(instanceId);
        return mockAstrBotDashboard.deleteKb(inst, mockAccountView(inst), kbId);
    },
    astrbotListSessionRules: (instanceId: string): Promise<AstrBotSessionRule[]> => {
        const inst = require(instanceId);
        return mockAstrBotDashboard.listRules(inst, mockAccountView(inst));
    },
    astrbotUpdateSessionRule: (
        instanceId: string,
        rule: AstrBotSessionRule,
    ): Promise<AstrBotSessionRule[]> => {
        const inst = require(instanceId);
        return mockAstrBotDashboard.updateRule(inst, mockAccountView(inst), rule);
    },
    astrbotDeleteSessionRule: (
        instanceId: string,
        umo: string,
        ruleKey: string,
    ): Promise<AstrBotSessionRule[]> => {
        const inst = require(instanceId);
        return mockAstrBotDashboard.deleteRule(inst, mockAccountView(inst), umo, ruleKey);
    },
    astrbotListAbconfs: (instanceId: string): Promise<AstrBotAbconfInfo[]> => {
        const inst = require(instanceId);
        return mockAstrBotDashboard.listAbconfs(inst, mockAccountView(inst));
    },
    astrbotCreateAbconf: (instanceId: string, name: string): Promise<AstrBotAbconfInfo[]> => {
        const inst = require(instanceId);
        return mockAstrBotDashboard.createAbconf(inst, mockAccountView(inst), name);
    },
    astrbotDeleteAbconf: (instanceId: string, abconfId: string): Promise<AstrBotAbconfInfo[]> => {
        const inst = require(instanceId);
        return mockAstrBotDashboard.deleteAbconf(inst, mockAccountView(inst), abconfId);
    },
    astrbotListSourceModels: (instanceId: string, sourceId: string): Promise<string[]> => {
        const inst = require(instanceId);
        return mockAstrBotDashboard.listSourceModels(inst, mockAccountView(inst), sourceId);
    },
    astrbotListSubagentTools: (instanceId: string): Promise<string[]> => {
        const inst = require(instanceId);
        return mockAstrBotDashboard.listSubagentTools(inst, mockAccountView(inst));
    },
};
