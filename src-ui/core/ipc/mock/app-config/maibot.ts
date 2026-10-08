// MaiBot 假配置：bot/model/adapter 三份 TOML 的类型化状态、只读字段语义、对接联动。

import type {
    AppConfigWriteResult,
    AppInstance,
    AppInstanceConfigEnvelope,
    MaiBotInstanceConfig,
} from '../../types';
import { maibotDefaultConfig, validateMaiBotConfig } from '../../../domain/apps/maibotConfig';
import { makeAppConfigError } from '../../../domain/apps/appConfigError';
import { withMockDelay } from '../bootstrap.mock';
import { MAIBOT_DOCS } from './data';
import { combined, rev, type MockAppConfigDeps, type TypedState } from './shared';

const mbStates = new Map<string, TypedState<MaiBotInstanceConfig>>();

function mbState(instance: AppInstance): TypedState<MaiBotInstanceConfig> {
    let s = mbStates.get(instance.id);
    if (!s) {
        const docRev: Record<string, number> = {};
        for (const d of MAIBOT_DOCS) docRev[d.id] = 1;
        s = { config: maibotDefaultConfig(instance.port), docRev };
        s.config.webui_token = 'Ncd_mockMockMockMockMock';
        if (instance.link && s.config.adapter) {
            s.config.adapter = {
                ...s.config.adapter,
                enabled: true,
                napcat_port: 23456,
                has_token: true,
            };
        }
        mbStates.set(instance.id, s);
    }
    return s;
}

function mbEnvelope(s: TypedState<MaiBotInstanceConfig>): AppInstanceConfigEnvelope {
    return {
        config: { framework: 'maibot', data: structuredClone(s.config) },
        revision: combined(s.docRev, MAIBOT_DOCS),
        documents: MAIBOT_DOCS.map((d) => ({
            doc_id: d.id,
            revision: rev(s.docRev[d.id] ?? 0),
            hot_reload: d.hot_reload,
        })),
    };
}

export function mbReadConfig(inst: AppInstance): AppInstanceConfigEnvelope {
    return mbEnvelope(mbState(inst));
}

/** 运行期 mock 读当前落盘的配置（MCP 服务列表、提供商） */
export function peekMaiBotConfig(instance: AppInstance): MaiBotInstanceConfig {
    return structuredClone(mbState(instance).config);
}

/** 对接后假装适配器配置被写了（真机由后端 apply_link 写 config.toml） */
export function syncMaiBotLink(instance: AppInstance, linked: boolean): void {
    const s = mbState(instance);
    if (!s.config.adapter) return;
    s.config.adapter = {
        ...s.config.adapter,
        enabled: linked,
        napcat_port: linked ? 23456 : s.config.adapter.napcat_port,
        has_token: linked || s.config.adapter.has_token,
    };
    s.docRev.adapter_config = (s.docRev.adapter_config ?? 0) + 1;
}

export async function mbWriteConfig(
    inst: AppInstance,
    data: MaiBotInstanceConfig,
    baseRevision: string | null,
    deps: MockAppConfigDeps,
): Promise<AppConfigWriteResult> {
    const s = mbState(inst);
    if (baseRevision != null && baseRevision !== combined(s.docRev, MAIBOT_DOCS)) {
        throw makeAppConfigError('conflict', '配置已被修改（bot_config），请重新加载后再保存');
    }
    const next = structuredClone(data);
    const issues = validateMaiBotConfig(next);
    if (issues.length) {
        throw makeAppConfigError(
            'invalid',
            `配置校验未通过：${issues.map((i) => `${i.path}: ${i.message}`).join('; ')}`,
            issues,
        );
    }
    const before = s.config;
    const changed = (a: unknown, b: unknown) => JSON.stringify(a) !== JSON.stringify(b);
    if (changed(before.bot, next.bot)) s.docRev.bot_config = (s.docRev.bot_config ?? 0) + 1;
    if (changed(before.models, next.models)) {
        s.docRev.model_config = (s.docRev.model_config ?? 0) + 1;
    }
    if (changed(before.adapter?.chat, next.adapter?.chat)) {
        s.docRev.adapter_config = (s.docRev.adapter_config ?? 0) + 1;
    }
    // 和后端一样：只在启动时读的字段（端口、日志、插件运行时…）改了才提示重启
    const restartFields =
        before.bot.webui.port !== next.bot.webui.port ||
        changed(before.bot.maim_message, next.bot.maim_message) ||
        changed(before.bot.log, next.bot.log) ||
        before.bot.plugin_runtime.enabled !== next.bot.plugin_runtime.enabled;
    // 只读字段以落盘为准
    s.config = {
        ...next,
        webui_token: before.webui_token,
        adapter:
            next.adapter && before.adapter
                ? { ...before.adapter, chat: next.adapter.chat }
                : before.adapter,
    };
    let portChanged = false;
    if (next.bot.webui.port !== inst.port) {
        deps.publish({ ...inst, port: next.bot.webui.port }, 'port_changed');
        portChanged = true;
    }
    const env = mbEnvelope(s);
    return withMockDelay({
        config: env.config,
        revision: env.revision,
        documents: env.documents,
        restart_required: restartFields && inst.state === 'running',
        relinked: false,
        port_changed: portChanged,
    });
}

export function reset() {
    mbStates.clear();
}
