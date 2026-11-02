// 浏览器预览的配置文件原文。
import type { AppConfigDocument, AppConfigFormat, AppConfigText } from '../../types';
import { makeAppConfigError } from '../../../domain/apps/appConfigError';
import { require as requireInstance } from './state';

function doc(
    id: string,
    rel_path: string,
    format: AppConfigFormat,
    hot_reload = false,
): AppConfigDocument {
    return { id, rel_path, label: rel_path, format, hot_reload };
}

const documents: Record<string, AppConfigDocument[]> = {
    karin: [
        doc('env', '.env', 'dot_env', true),
        ...['config', 'adapter', 'groups', 'privates', 'render', 'redis'].map((name) =>
            doc(name, `@karinjs/config/${name}.json`, 'json', name !== 'redis'),
        ),
    ],
    nonebot2: [
        doc('env', '.env', 'dot_env'),
        doc('env_prod', '.env.prod', 'dot_env'),
        doc('pyproject', 'pyproject.toml', 'toml'),
    ],
    astrbot: [doc('cmd_config', 'data/cmd_config.json', 'json')],
    maibot: [
        doc('bot_config', 'config/bot_config.toml', 'toml', true),
        doc('model_config', 'config/model_config.toml', 'toml', true),
        doc('adapter_config', 'plugins/MaiBot-Napcat-Adapter/config.toml', 'toml', true),
    ],
    koishi: [
        doc('koishi', 'koishi.yml', 'yaml'),
        doc('env', '.env', 'dot_env'),
        doc('package', 'package.json', 'json'),
    ],
    neobot: [
        doc('adapter', 'app/data/config.toml', 'toml'),
        doc('dashboard', 'app/data/plugins_data/dashboard/config.toml', 'toml'),
    ],
    yunzai: ['bot', 'other', 'group', 'server', 'redis', 'renderer', 'db', 'milky', 'satori'].map(
        (name) =>
            doc(
                name,
                `config/config/${name}.yaml`,
                'yaml',
                !['server', 'redis', 'db', 'milky', 'satori'].includes(name),
            ),
    ),
};
const files = new Map<string, AppConfigText>();
let revision = 0;

function read(instanceId: string, docId: string): AppConfigText {
    const instance = requireInstance(instanceId);
    const document = documents[instance.framework_id]?.find((item) => item.id === docId);
    if (!document) throw makeAppConfigError('invalid', '未知的配置文件');
    const key = `${instanceId}:${docId}`;
    let file = files.get(key);
    if (!file) {
        let text = document.format === 'json' ? '{}\n' : '';
        if (instance.framework_id === 'nonebot2') {
            if (docId === 'env') text = 'ENVIRONMENT=prod\n';
            if (docId === 'env_prod')
                text = `HOST=127.0.0.1\nPORT=${instance.port}\nLOG_LEVEL=INFO\n\nCOMMAND_START=["/"]\nSUPERUSERS=[]\nONEBOT_ACCESS_TOKEN=""\n`;
            if (docId === 'pyproject')
                text =
                    '[project]\nname = "bot-xiuxian"\nversion = "0.1.0"\n\n[tool.nonebot]\nplugins = []\n';
        } else if (instance.framework_id === 'karin' && docId === 'env') {
            text = `HTTP_PORT=${instance.port}\nLOG_LEVEL=info\nWS_SERVER_AUTH_KEY=preview-key\n`;
        } else if (instance.framework_id === 'koishi' && docId === 'koishi') {
            text = `host: 127.0.0.1\nport: ${instance.port}\nplugins:\n  group:basic:\n    help: {}\n`;
        } else if (instance.framework_id === 'astrbot') {
            text =
                JSON.stringify(
                    {
                        dashboard: { enable: true, host: '127.0.0.1', port: instance.port },
                        platform: [],
                    },
                    null,
                    2,
                ) + '\n';
        }
        file = { doc_id: docId, text, revision: `mock-${++revision}` };
        files.set(key, file);
    }
    return { ...file };
}

export const configFilesApi = {
    listConfigDocuments: async (instanceId: string): Promise<AppConfigDocument[]> =>
        (documents[requireInstance(instanceId).framework_id] ?? []).map((item) => ({ ...item })),
    readConfigText: async (instanceId: string, docId: string) => read(instanceId, docId),
    writeConfigText: async (
        instanceId: string,
        docId: string,
        text: string,
        baseRevision: string | null,
    ): Promise<AppConfigText> => {
        const instance = requireInstance(instanceId);
        if (
            instance.framework_id === 'koishi' &&
            docId === 'koishi' &&
            instance.state === 'running'
        ) {
            throw makeAppConfigError(
                'invalid',
                'Koishi 运行时会覆盖 koishi.yml，请停止实例后保存，或在 WebUI 中修改。',
            );
        }
        const current = read(instanceId, docId);
        if (baseRevision !== null && baseRevision !== current.revision)
            throw makeAppConfigError('conflict', '配置文件已被修改');
        if (
            documents[instance.framework_id]?.find((item) => item.id === docId)?.format === 'json'
        ) {
            try {
                JSON.parse(text);
            } catch {
                throw makeAppConfigError('invalid', 'JSON 语法错误');
            }
        }
        const result = { doc_id: docId, text, revision: `mock-${++revision}` };
        files.set(`${instanceId}:${docId}`, result);
        return { ...result };
    },
};
