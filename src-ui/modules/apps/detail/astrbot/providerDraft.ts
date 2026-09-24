// 服务商对话框的局部草稿：打开时从表单拷一份，确定时整体写回。模型页和概览页共用这一套。

import { newSourceFromPreset, type AstrBotSourcePreset } from '../../../../core/domain/apps/astrbotConfig';
import type {
    AstrBotInstanceConfig,
    AstrBotProviderModel,
    AstrBotProviderSource,
} from '../../../../core/ipc/types';

/**
 * index 为 null 表示新增。defaultId 是全局默认模型的副本：加第一条对话模型时顺手填上，
 * 删掉默认模型时清空，确定时一并写回。
 */
export type ProviderDraft = {
    index: number | null;
    original: AstrBotProviderSource | null;
    src: AstrBotProviderSource;
    kids: AstrBotProviderModel[];
    defaultId: string;
};

export function draftForCreate(cfg: AstrBotInstanceConfig, preset: AstrBotSourcePreset): ProviderDraft {
    return {
        index: null,
        original: null,
        src: newSourceFromPreset(preset, cfg.sources),
        kids: [],
        defaultId: cfg.ai.default_provider_id,
    };
}

export function draftForEdit(cfg: AstrBotInstanceConfig, index: number): ProviderDraft {
    const src = cfg.sources[index];
    return {
        index,
        original: src,
        src,
        kids: cfg.models.filter((m) => m.provider_source_id === src.id),
        defaultId: cfg.ai.default_provider_id,
    };
}

export function commitProviderDraft(cfg: AstrBotInstanceConfig, d: ProviderDraft): AstrBotInstanceConfig {
    const id = d.src.id.trim();
    const src = { ...d.src, id };
    const kids = d.kids.map((m) => ({ ...m, provider_source_id: id }));
    const sources = d.index === null ? [...cfg.sources, src] : cfg.sources.map((s, i) => (i === d.index ? src : s));
    // 原位替换这一源的模型，别把它们挪到列表尾巴上改变配置文件顺序
    const models: AstrBotProviderModel[] = [];
    let placed = false;
    for (const m of cfg.models) {
        if (d.original && m.provider_source_id === d.original.id) {
            if (!placed) {
                models.push(...kids);
                placed = true;
            }
            continue;
        }
        models.push(m);
    }
    if (!placed) models.push(...kids);
    const default_provider_id = models.some((m) => m.id === d.defaultId) ? d.defaultId : '';
    return { ...cfg, sources, models, ai: { ...cfg.ai, default_provider_id } };
}

/** 连同名下模型一起移出草稿；默认模型跟着没了就清掉，别留一个指向空处的 id。 */
export function removeProviderSource(cfg: AstrBotInstanceConfig, index: number): AstrBotInstanceConfig {
    const id = cfg.sources[index]?.id;
    const models = cfg.models.filter((m) => m.provider_source_id !== id);
    const defaultGone = !models.some((m) => m.id === cfg.ai.default_provider_id);
    return {
        ...cfg,
        sources: cfg.sources.filter((_, i) => i !== index),
        models,
        ai: defaultGone ? { ...cfg.ai, default_provider_id: '' } : cfg.ai,
    };
}
