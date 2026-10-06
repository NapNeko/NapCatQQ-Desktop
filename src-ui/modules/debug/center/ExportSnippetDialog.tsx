// 「导出调用代码」对话框：按当前标签的动作、参数和实际生效的调用通道生成 curl / JavaScript / Python 片段。
// token 只有打码值（token_hint），片段里写的也是打码值——对话框上明说，用之前换掉。

import { useMemo, useState } from 'react';
import { Check, Copy } from 'lucide-react';
import {
    Button,
    Dialog,
    DialogContent,
    DialogDescription,
    DialogHeader,
    DialogTitle,
    Tabs,
    TabsContent,
    TabsList,
    TabsTrigger,
} from '../../../shared/ui';
import { useDebugChannels } from '../../../hooks/debug/useDebugChannels';
import { effectiveChannelId, findChannel } from '../../../core/domain/debug/channelPick';
import {
    exportChannelOf,
    SNIPPET_LANGS,
    snippetCode,
    type SnippetLang,
} from '../../../core/domain/debug/codeExport';
import type { DebugChannelId } from '../../../core/ipc/generated/debug/DebugChannelId';
import type { DebugRequestDraft } from '../../../core/ipc/generated/debug/DebugRequestDraft';
import { copyWithToast } from './centerParts';

export interface ExportSnippetDialogProps {
    open: boolean;
    onOpenChange: (open: boolean) => void;
    tab: DebugRequestDraft;
    /** 已解析好的参数（参数 JSON 有错时入口是禁用的，不会给坏数据） */
    params: Record<string, unknown>;
    botId: string | null;
    /** 顶栏为这个 Bot 选的调用通道；标签自己指定了通道的以标签为准 */
    callChannel: DebugChannelId;
}

export function ExportSnippetDialog({
    open,
    onOpenChange,
    tab,
    params,
    botId,
    callChannel,
}: ExportSnippetDialogProps) {
    const channels = useDebugChannels(botId).data;
    const [lang, setLang] = useState<SnippetLang>('curl');
    const [copied, setCopied] = useState(false);

    const action = tab.action.trim();
    const resolved = effectiveChannelId(channels, tab.channel ?? callChannel, 'call');
    const channel = exportChannelOf(resolved, findChannel(channels, resolved));
    const code = useMemo(
        () => snippetCode(lang, { action, params, channel }),
        [lang, action, params, channel],
    );
    const langLabel = SNIPPET_LANGS.find((l) => l.id === lang)?.label ?? lang;

    const copy = () => {
        void copyWithToast(code, `已复制 ${langLabel} 片段`);
        setCopied(true);
        window.setTimeout(() => setCopied(false), 2000);
    };

    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent size="sheet">
                <DialogHeader>
                    <DialogTitle>导出调用代码</DialogTitle>
                    <DialogDescription>
                        按通道 {channel.label} 生成{' '}
                        <code className="font-mono text-text">{action}</code> 的调用片段
                    </DialogDescription>
                </DialogHeader>
                {(channel.placeholderReason || channel.tokenHint) && (
                    <div className="space-y-1 text-2xs leading-relaxed">
                        {channel.placeholderReason && (
                            <p className="text-warning">{channel.placeholderReason}</p>
                        )}
                        {channel.tokenHint && (
                            <p className="text-text-secondary">
                                token 打码显示（{channel.tokenHint}
                                ）：片段里写的就是打码值，用之前换成真实 token。
                            </p>
                        )}
                    </div>
                )}
                <Tabs
                    value={lang}
                    onValueChange={(v) => setLang(v as SnippetLang)}
                    className="flex min-h-0 flex-1 flex-col"
                >
                    <div className="flex items-center gap-2">
                        <TabsList className="h-8 border-b-0">
                            {SNIPPET_LANGS.map((l) => (
                                <TabsTrigger key={l.id} value={l.id} className="h-7 px-2.5 text-xs">
                                    {l.label}
                                </TabsTrigger>
                            ))}
                        </TabsList>
                        <Button size="sm" variant="secondary" className="ml-auto" onClick={copy}>
                            {copied ? (
                                <Check size={12} aria-hidden className="text-success" />
                            ) : (
                                <Copy size={12} aria-hidden />
                            )}
                            {copied ? '已复制' : '复制片段'}
                        </Button>
                    </div>
                    <TabsContent
                        value={lang}
                        className="mt-2 min-h-0 flex-1 overflow-auto rounded-sm border border-border-subtle bg-inset"
                    >
                        <pre className="px-3 py-2 font-mono text-[11.5px] leading-relaxed text-text">
                            {code}
                        </pre>
                    </TabsContent>
                </Tabs>
            </DialogContent>
        </Dialog>
    );
}
