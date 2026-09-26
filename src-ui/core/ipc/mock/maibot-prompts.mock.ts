// 浏览器预览：麦麦提示词模板。语义照后端 resources/prompts：覆盖、版本、在用版本、恢复默认留版本。

import { makeAppConfigError } from '../../domain/apps/appConfigError';
import { checkPrompt, PROMPT_LEGACY_VERSION } from '../../domain/apps/maibotPrompts';
import type {
    AppInstance,
    MaiBotPromptAction,
    MaiBotPromptCatalog,
    MaiBotPromptFile,
    MaiBotPromptVersion,
} from '../types';
import { withMockDelay } from './bootstrap.mock';

type Seed = { name: string; display: string; description: string; advanced: boolean; content: string };

const ZH: Seed[] = [
    {
        name: 'maisaka_replyer.prompt',
        display: '回复',
        description: '根据人格、表达风格、群聊注意事项和待回复的上下文生成最终回复。',
        advanced: false,
        content:
            '{identity}\n\n你正在群里聊天。说话风格：{reply_style}\n\n{group_chat_attention_block}\n\n{replyer_output_instruction}\n只输出要说的话，不要加引号，也不要解释你为什么这么说。',
    },
    {
        name: 'maisaka_chat.prompt',
        display: '规划器',
        description: '决定这一轮要不要说话、做什么动作、要不要查记忆。',
        advanced: false,
        content:
            '你是 {bot_name}。行为准则：{behavior_style}\n\n{group_chat_attention_block}\n\n先想清楚现在有没有必要开口，再决定动作。\n{query_memory_rule}\n\n用 JSON 回答，格式形如 {{"action": "reply", "reason": "…"}}。',
    },
    {
        name: 'image_description.prompt',
        display: '图片描述',
        description: '把聊天里的图片描述成一段文字，给后面的回复参考。',
        advanced: false,
        content: '请用一两句话描述这张图片的主要内容。如果图片里有文字，把文字原样写出来。',
    },
    {
        name: 'emoji_content_analysis.prompt',
        display: '表情包内容分析',
        description: '分析表情包图片表达的情绪，生成标签。',
        advanced: true,
        content: '这是一个{image_type}表情包。用 3 到 6 个逗号分隔的词描述它表达的情绪和场景，不要写句子。',
    },
    {
        name: 'learn_style.prompt',
        display: '学习表达方式',
        description: '从聊天记录里总结「什么情况下可以怎么说」。',
        advanced: true,
        content:
            '下面是一段聊天记录：\n{chat_str}\n\n总结其中值得学习的表达方式，每条写成「当 AAAA 时，可以说 BBBB」，AAAA 和 BBBB 都不超过 20 个字。',
    },
    {
        name: 'learn_jargon.prompt',
        display: '学习黑话',
        description: '找出聊天里反复出现、外人看不懂的词。',
        advanced: true,
        content: '聊天记录：\n{chat_str}\n\n列出里面像是圈内黑话、梗或缩写的词，每行一个，不确定的不要写。',
    },
    {
        name: 'expression_evaluation.prompt',
        display: '表达方式评估',
        description: '判断学到的表达方式值不值得留下。',
        advanced: true,
        content: '情境：{situation}\n表达：{style}\n\n按下面的标准打分并说明理由：\n{criteria_list}',
    },
    {
        name: 'mid_term_memory_summary.prompt',
        display: '聊天回想总结',
        description: '把一段时间的聊天压成回想，供之后召回。',
        advanced: true,
        content: '参与者：{participants_text}\n时间：{time_range}\n\n用几句话总结这段时间聊了什么、谁说了什么重要的事。',
    },
];

const EN: Seed[] = ZH.filter((s) => !s.advanced).map((s) => ({
    ...s,
    content:
        s.name === 'image_description.prompt'
            ? 'Describe the main content of this image in one or two sentences. Quote any text in it verbatim.'
            : s.content.replace(/[一-龥，。：、「」（）！？]+/g, ' ').replace(/ +/g, ' '),
}));

type Entry = {
    seed: Seed;
    custom?: string;
    active?: string;
    versions: { id: string; label: string; created_at: number; modified_at: number }[];
};

const stores = new Map<string, Map<string, Entry>>();
// 版本内容另存一份：键是 实例/语言/文件名/版本号
const versionBodies = new Map<string, string>();
const bodyKey = (instanceId: string, language: string, name: string, id: string) =>
    `${instanceId}/${language}/${name}/${id}`;

function store(instanceId: string): Map<string, Entry> {
    let s = stores.get(instanceId);
    if (!s) {
        s = new Map();
        for (const seed of ZH) s.set(`zh-CN/${seed.name}`, { seed, versions: [] });
        for (const seed of EN) s.set(`en-US/${seed.name}`, { seed, versions: [] });
        // 预置一份改过的回复模板，走查时能看到「已改」和版本
        const replyer = s.get('zh-CN/maisaka_replyer.prompt')!;
        const now = Date.now() / 1000;
        replyer.custom = replyer.seed.content.replace('只输出要说的话', '只输出要说的话，句子短一点');
        replyer.versions = [{ id: 'v20260920101500', label: '短句一点', created_at: now - 86400 * 6, modified_at: now - 86400 * 6 }];
        replyer.active = 'v20260920101500';
        versionBodies.set(bodyKey(instanceId, 'zh-CN', replyer.seed.name, 'v20260920101500'), replyer.custom);
        stores.set(instanceId, s);
    }
    return s;
}

function entry(inst: AppInstance, language: string, name: string): Entry {
    const e = store(inst.id).get(`${language}/${name}`);
    if (!e) throw makeAppConfigError('invalid', `没有这个提示词：${name}`);
    return e;
}

function versionsOf(e: Entry): MaiBotPromptVersion[] {
    const out = e.versions.map((v) => ({ ...v, active: v.id === e.active }));
    if (e.custom !== undefined && !e.active && out.length === 0) {
        out.push({ id: PROMPT_LEGACY_VERSION, label: '当前自定义（旧格式）', created_at: 0, modified_at: 0, active: true });
    }
    return out.sort((a, b) => b.modified_at - a.modified_at);
}

function toFile(language: string, e: Entry): MaiBotPromptFile {
    return {
        language,
        name: e.seed.name,
        content: e.custom ?? e.seed.content,
        default_content: e.seed.content,
        customized: e.custom !== undefined,
        active_version_id: e.active ?? (e.custom !== undefined ? PROMPT_LEGACY_VERSION : undefined),
        versions: versionsOf(e),
    };
}

function stamp(): string {
    const d = new Date();
    const p = (n: number) => String(n).padStart(2, '0');
    return `v${d.getFullYear()}${p(d.getMonth() + 1)}${p(d.getDate())}${p(d.getHours())}${p(d.getMinutes())}${p(d.getSeconds())}`;
}

export const mockMaiBotPrompts = {
    catalog(inst: AppInstance): Promise<MaiBotPromptCatalog> {
        const byLang = new Map<string, MaiBotPromptCatalog['languages'][number]>();
        for (const [key, e] of store(inst.id)) {
            const language = key.split('/')[0]!;
            const lang = byLang.get(language) ?? { language, prompts: [] };
            lang.prompts.push({
                name: e.seed.name,
                display_name: e.seed.display,
                description: e.seed.description,
                advanced: e.seed.advanced,
                customized: e.custom !== undefined,
                version_count: versionsOf(e).length,
            });
            byLang.set(language, lang);
        }
        const languages = [...byLang.values()]
            .map((l) => ({ ...l, prompts: l.prompts.sort((a, b) => a.name.localeCompare(b.name)) }))
            .sort((a, b) => a.language.localeCompare(b.language));
        return withMockDelay({ languages, active_language: 'zh-CN', live: inst.state === 'running' });
    },

    file(inst: AppInstance, language: string, name: string): Promise<MaiBotPromptFile> {
        return withMockDelay(toFile(language, entry(inst, language, name)));
    },

    version(inst: AppInstance, language: string, name: string, versionId: string): Promise<string> {
        const e = entry(inst, language, name);
        const content =
            versionId === PROMPT_LEGACY_VERSION ? e.custom : versionBodies.get(bodyKey(inst.id, language, name, versionId));
        if (content === undefined) return Promise.reject(makeAppConfigError('invalid', '这个版本已经不在了'));
        return withMockDelay(content);
    },

    action(inst: AppInstance, action: MaiBotPromptAction): Promise<MaiBotPromptFile> {
        const { language, name } = action;
        const e = entry(inst, language, name);
        const now = Date.now() / 1000;
        switch (action.op) {
            case 'save': {
                const check = checkPrompt(action.content, e.seed.content);
                if (check.error) return Promise.reject(makeAppConfigError('invalid', check.error));
                let id = action.version_id && action.version_id !== PROMPT_LEGACY_VERSION ? action.version_id : null;
                const existing = id ? e.versions.find((v) => v.id === id) : undefined;
                if (id && !existing) return Promise.reject(makeAppConfigError('invalid', '这个版本已经不在了'));
                if (existing) {
                    if (action.label.trim()) existing.label = action.label.trim();
                    existing.modified_at = now;
                } else {
                    id = stamp();
                    const base = id;
                    for (let n = 2; e.versions.some((v) => v.id === id); n += 1) id = `${base}-${n}`;
                    const stem = name.replace(/\.prompt$/, '');
                    const label = action.label.trim() || `${stem} 自定义版本 ${new Date().toLocaleString('zh-CN', { hour12: false })}`;
                    e.versions.push({ id, label, created_at: now, modified_at: now });
                }
                versionBodies.set(bodyKey(inst.id, language, name, id!), action.content);
                e.active = id!;
                e.custom = action.content;
                break;
            }
            case 'activate': {
                const content =
                    action.version_id === PROMPT_LEGACY_VERSION
                        ? e.custom
                        : versionBodies.get(bodyKey(inst.id, language, name, action.version_id));
                if (content === undefined) return Promise.reject(makeAppConfigError('invalid', '这个版本已经不在了'));
                e.custom = content;
                if (action.version_id !== PROMPT_LEGACY_VERSION) e.active = action.version_id;
                break;
            }
            case 'delete_version': {
                if (action.version_id === PROMPT_LEGACY_VERSION) {
                    e.custom = undefined;
                    break;
                }
                e.versions = e.versions.filter((v) => v.id !== action.version_id);
                if (e.active === action.version_id) {
                    e.active = undefined;
                    e.custom = undefined;
                }
                break;
            }
            case 'restore':
                e.custom = undefined;
                e.active = undefined;
                break;
        }
        return withMockDelay(toFile(language, e));
    },
};
