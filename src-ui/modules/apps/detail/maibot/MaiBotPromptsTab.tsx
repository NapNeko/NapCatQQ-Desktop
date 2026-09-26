// 提示词页：左边模板列表（语言、搜索、常用 / 高级），右边编辑。跑着走 WebUI，停着直接改盘上文件；
// 刚启动、WebUI 还没起来时整页等它，免得一边读盘一边读接口两头对不上。

import { useState } from 'react';
import { ChevronDown, Search } from 'lucide-react';
import { Select } from '../../../../shared/ui';
import { cn } from '../../../../shared/utils/cn';
import { promptDisplayName, promptLanguageLabel, sortPrompts } from '../../../../core/domain/apps/maibotPrompts';
import type { AppInstance, MaiBotPromptInfo, MaiBotRuntimeStatus } from '../../../../core/ipc/types';
import { useMaiBotPromptCatalog, type PromptMode } from '../../../../hooks/apps/useMaiBotPrompts';
import { PaneLoadError, PaneLoading } from '../PaneStatus';
import { MaiBotLiveGate, maibotLive } from './MaiBotLiveGate';
import { PromptEditor } from './maibotPromptEditor';
import { promptDraftKey, type PromptDrafts } from './maibotPromptDrafts';

const PromptRow: React.FC<{
    info: MaiBotPromptInfo;
    selected: boolean;
    dirty: boolean;
    onSelect: () => void;
}> = ({ info, selected, dirty, onSelect }) => (
    <button
        type="button"
        aria-current={selected || undefined}
        onClick={onSelect}
        className={cn(
            'flex w-full items-center gap-2 rounded-sm px-2.5 py-1.5 text-left transition-colors',
            'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand/40',
            selected ? 'bg-brand-soft/80' : 'hover:bg-inset',
        )}
    >
        <span className="min-w-0 flex-1">
            <span className={cn('block truncate text-[13px] text-text', selected && 'font-medium')}>
                {promptDisplayName(info)}
            </span>
            <span className="block truncate font-mono text-2xs text-text-tertiary">{info.name.replace(/\.prompt$/, '')}</span>
        </span>
        {dirty ? (
            <span title="有没保存的改动" className="h-1.5 w-1.5 shrink-0 rounded-full bg-warning" />
        ) : (
            info.customized && <span className="shrink-0 text-2xs text-brand">已改</span>
        )}
    </button>
);

export const MaiBotPromptsTab: React.FC<{
    instance: AppInstance;
    status: MaiBotRuntimeStatus | undefined;
    drafts: PromptDrafts;
    onStart: () => void;
    starting: boolean;
}> = ({ instance, status, drafts, onStart, starting }) => {
    const running = instance.state === 'running';
    const mode: PromptMode = running ? (maibotLive(status) ? 'live' : 'waiting') : 'disk';
    const catalog = useMaiBotPromptCatalog(instance.id, mode);
    const [language, setLanguage] = useState<string | null>(null);
    const [selected, setSelected] = useState<string | null>(null);
    const [query, setQuery] = useState('');
    const [advancedOpen, setAdvancedOpen] = useState(false);

    if (mode === 'waiting') return <MaiBotLiveGate status={status} what="提示词" onStart={onStart} starting={starting} />;
    if (catalog.isLoading) return <PaneLoading text="正在读取提示词…" />;
    if (!catalog.data) return <PaneLoadError message="读取提示词失败" onRetry={() => void catalog.refetch()} />;

    const data = catalog.data;
    const languages = data.languages.map((l) => l.language);
    const lang = language && languages.includes(language)
        ? language
        : languages.includes(data.active_language)
          ? data.active_language
          : languages[0];
    const prompts = sortPrompts(data.languages.find((l) => l.language === lang)?.prompts ?? []);
    const q = query.trim().toLowerCase();
    const shown = q
        ? prompts.filter((p) => [p.display_name, p.name, p.description].some((s) => s.toLowerCase().includes(q)))
        : prompts;
    const basic = shown.filter((p) => !p.advanced);
    const advanced = shown.filter((p) => p.advanced);
    const current =
        prompts.find((p) => p.name === selected) ?? prompts.find((p) => !p.advanced) ?? prompts[0];
    const showAdvanced = advancedOpen || !!q || !!current?.advanced;
    const row = (p: MaiBotPromptInfo) =>
        lang && (
            <PromptRow
                key={p.name}
                info={p}
                selected={p.name === current?.name}
                dirty={drafts.get(promptDraftKey(lang, p.name)) !== undefined}
                onSelect={() => setSelected(p.name)}
            />
        );

    return (
        <div className="flex min-h-0 flex-1 gap-3">
            <aside className="flex w-60 shrink-0 flex-col overflow-hidden rounded-md border border-border-subtle bg-surface">
                <div className="flex flex-col gap-2 border-b border-border-subtle p-2.5">
                    {languages.length > 1 && lang && (
                        <Select
                            className="[&_button]:h-8 [&_button]:min-h-8 [&_button]:text-[12.5px]"
                            items={languages.map((l) => ({
                                value: l,
                                label: l === data.active_language ? `${promptLanguageLabel(l)} · 麦麦在用` : promptLanguageLabel(l),
                            }))}
                            value={lang}
                            onValueChange={(l) => {
                                setLanguage(l);
                                setSelected(null);
                            }}
                        />
                    )}
                    <div className="relative">
                        <Search
                            size={13}
                            className="pointer-events-none absolute left-2.5 top-1/2 -translate-y-1/2 text-text-tertiary"
                        />
                        <input
                            type="search"
                            aria-label="搜索提示词"
                            placeholder="搜索"
                            value={query}
                            onChange={(e) => setQuery(e.target.value)}
                            className="h-8 w-full rounded-sm border border-transparent bg-inset/70 pl-8 pr-2 text-[12.5px] text-text outline-none transition-colors placeholder:text-text-tertiary focus:border-brand/50 focus:bg-surface"
                        />
                    </div>
                </div>
                <nav className="min-h-0 flex-1 overflow-y-auto p-1.5" aria-label="提示词模板">
                    {shown.length === 0 ? (
                        <p className="px-2.5 py-6 text-center text-xs text-text-tertiary">没有对得上的模板</p>
                    ) : (
                        <>
                            {basic.length > 0 && (
                                <p className="px-2.5 pb-1 pt-1.5 text-2xs font-medium text-text-tertiary">常用</p>
                            )}
                            {basic.map(row)}
                            {advanced.length > 0 && (
                                <button
                                    type="button"
                                    aria-expanded={showAdvanced}
                                    onClick={() => setAdvancedOpen((v) => !v)}
                                    className="mt-2 flex w-full items-center gap-1 rounded-sm px-2.5 py-1.5 text-2xs font-medium text-text-tertiary hover:text-text-secondary"
                                >
                                    高级 · {advanced.length}
                                    <ChevronDown
                                        size={12}
                                        className={cn('ml-auto transition-transform duration-200', showAdvanced && 'rotate-180')}
                                    />
                                </button>
                            )}
                            {showAdvanced && advanced.map(row)}
                        </>
                    )}
                </nav>
            </aside>
            <section className="flex min-w-0 flex-1 flex-col overflow-hidden rounded-md border border-border-subtle bg-surface">
                {current && lang ? (
                    <PromptEditor
                        key={`${lang}/${current.name}`}
                        instanceId={instance.id}
                        mode={mode}
                        info={current}
                        language={lang}
                        activeLanguage={data.active_language}
                        live={data.live}
                        drafts={drafts}
                    />
                ) : (
                    <p className="m-auto text-sm text-text-tertiary">这个语言下没有提示词模板</p>
                )}
            </section>
        </div>
    );
};
