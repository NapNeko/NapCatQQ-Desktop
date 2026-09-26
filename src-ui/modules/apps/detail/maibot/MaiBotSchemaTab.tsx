// 照 maibotPages 里的定义铺一页 bot_config：每个小节一个分组，高级字段默认收起，
// 在小节标题右边各自展开。

import type { ReactNode } from 'react';
import { ExternalLink } from 'lucide-react';
import { Button, FormSection } from '../../../../shared/ui';
import { ConfigForm } from '../karin/configLayout';
import type { MaiBotChatSession, MaiBotInstanceConfig } from '../../../../core/ipc/types';
import { advancedHasError, anyVisible, FieldGrid, hasAdvanced, type SchemaCtx } from './SchemaForm';
import { BOT_SCHEMA, nodeAt, type UiNode } from '../../../../core/domain/apps/maibotSchema';
import type { SchemaPageDef, SchemaSectionDef } from './maibotPages';
import { AdvancedToggle, useRevealOnError, type AdvancedSections } from './advancedToggle';

function sectionNode(s: SchemaSectionDef): UiNode | undefined {
    const node = nodeAt(BOT_SCHEMA, s.path);
    if (!node || !s.fields) return node;
    return { ...node, fields: node.fields.filter((f) => s.fields?.includes(f.name)) };
}

export const MaiBotSchemaTab: React.FC<{
    /** 页的 id，和小节路径一起当展开状态的键 */
    tab: string;
    page: SchemaPageDef;
    config: MaiBotInstanceConfig;
    onChange: (next: MaiBotInstanceConfig) => void;
    errors: Record<string, string>;
    disabled?: boolean;
    advancedSections: AdvancedSections;
    /** 页顶的说明或运行状态 */
    intro?: ReactNode;
    /** 麦麦在跑时它见过的聊天，按聊天配的列表能直接挑 */
    chatTargets?: readonly MaiBotChatSession[];
    /** WebUI 应答了才能开 links 里的页 */
    live: boolean;
    onOpenWebUi: (path: string) => void;
}> = ({
    tab,
    page,
    config,
    onChange,
    errors,
    disabled,
    advancedSections,
    intro,
    chatTargets,
    live,
    onOpenWebUi,
}) => {
    const base: SchemaCtx = {
        value: config.bot,
        onChange: (bot) => onChange({ ...config, bot: bot as MaiBotInstanceConfig['bot'] }),
        errors,
        prefix: 'bot',
        disabled,
        showAdvanced: false,
        skip: new Set(page.skip),
        advanced: new Set(page.advanced),
        chatTargets,
    };
    const sections = page.sections.map((def) => {
        const node = sectionNode(def);
        return {
            def,
            node,
            key: `${tab}/${def.path.join('.')}`,
            collapsible: hasAdvanced(base, def.path, node),
            errored: !!node && advancedHasError(base, def.path, node),
        };
    });
    useRevealOnError(
        advancedSections,
        sections.filter((s) => s.errored).map((s) => s.key),
    );

    const links = page.links ?? [];
    return (
        <ConfigForm>
            {links.length > 0 && (
                <div className="flex flex-wrap items-center gap-1">
                    {links.map((l) => (
                        <Button
                            key={l.path}
                            size="sm"
                            variant="ghost"
                            disabled={!live}
                            title={live ? `在麦麦的 WebUI 里看${l.label}` : '启动麦麦后能看'}
                            onClick={() => onOpenWebUi(l.path)}
                        >
                            <ExternalLink size={13} />
                            {l.label}
                        </Button>
                    ))}
                </div>
            )}
            {intro}
            {sections.map(({ def, node, key, collapsible }) => {
                if (!node) return null;
                const open = collapsible && advancedSections.isOpen(key);
                const ctx = open ? { ...base, showAdvanced: true } : base;
                const visible = anyVisible(ctx, def.path, node);
                // 整节都是高级字段时收起也留着标题，不然展开的入口就没了
                if (!visible && !collapsible) return null;
                return (
                    <FormSection
                        key={key}
                        title={def.title}
                        description={def.description}
                        actions={
                            collapsible ? (
                                <AdvancedToggle open={open} onToggle={() => advancedSections.toggle(key)} />
                            ) : undefined
                        }
                    >
                        {visible && <FieldGrid ctx={ctx} path={def.path} node={node} />}
                    </FormSection>
                );
            })}
        </ConfigForm>
    );
};
