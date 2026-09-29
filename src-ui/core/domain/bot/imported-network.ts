// 把远端 onebot 反向解析结果合进 BotConfig：导入时直接合，已有 Bot 回读时先出增删改预览。
// 纯函数，方便单测。

import type { BotConfig } from '../../ipc/generated/domain/BotConfig';
import type { ImportedNetworkConfig } from '../../ipc/generated/domain/ImportedNetworkConfig';
import {
    CONNECTION_GROUP_KEY,
    CONNECTION_KINDS,
    type ConnectionConfig,
    type ConnectionKind,
} from './connections';

export function applyImportedNetwork(
    cfg: BotConfig,
    imported: ImportedNetworkConfig,
): void {
    cfg.connect = imported.connect;
    if (imported.musicSignUrl) {
        cfg.bot = { ...cfg.bot, musicSignUrl: imported.musicSignUrl };
    }
    if (imported.statusCommand) {
        cfg.statusCommand = imported.statusCommand;
    }
    if (
        imported.enableLocalFile2Url != null ||
        imported.parseMultMsg != null
    ) {
        cfg.advanced = {
            ...cfg.advanced,
            ...(imported.enableLocalFile2Url != null
                ? { enableLocalFile2Url: imported.enableLocalFile2Url }
                : {}),
            ...(imported.parseMultMsg != null
                ? { parseMultMsg: imported.parseMultMsg }
                : {}),
        };
    }
}

export interface ConnectionRef {
    kind: ConnectionKind;
    name: string;
}

export interface ImportedNetworkPreview {
    /** 合进远端内容后的整份配置；用户确认后直接替换表单 */
    next: BotConfig;
    added: ConnectionRef[];
    removed: ConnectionRef[];
    changed: ConnectionRef[];
    /** 连接之外还有字段不同（音乐签名、状态命令、NC 高级开关） */
    otherChanged: boolean;
}

/** 算出用远端内容替换后连接的增删改；按 类型 + 名称 对齐，名称在同类里唯一 */
export function previewImportedNetwork(
    current: BotConfig,
    imported: ImportedNetworkConfig,
): ImportedNetworkPreview {
    const next = structuredClone(current);
    applyImportedNetwork(next, imported);

    const added: ConnectionRef[] = [];
    const removed: ConnectionRef[] = [];
    const changed: ConnectionRef[] = [];
    for (const meta of CONNECTION_KINDS) {
        const key = CONNECTION_GROUP_KEY[meta.kind];
        const before = current.connect[key] as ConnectionConfig[];
        const after = next.connect[key] as ConnectionConfig[];
        for (const item of after) {
            const old = before.find((c) => c.name === item.name);
            if (!old) added.push({ kind: meta.kind, name: item.name });
            else if (stableJson(old) !== stableJson(item))
                changed.push({ kind: meta.kind, name: item.name });
        }
        for (const item of before) {
            if (!after.some((c) => c.name === item.name))
                removed.push({ kind: meta.kind, name: item.name });
        }
    }
    const otherChanged =
        stableJson({ ...current, connect: null }) !== stableJson({ ...next, connect: null });
    return { next, added, removed, changed, otherChanged };
}

export function isPreviewEmpty(p: ImportedNetworkPreview): boolean {
    return (
        p.added.length === 0 &&
        p.removed.length === 0 &&
        p.changed.length === 0 &&
        !p.otherChanged
    );
}

// 两边都来自 serde，键序通常一致；排一下键免得偶发的顺序差被当成改动
function stableJson(value: unknown): string {
    return JSON.stringify(value, (_k, v: unknown) => {
        if (v && typeof v === 'object' && !Array.isArray(v)) {
            const obj = v as Record<string, unknown>;
            return Object.fromEntries(
                Object.keys(obj)
                    .sort()
                    .map((k) => [k, obj[k]]),
            );
        }
        return v;
    });
}
