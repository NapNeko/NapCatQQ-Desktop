// 浏览器预览：AstrBot Dashboard 运行期资源（人格 / 知识库 / 规则 / 多配置）。

import { makeAppConfigError } from '../../domain/apps/appConfigError';
import type {
    AppInstance,
    AppWebUiAccount,
    AstrBotAbconfInfo,
    AstrBotDashboardStatus,
    AstrBotKbCreate,
    AstrBotKnowledgeBase,
    AstrBotPersona,
    AstrBotSessionRule,
} from '../types';
import { withMockDelay } from './bootstrap.mock';

const personas = new Map<string, AstrBotPersona[]>();
const kbs = new Map<string, AstrBotKnowledgeBase[]>();
const rules = new Map<string, AstrBotSessionRule[]>();
const abconfs = new Map<string, AstrBotAbconfInfo[]>();

function seed(id: string) {
    if (!personas.has(id)) {
        personas.set(id, [
            {
                persona_id: 'default',
                system_prompt: 'You are a helpful assistant.',
                begin_dialogs: [],
                folder_id: '',
            },
        ]);
    }
    if (!kbs.has(id)) kbs.set(id, []);
    if (!rules.has(id)) {
        rules.set(id, [
            {
                umo: 'aiocqhttp:GroupMessage:123456789',
                rule_key: 'session_service_config',
                rule_json: JSON.stringify({
                    session_enabled: true,
                    llm_enabled: false,
                    tts_enabled: true,
                    custom_name: '测试群',
                }),
            },
            {
                umo: 'aiocqhttp:GroupMessage:123456789',
                rule_key: 'provider_perf_chat_completion',
                rule_json: '"openai/gpt-4o-mini"',
            },
        ]);
    }
    if (!abconfs.has(id)) abconfs.set(id, [{ id: 'default', name: '默认' }]);
}

export function mockAstrBotDashboardStatus(
    inst: AppInstance,
    account: AppWebUiAccount | null,
): AstrBotDashboardStatus {
    if (inst.state !== 'running') {
        return {
            running: false,
            has_password: false,
            reachable: false,
            authenticated: false,
            gate: 'not_running',
            message: '启动实例后才能改人格、知识库和会话规则',
        };
    }
    const hasPassword = !!account?.password;
    if (!hasPassword) {
        return {
            running: true,
            has_password: false,
            reachable: true,
            authenticated: false,
            gate: 'auth',
            message: '没有可用的 WebUI 密码。到连接页写下密码',
        };
    }
    return {
        running: true,
        has_password: true,
        reachable: true,
        authenticated: true,
        gate: 'ok',
        message: '',
    };
}

function requireLive(inst: AppInstance, account: AppWebUiAccount | null) {
    const status = mockAstrBotDashboardStatus(inst, account);
    if (status.gate === 'not_running') {
        throw makeAppConfigError('not_running', status.message);
    }
    if (status.gate === 'auth') {
        throw makeAppConfigError('auth', status.message);
    }
    if (status.gate === 'unreachable') {
        throw makeAppConfigError('unreachable', status.message);
    }
    seed(inst.id);
}

export const mockAstrBotDashboard = {
    status: (inst: AppInstance, account: AppWebUiAccount | null) =>
        withMockDelay(mockAstrBotDashboardStatus(inst, account)),

    listPersonas: (inst: AppInstance, account: AppWebUiAccount | null) => {
        requireLive(inst, account);
        return withMockDelay([...(personas.get(inst.id) ?? [])]);
    },

    upsertPersona: (
        inst: AppInstance,
        account: AppWebUiAccount | null,
        persona: AstrBotPersona,
        creating: boolean,
    ) => {
        requireLive(inst, account);
        const list = [...(personas.get(inst.id) ?? [])];
        const idx = list.findIndex((p) => p.persona_id === persona.persona_id);
        if (creating && idx >= 0) {
            throw makeAppConfigError('invalid', `人格 ${persona.persona_id} 已存在`);
        }
        if (!creating && idx < 0) {
            throw makeAppConfigError('other', `人格 ${persona.persona_id} 不存在`);
        }
        if (idx >= 0) list[idx] = persona;
        else list.push(persona);
        personas.set(inst.id, list);
        return withMockDelay(list);
    },

    deletePersona: (inst: AppInstance, account: AppWebUiAccount | null, personaId: string) => {
        requireLive(inst, account);
        const list = (personas.get(inst.id) ?? []).filter((p) => p.persona_id !== personaId);
        personas.set(inst.id, list);
        return withMockDelay(list);
    },

    listKbs: (inst: AppInstance, account: AppWebUiAccount | null) => {
        requireLive(inst, account);
        return withMockDelay([...(kbs.get(inst.id) ?? [])]);
    },

    createKb: (inst: AppInstance, account: AppWebUiAccount | null, req: AstrBotKbCreate) => {
        requireLive(inst, account);
        if (!req.embedding_provider_id.trim()) {
            throw makeAppConfigError('invalid', '创建知识库需要先有 embedding 提供商');
        }
        const list = [...(kbs.get(inst.id) ?? [])];
        list.push({
            kb_id: `kb-${Date.now().toString(36)}`,
            kb_name: req.kb_name,
            description: req.description,
            embedding_provider_id: req.embedding_provider_id,
        });
        kbs.set(inst.id, list);
        return withMockDelay(list);
    },

    deleteKb: (inst: AppInstance, account: AppWebUiAccount | null, kbId: string) => {
        requireLive(inst, account);
        const list = (kbs.get(inst.id) ?? []).filter((k) => k.kb_id !== kbId);
        kbs.set(inst.id, list);
        return withMockDelay(list);
    },

    listRules: (inst: AppInstance, account: AppWebUiAccount | null) => {
        requireLive(inst, account);
        return withMockDelay([...(rules.get(inst.id) ?? [])]);
    },

    updateRule: (inst: AppInstance, account: AppWebUiAccount | null, rule: AstrBotSessionRule) => {
        requireLive(inst, account);
        const list = [...(rules.get(inst.id) ?? [])];
        const idx = list.findIndex((r) => r.umo === rule.umo && r.rule_key === rule.rule_key);
        if (idx >= 0) list[idx] = rule;
        else list.push(rule);
        rules.set(inst.id, list);
        return withMockDelay(list);
    },

    deleteRule: (
        inst: AppInstance,
        account: AppWebUiAccount | null,
        umo: string,
        ruleKey: string,
    ) => {
        requireLive(inst, account);
        const list = (rules.get(inst.id) ?? []).filter(
            (r) => !(r.umo === umo && r.rule_key === ruleKey),
        );
        rules.set(inst.id, list);
        return withMockDelay(list);
    },

    listAbconfs: (inst: AppInstance, account: AppWebUiAccount | null) => {
        requireLive(inst, account);
        return withMockDelay([...(abconfs.get(inst.id) ?? [])]);
    },

    createAbconf: (inst: AppInstance, account: AppWebUiAccount | null, name: string) => {
        requireLive(inst, account);
        const list = [...(abconfs.get(inst.id) ?? [])];
        list.push({ id: `abconf_${Date.now().toString(36)}`, name });
        abconfs.set(inst.id, list);
        return withMockDelay(list);
    },

    deleteAbconf: (inst: AppInstance, account: AppWebUiAccount | null, id: string) => {
        requireLive(inst, account);
        if (id === 'default') throw makeAppConfigError('invalid', '不能删除默认配置');
        const list = (abconfs.get(inst.id) ?? []).filter((a) => a.id !== id);
        abconfs.set(inst.id, list);
        return withMockDelay(list);
    },

    listSourceModels: (inst: AppInstance, account: AppWebUiAccount | null, _sourceId: string) => {
        requireLive(inst, account);
        return withMockDelay(['gpt-4o-mini', 'deepseek-chat', 'qwen-plus']);
    },

    listSubagentTools: (inst: AppInstance, account: AppWebUiAccount | null) => {
        requireLive(inst, account);
        return withMockDelay(['web_search', 'knowledge_base']);
    },
};
