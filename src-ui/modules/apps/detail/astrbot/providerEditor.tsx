// 提供商的添加入口和编辑状态，模型页和概览页共用：预设菜单挑一家，打开填 Key 的对话框，确定才写回表单。

import { useState, type ReactNode } from 'react';
import { Popover, PopoverClose, PopoverContent, PopoverTrigger } from '../../../../shared/ui';
import {
    ASTRBOT_SOURCE_PRESETS,
    type AstrBotSourcePreset,
} from '../../../../core/domain/apps/astrbotConfig';
import {
    commitProviderDraft,
    draftForCreate,
    draftForEdit,
    type ProviderDraft,
} from './providerDraft';
import type { AstrBotInstanceConfig } from '../../../../core/ipc/types';

export const PROVIDER_TYPE_LABEL: Record<string, string> = {
    chat_completion: '对话',
    speech_to_text: '语音转文字',
    text_to_speech: '文字转语音',
    embedding: '向量嵌入',
    rerank: '重排',
    agent_runner: '智能体',
};

export function hostOf(apiBase: string): string {
    try {
        return new URL(apiBase).host;
    } catch {
        return apiBase;
    }
}

export function useProviderEditor(
    config: AstrBotInstanceConfig,
    onChange: (next: AstrBotInstanceConfig) => void,
) {
    const [draft, setDraft] = useState<ProviderDraft | null>(null);
    return {
        draft,
        setDraft,
        openCreate: (preset: AstrBotSourcePreset) => setDraft(draftForCreate(config, preset)),
        openEdit: (index: number) => setDraft(draftForEdit(config, index)),
        cancel: () => setDraft(null),
        confirm: () => {
            if (draft) onChange(commitProviderDraft(config, draft));
            setDraft(null);
        },
    };
}

/** children 是触发按钮，由调用方决定样式（模型页用次要按钮，概览清单里用主按钮）。 */
export const ProviderPresetMenu: React.FC<{
    children: ReactNode;
    onPick: (preset: AstrBotSourcePreset) => void;
    align?: 'start' | 'center' | 'end';
}> = ({ children, onPick, align = 'end' }) => (
    <Popover>
        <PopoverTrigger asChild>{children}</PopoverTrigger>
        <PopoverContent align={align} sideOffset={6} className="w-72 p-1">
            {/* 十几条预设不限高会顶出窗口；变量是触发器到窗口边的剩余空间，滚轮只在菜单里 */}
            <div
                className="overflow-y-auto overscroll-contain"
                style={{
                    maxHeight:
                        'min(24rem, calc(var(--radix-popover-content-available-height, 24rem) - 8px))',
                }}
            >
                <PresetGroup
                    title="对话"
                    items={ASTRBOT_SOURCE_PRESETS.filter(
                        (p) => p.provider_type === 'chat_completion',
                    )}
                    onPick={onPick}
                />
                <div className="my-1 h-px bg-border-subtle" />
                <PresetGroup
                    title="向量嵌入（知识库用）"
                    items={ASTRBOT_SOURCE_PRESETS.filter((p) => p.provider_type === 'embedding')}
                    onPick={onPick}
                />
            </div>
        </PopoverContent>
    </Popover>
);

const PresetGroup: React.FC<{
    title: string;
    items: readonly AstrBotSourcePreset[];
    onPick: (p: AstrBotSourcePreset) => void;
}> = ({ title, items, onPick }) => (
    <div>
        <p className="px-2 pb-1 pt-1.5 text-2xs font-medium uppercase tracking-wider text-text-tertiary">
            {title}
        </p>
        {items.map((p) => (
            <PopoverClose key={p.id} asChild>
                <button
                    type="button"
                    className="flex w-full items-center justify-between gap-3 rounded-sm px-2 py-1.5 text-left text-[13px] text-text hover:bg-inset"
                    onClick={() => onPick(p)}
                >
                    <span className="shrink-0 whitespace-nowrap">{p.label}</span>
                    {p.api_base && (
                        <span className="min-w-0 truncate font-mono text-2xs text-text-tertiary">
                            {hostOf(p.api_base)}
                        </span>
                    )}
                </button>
            </PopoverClose>
        ))}
    </div>
);
