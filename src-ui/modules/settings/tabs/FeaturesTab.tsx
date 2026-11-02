// 功能 Tab：可选功能模块开关，控制界面入口与后台探测资源。
// 保持设置页统一美学：左引导线 + FieldRow 统一布局，受限项使用 Icon + Tooltip 提示。

import { AlertTriangle } from 'lucide-react';
import { Switch, Tooltip, TooltipContent, TooltipTrigger } from '../../../shared/ui';
import { ExpandPresence } from '../../../shared/ui/motion';
import {
    FEATURE_GROUPS,
    appFrameworkOffBlock,
    featureOffBlock,
    featureOffWarning,
    isAppFrameworkVisible,
    setAppFrameworkVisible,
    type FeatureDef,
    type FeatureUsage,
} from '../../../core/domain/settings/features';
import { useAppInstanceUsage } from '../../../hooks/apps/useAppInstanceUsage';
import { useAppFrameworkCatalog } from '../../../hooks/apps/useAppInstances';
import { useBotBackendCounts } from '../../../hooks/bot/useBotBackendCounts';
import { useNcdWatchServers } from '../../../hooks/settings/useNcdWatchServers';
import { useTerminalState } from '../../../hooks/terminal/terminalStore';
import type { SettingsDraft } from '../settings-draft';
import { FieldRow, SettingsSection, SettingsTabSections } from '../_shared';

interface Props {
    draft: SettingsDraft | null;
    patchDraft: (patch: Partial<SettingsDraft>) => void;
}

export function FeaturesTab({ draft, patchDraft }: Props) {
    const usage = useFeatureUsage();
    const { data: frameworks = [] } = useAppFrameworkCatalog();

    if (!draft) {
        return <p className="text-[13px] text-text-tertiary">正在加载设置…</p>;
    }
    const features = draft.features;

    const renderFeature = (def: FeatureDef) => {
        const on = features[def.key];
        const blocked = on ? featureOffBlock(def.key, features, usage) : null;
        const warning = on ? null : featureOffWarning(def.key, usage);

        return (
            <FieldRow
                key={def.key}
                label={def.label}
                description={<Description text={def.description} saves={def.saves} />}
            >
                <div className="flex items-center gap-2">
                    {(blocked || warning) && (
                        <Tooltip>
                            <TooltipTrigger asChild>
                                <button
                                    type="button"
                                    aria-label={blocked ?? warning ?? undefined}
                                    className="inline-flex h-6 w-6 items-center justify-center rounded-full text-warning transition-colors hover:bg-warning/15 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand"
                                >
                                    <AlertTriangle size={14} strokeWidth={2.4} />
                                </button>
                            </TooltipTrigger>
                            <TooltipContent side="left" sideOffset={6}>
                                {blocked ?? warning}
                            </TooltipContent>
                        </Tooltip>
                    )}
                    <Switch
                        checked={on}
                        disabled={!!blocked}
                        onCheckedChange={(v) =>
                            patchDraft({ features: { ...features, [def.key]: v } })
                        }
                        aria-label={`切换 ${def.label}`}
                    />
                </div>
            </FieldRow>
        );
    };

    return (
        <SettingsTabSections>
            {FEATURE_GROUPS.map((group) => {
                const hasApps = group.items.some((d) => d.key === 'apps');

                return (
                    <SettingsSection
                        key={group.title}
                        title={group.title}
                        description={group.description}
                    >
                        {group.items.map(renderFeature)}

                        {hasApps && (
                            <ExpandPresence visible={features.apps && frameworks.length > 0}>
                                <div className="border-l border-border-subtle/80 pl-4 sm:pl-5 my-2">
                                    <div className="flex flex-col divide-y divide-border-subtle/70">
                                        {frameworks.map((m) => {
                                            const visible = isAppFrameworkVisible(features, m.id);
                                            const itemBlocked = visible
                                                ? appFrameworkOffBlock(m.id, usage)
                                                : null;

                                            return (
                                                <FieldRow
                                                    key={m.id}
                                                    label={m.display_name}
                                                    description={
                                                        m.description || '控制组件页与新建时的显示'
                                                    }
                                                >
                                                    <div className="flex items-center gap-2">
                                                        {itemBlocked && (
                                                            <Tooltip>
                                                                <TooltipTrigger asChild>
                                                                    <button
                                                                        type="button"
                                                                        aria-label={itemBlocked}
                                                                        className="inline-flex h-6 w-6 items-center justify-center rounded-full text-warning transition-colors hover:bg-warning/15 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand"
                                                                    >
                                                                        <AlertTriangle
                                                                            size={13}
                                                                            strokeWidth={2.4}
                                                                        />
                                                                    </button>
                                                                </TooltipTrigger>
                                                                <TooltipContent
                                                                    side="left"
                                                                    sideOffset={6}
                                                                >
                                                                    {itemBlocked}
                                                                </TooltipContent>
                                                            </Tooltip>
                                                        )}
                                                        <Switch
                                                            checked={visible}
                                                            disabled={!!itemBlocked}
                                                            onCheckedChange={(v) =>
                                                                patchDraft({
                                                                    features:
                                                                        setAppFrameworkVisible(
                                                                            features,
                                                                            m.id,
                                                                            v,
                                                                        ),
                                                                })
                                                            }
                                                            aria-label={`切换 ${m.display_name}`}
                                                        />
                                                    </div>
                                                </FieldRow>
                                            );
                                        })}
                                    </div>
                                </div>
                            </ExpandPresence>
                        )}
                    </SettingsSection>
                );
            })}
        </SettingsTabSections>
    );
}

function Description({ text, saves }: { text: string; saves?: string }) {
    if (!saves) return <>{text}</>;
    return (
        <>
            <span>{text}</span>
            <span className="mt-0.5 block text-[11.5px] text-text-tertiary">关掉后：{saves}</span>
        </>
    );
}

/** 判断能不能关要看的现状：Bot 用着哪个协议端、应用端实例、哪些远端装着 ncd-watch、开着几个终端。 */
function useFeatureUsage(): FeatureUsage {
    const botsByBackend = useBotBackendCounts();
    const apps = useAppInstanceUsage();
    const { rows: watchRows } = useNcdWatchServers();
    const openTerminals = Object.keys(useTerminalState().sessions).length;
    return {
        botsByBackend,
        activeAppInstances: apps.active,
        instancesByFramework: apps.byFramework,
        hostsWithNcdWatch: watchRows.filter((r) => r.watchInstalled === true).length,
        openTerminals,
    };
}
