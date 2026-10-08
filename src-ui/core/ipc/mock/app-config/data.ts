// 各应用端框架的假配置文档清单：纯数据，状态与读写逻辑在 shared.ts / api.ts / write.ts。

import type { AppConfigDocument } from '../../types';

export const KARIN_DOCS: AppConfigDocument[] = [
    { id: 'env', label: '.env', rel_path: '.env', format: 'dot_env', hot_reload: true },
    {
        id: 'config',
        label: 'config.json',
        rel_path: '@karinjs/config/config.json',
        format: 'json',
        hot_reload: true,
    },
    {
        id: 'adapter',
        label: 'adapter.json',
        rel_path: '@karinjs/config/adapter.json',
        format: 'json',
        hot_reload: true,
    },
    {
        id: 'groups',
        label: 'groups.json',
        rel_path: '@karinjs/config/groups.json',
        format: 'json',
        hot_reload: true,
    },
    {
        id: 'privates',
        label: 'privates.json',
        rel_path: '@karinjs/config/privates.json',
        format: 'json',
        hot_reload: true,
    },
    {
        id: 'render',
        label: 'render.json',
        rel_path: '@karinjs/config/render.json',
        format: 'json',
        hot_reload: true,
    },
    {
        id: 'redis',
        label: 'redis.json',
        rel_path: '@karinjs/config/redis.json',
        format: 'json',
        hot_reload: false,
    },
];

export const NONEBOT2_DOCS: AppConfigDocument[] = [
    { id: 'env', label: '.env', rel_path: '.env', format: 'dot_env', hot_reload: false },
    {
        id: 'env_prod',
        label: '.env.prod',
        rel_path: '.env.prod',
        format: 'dot_env',
        hot_reload: false,
    },
    {
        id: 'pyproject',
        label: 'pyproject.toml',
        rel_path: 'pyproject.toml',
        format: 'toml',
        hot_reload: false,
    },
];

export const ASTRBOT_DOCS: AppConfigDocument[] = [
    {
        id: 'cmd_config',
        label: 'cmd_config.json',
        rel_path: 'data/cmd_config.json',
        format: 'json',
        hot_reload: false,
    },
];

/** NeoBot 两份 TOML（对齐后端 neobot_config_documents 的 id / rel_path / hot_reload） */
export const NEOBOT_DOCS: AppConfigDocument[] = [
    {
        id: 'adapter',
        label: 'OneBot 对接（app/data/config.toml）',
        rel_path: 'app/data/config.toml',
        format: 'toml',
        hot_reload: true,
    },
    {
        id: 'dashboard',
        label: '网页面板（app/data/plugins_data/dashboard/config.toml）',
        rel_path: 'app/data/plugins_data/dashboard/config.toml',
        format: 'toml',
        hot_reload: true,
    },
];

export const MAIBOT_DOCS: AppConfigDocument[] = [
    {
        id: 'bot_config',
        label: '主配置 bot_config.toml',
        rel_path: 'config/bot_config.toml',
        format: 'toml',
        hot_reload: true,
    },
    {
        id: 'model_config',
        label: '模型配置 model_config.toml',
        rel_path: 'config/model_config.toml',
        format: 'toml',
        hot_reload: true,
    },
    {
        id: 'adapter_config',
        label: 'NapCat 适配器 config.toml',
        rel_path: 'plugins/MaiBot-Napcat-Adapter/config.toml',
        format: 'toml',
        hot_reload: true,
    },
];

export const KOISHI_DOCS: AppConfigDocument[] = [
    {
        id: 'koishi',
        label: 'koishi.yml',
        rel_path: 'koishi.yml',
        format: 'yaml',
        hot_reload: false,
    },
    { id: 'env', label: '.env', rel_path: '.env', format: 'dot_env', hot_reload: false },
    {
        id: 'package',
        label: 'package.json',
        rel_path: 'package.json',
        format: 'json',
        hot_reload: false,
    },
];

/** 和后端 yunzai_config_documents 一致：config/config 下九份，server / redis / db 只在启动时读 */
export const YUNZAI_DOCS: AppConfigDocument[] = [
    'bot',
    'other',
    'group',
    'server',
    'redis',
    'renderer',
    'db',
    'milky',
    'satori',
].map((name) => ({
    id: name,
    label: `${name}.yaml`,
    rel_path: `config/config/${name}.yaml`,
    format: 'yaml',
    hot_reload: !['server', 'redis', 'db'].includes(name),
}));

export function docsOf(frameworkId: string): AppConfigDocument[] {
    switch (frameworkId) {
        case 'karin':
            return KARIN_DOCS;
        case 'astrbot':
            return ASTRBOT_DOCS;
        case 'maibot':
            return MAIBOT_DOCS;
        case 'koishi':
            return KOISHI_DOCS;
        case 'yunzai':
            return YUNZAI_DOCS;
        case 'neobot':
            return NEOBOT_DOCS;
        default:
            return NONEBOT2_DOCS;
    }
}
