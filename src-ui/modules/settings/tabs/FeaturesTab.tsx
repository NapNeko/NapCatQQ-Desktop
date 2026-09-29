// 功能 Tab：可选功能模块开关。用不上的整块拿掉，侧栏和各页入口跟着少；
// 关掉能省后台活的，每行下面写着省了什么。

import type { ReactNode } from 'react';
import { Switch } from '../../../shared/ui';
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
                description={<Description text={def.description} saves={def.saves} note={blocked ?? warning} />}
            >
                <Switch
                    checked={on}
                    disabled={!!blocked}
                    onCheckedChange={(v) => patchDraft({ features: { ...features, [def.key]: v } })}
                />
            </FieldRow>
        );
    };

    return (
        <SettingsTabSections>
            <p className="-mb-6 text-[12px] leading-relaxed text-text-tertiary">
                用不上的可以关掉，侧栏和各页里对应的入口一起隐藏；已有的配置和数据不删，再打开就回来。
                关掉会让东西没处看、没处管的（比如还有 Bot 在用），开关会先拦住。
            </p>
            {FEATURE_GROUPS.map((group) => (
                <SettingsSection key={group.title} title={group.title}>
                    {group.items.map(renderFeature)}
                    {group.items.some((d) => d.key === 'apps') &&
                        features.apps &&
                        frameworks.map((m) => {
                            const visible = isAppFrameworkVisible(features, m.id);
                            const blocked = visible ? appFrameworkOffBlock(m.id, usage) : null;
                            return (
                                <FieldRow
                                    key={m.id}
                                    label={`应用端 · ${m.display_name}`}
                                    description={
                                        <Description
                                            text={`关掉后组件页不列 ${m.display_name}，新建、导入时也不给选`}
                                            note={blocked}
                                        />
                                    }
                                >
                                    <Switch
                                        checked={visible}
                                        disabled={!!blocked}
                                        onCheckedChange={(v) =>
                                            patchDraft({ features: setAppFrameworkVisible(features, m.id, v) })
                                        }
                                    />
                                </FieldRow>
                            );
                        })}
                </SettingsSection>
            ))}
        </SettingsTabSections>
    );
}

function Description({ text, saves, note }: { text: string; saves?: string; note?: string | null }) {
    const extra: ReactNode[] = [];
    if (saves) extra.push(<span key="saves" className="mt-0.5 block text-text-secondary">关掉后：{saves}</span>);
    if (note) extra.push(<span key="note" className="mt-1 block text-warning">{note}</span>);
    if (extra.length === 0) return <>{text}</>;
    return (
        <>
            {text}
            {extra}
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
