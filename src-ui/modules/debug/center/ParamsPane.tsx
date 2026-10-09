// 请求参数区：子页「参数 / 文档」，参数又有「表单 / JSON」两种写法。
//
// 两种写法读写同一份 params_text：表单改字段是对文本做最小修改，JSON 里改的就是文本本身。
// JSON 写坏了表单锁住（内容照旧显示、变灰），提示错在第几行，一键跳过去改；文本一个字都不丢。
// 两个视图都一直挂着，只是藏起不看的那个：来回切换时 JSON 编辑器的光标和撤销记录都还在。

import { memo, useEffect, useLayoutEffect, useRef, useState, type RefObject } from 'react';
import {
    AlertTriangle,
    AlignLeft,
    ArrowRight,
    BookOpen,
    ChevronDown,
    ChevronUp,
    SlidersHorizontal,
} from 'lucide-react';
import { Button, JsonCodeEditor, Spinner, type JsonCodeEditorHandle } from '../../../shared/ui';
import { cn } from '../../../shared/utils/cn';
import { debugWorkspaceStore } from '../../../hooks/debug/debugWorkspaceStore';
import { formatParams, type ParamsParse } from '../../../core/domain/debug/paramsText';
import type { FormModel } from '../../../core/domain/debug/schemaForm';
import type { ParamIssue } from '../../../core/domain/debug/validate';
import type { DebugActionSpec } from '../../../core/ipc/generated/debug/DebugActionSpec';
import type { DebugRequestDraft } from '../../../core/ipc/generated/debug/DebugRequestDraft';
import type { DebugTarget } from '../../../core/ipc/generated/debug/DebugTarget';
import { MOD_KEY_LABEL } from '../TopBar';
import { IconTip, Segmented } from './centerParts';
import { DocsPane } from './DocsPane';
import { ParamsForm, fieldInputId } from './ParamsForm';
import { lineOfKey } from './viewHelpers';
import { useMotion } from '../../../hooks/preferences/useMotion';
import { cssEase } from '../../../core/design/cssEase';

export type ParamsSubTab = 'params' | 'docs';

// React 18 还不认 inert 这个布尔属性，写成空串字符串属性浏览器照样生效
const INERT = { inert: '' } as Record<string, string>;
export type ParamsView = 'form' | 'json';

export interface ParamsPaneProps {
    tab: DebugRequestDraft;
    spec: DebugActionSpec | null;
    specLoading: boolean;
    model: FormModel | null;
    parsed: ParamsParse;
    issues: readonly ParamIssue[];
    target: DebugTarget | null;
    sub: ParamsSubTab;
    onSubChange: (sub: ParamsSubTab) => void;
    view: ParamsView;
    onViewChange: (view: ParamsView) => void;
    /** 参数被用户改过（套用示例前要问） */
    dirty: boolean;
    onSubmit: () => void;
    /** 「去改这个参数」：外面（发送条上的问题汇总）点了之后聚焦对应字段 */
    focusRequest: { name: string; nonce: number } | null;
    collapsed?: boolean;
    onCollapsedChange?: (collapsed: boolean) => void;
}

export const ParamsPane = memo(function ParamsPane({
    tab,
    spec,
    specLoading,
    model,
    parsed,
    issues,
    target,
    sub,
    onSubChange,
    view,
    onViewChange,
    dirty,
    onSubmit,
    focusRequest,
    collapsed = false,
    onCollapsedChange,
}: ParamsPaneProps) {
    const {
        enabled,
        duration,
        ease: { enter },
    } = useMotion();
    const contentRef = useRef<HTMLDivElement>(null);
    const editorRef = useRef<JsonCodeEditorHandle>(null);
    const formScrollRef = useRef<HTMLDivElement>(null);
    const [revealAt, setRevealAt] = useState<{ line: number; nonce: number } | null>(null);
    // JSON 写坏的时候表单照旧显示上一份能解析的内容（变灰锁住），而不是突然清空
    const lastGood = useRef<Record<string, unknown>>({});
    if (parsed.ok) lastGood.current = parsed.value;

    const action = tab.action.trim();
    // 目录里没有的动作（自由输入的、_async 变体）只能写 JSON。说明还在读时当作「有表单」：
    // 先强切到 JSON、读到后再切回来，会闪一下、还会把焦点弄丢
    const formAvailable = !!model || specLoading;
    const effectiveView: ParamsView = formAvailable ? view : 'json';
    const paneKey = `${sub}:${effectiveView}:${collapsed}`;
    const shownPane = useRef(paneKey);
    useLayoutEffect(() => {
        const changed = shownPane.current !== paneKey;
        shownPane.current = paneKey;
        const el = contentRef.current;
        if (!changed || collapsed || !enabled || !el || typeof el.animate !== 'function') return;
        const anim = el.animate(
            [
                { opacity: 0, transform: 'translateY(4px)' },
                { opacity: 1, transform: 'none' },
            ],
            { duration: duration('fast') * 1000, easing: cssEase(enter) },
        );
        return () => anim.cancel();
    }, [paneKey, collapsed, enabled, duration, enter]);
    // 「跳到第几行」「聚焦某个字段」都只执行一次：记下处理过的那次，之后切视图不再重放（不然光标、焦点会被抢走）
    const consumedReveal = useRef(0);
    const consumedFocus = useRef(focusRequest?.nonce ?? 0);

    const revealInJson = (line: number) => {
        onSubChange('params');
        onViewChange('json');
        setRevealAt((prev) => ({ line, nonce: (prev?.nonce ?? 0) + 1 }));
    };

    // 切到 JSON 视图之后（这一帧它才显示出来）再跳行，不然滚动量是按隐藏时的尺寸算的
    useEffect(() => {
        if (
            !revealAt ||
            revealAt.nonce === consumedReveal.current ||
            effectiveView !== 'json' ||
            sub !== 'params'
        )
            return;
        const frame = requestAnimationFrame(() => {
            consumedReveal.current = revealAt.nonce;
            editorRef.current?.revealLine(revealAt.line);
        });
        return () => cancelAnimationFrame(frame);
    }, [revealAt, effectiveView, sub]);

    useEffect(() => {
        if (
            !focusRequest ||
            focusRequest.nonce === consumedFocus.current ||
            sub !== 'params' ||
            effectiveView !== 'form'
        ) {
            return;
        }
        const frame = requestAnimationFrame(() => {
            consumedFocus.current = focusRequest.nonce;
            const host = document.getElementById(fieldInputId(tab.id, focusRequest.name));
            const el = host?.matches('input, textarea, button, [role="combobox"]')
                ? host
                : host?.querySelector<HTMLElement>(
                      'input, textarea, button, [contenteditable="true"]',
                  );
            host?.closest('[data-param]')?.scrollIntoView({ block: 'nearest' });
            (el as HTMLElement | null | undefined)?.focus();
        });
        return () => cancelAnimationFrame(frame);
    }, [focusRequest, sub, effectiveView, tab.id]);

    const setText = (text: string) => debugWorkspaceStore.setParamsText(tab.id, text);
    const format = () => {
        if (parsed.ok) setText(formatParams(parsed.value));
    };

    return (
        <div className="flex min-h-0 flex-1 flex-col">
            <div className="flex h-9 shrink-0 items-center gap-2 border-b border-border-subtle/70 px-3">
                <Segmented<ParamsSubTab>
                    label="参数或文档"
                    value={sub}
                    onChange={(next) => {
                        onCollapsedChange?.(false);
                        onSubChange(next);
                    }}
                    options={[
                        {
                            value: 'params',
                            label: (
                                <>
                                    <SlidersHorizontal size={11} aria-hidden />
                                    参数
                                </>
                            ),
                        },
                        {
                            value: 'docs',
                            label: (
                                <>
                                    <BookOpen size={11} aria-hidden />
                                    文档
                                </>
                            ),
                            disabled: !spec && !specLoading,
                            title:
                                spec || specLoading
                                    ? undefined
                                    : action
                                      ? '目录里没有这个接口的说明'
                                      : '先填接口名',
                        },
                    ]}
                />
                <span className="flex-1" />
                {sub === 'params' && !collapsed && (
                    <>
                        {effectiveView === 'json' && (
                            <IconTip
                                icon={AlignLeft}
                                label="格式化 JSON"
                                size="sm"
                                disabled={!parsed.ok}
                                onClick={format}
                            />
                        )}
                        <Segmented<ParamsView>
                            label="参数的写法"
                            value={effectiveView}
                            onChange={onViewChange}
                            options={[
                                {
                                    value: 'form',
                                    label: '表单',
                                    disabled: !formAvailable,
                                    title: formAvailable
                                        ? undefined
                                        : '目录里没有这个接口的参数说明，只能写 JSON',
                                },
                                { value: 'json', label: 'JSON' },
                            ]}
                        />
                    </>
                )}
                {onCollapsedChange && (
                    <IconTip
                        icon={collapsed ? ChevronDown : ChevronUp}
                        label={collapsed ? '展开参数和文档' : '收起参数和文档'}
                        size="sm"
                        aria-expanded={!collapsed}
                        onClick={() => onCollapsedChange(!collapsed)}
                    />
                )}
            </div>

            <div
                ref={contentRef}
                className={cn('flex min-h-0 flex-1 flex-col', collapsed && 'hidden')}
            >
                <div
                    className={cn(
                        'relative flex min-h-0 flex-1 flex-col',
                        sub !== 'params' && 'hidden',
                    )}
                >
                    {/* 表单 */}
                    <div
                        className={cn(
                            'relative flex min-h-0 flex-1 flex-col',
                            effectiveView !== 'form' && 'hidden',
                        )}
                    >
                        <div
                            ref={formScrollRef}
                            className={cn(
                                'min-h-0 flex-1 overflow-y-auto transition-opacity duration-150',
                                !parsed.ok && 'pointer-events-none select-none opacity-35',
                            )}
                            aria-hidden={!parsed.ok || undefined}
                            // 锁住时键盘也进不去：否则 Tab 能落到变灰的输入框上改出一份和 JSON 不一致的东西
                            {...(!parsed.ok ? INERT : {})}
                        >
                            {model ? (
                                <ParamsForm
                                    tabId={tab.id}
                                    model={model}
                                    values={parsed.ok ? parsed.value : lastGood.current}
                                    issues={parsed.ok ? issues : []}
                                    target={target}
                                    onSubmit={onSubmit}
                                    onEditInJson={(key) =>
                                        revealInJson(lineOfKey(tab.params_text, key) ?? 1)
                                    }
                                />
                            ) : specLoading ? (
                                <div
                                    className="flex items-center gap-2 px-3 py-4 text-xs text-text-tertiary"
                                    role="status"
                                >
                                    <Spinner size="xs" />
                                    正在读取接口说明，读到就出表单；也可以先切到 JSON 写
                                </div>
                            ) : null}
                        </div>
                        {!parsed.ok && (
                            <div
                                className="absolute inset-x-3 top-3 flex justify-center"
                                role="alert"
                            >
                                <div className="flex max-w-md items-start gap-2.5 rounded-md border border-warning/40 bg-elevated px-3 py-2.5 shadow-popover">
                                    <AlertTriangle
                                        size={15}
                                        strokeWidth={2.2}
                                        aria-hidden
                                        className="mt-0.5 shrink-0 text-warning"
                                    />
                                    <div className="min-w-0 space-y-1">
                                        <p className="text-[13px] font-medium text-text">
                                            JSON 第 {parsed.line} 行有错，改好后表单恢复
                                        </p>
                                        <p className="break-words text-2xs text-text-tertiary">
                                            {parsed.message}
                                        </p>
                                        <Button
                                            size="sm"
                                            variant="secondary"
                                            className="mt-1"
                                            onClick={() => revealInJson(parsed.line)}
                                        >
                                            去 JSON 修改
                                            <ArrowRight size={12} aria-hidden />
                                        </Button>
                                    </div>
                                </div>
                            </div>
                        )}
                    </div>

                    {/* JSON */}
                    <div
                        className={cn(
                            'flex min-h-0 flex-1 flex-col gap-1.5 p-2',
                            effectiveView !== 'json' && 'hidden',
                        )}
                    >
                        {!formAvailable && action && !specLoading && (
                            <p className="shrink-0 px-1 text-2xs text-text-tertiary">
                                目录里没有{' '}
                                <code className="font-mono text-text-secondary">{action}</code>
                                ，没有表单和校验，照样可以发。
                            </p>
                        )}
                        <JsonCodeEditor
                            value={tab.params_text}
                            onChange={setText}
                            schema={spec?.params_schema ?? null}
                            onSubmit={onSubmit}
                            ariaLabel="请求参数 JSON"
                            handleRef={editorRef as RefObject<JsonCodeEditorHandle>}
                        />
                        {/* 不设 live region：敲一个字就读一遍错误太吵；出错时编辑器本身有诊断标记 */}
                        <p
                            className={cn(
                                'shrink-0 truncate px-1 text-2xs',
                                parsed.ok ? 'text-text-tertiary' : 'text-danger',
                            )}
                        >
                            {parsed.ok
                                ? `Ctrl+Space 补全参数名 · ${MOD_KEY_LABEL}+Enter 发送`
                                : `第 ${parsed.line} 行第 ${parsed.column} 列：${parsed.message}`}
                        </p>
                    </div>
                </div>

                {sub === 'docs' && !spec && specLoading && (
                    <div
                        className="flex items-center gap-2 px-3 py-4 text-xs text-text-tertiary"
                        role="status"
                    >
                        <Spinner size="xs" />
                        正在读取接口说明…
                    </div>
                )}
                {sub === 'docs' && spec && (
                    <div className="min-h-0 flex-1 overflow-y-auto">
                        <DocsPane
                            spec={spec}
                            backend={target?.backend ?? null}
                            dirty={dirty}
                            onUseExample={(text) => {
                                setText(text);
                                onSubChange('params');
                            }}
                        />
                    </div>
                )}
            </div>
        </div>
    );
});
