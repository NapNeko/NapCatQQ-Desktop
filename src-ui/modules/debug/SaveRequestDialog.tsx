// 「收藏」对话框：起个名字、放进哪个文件夹，存的是动作 + 参数（+ 通道，可选）。
// 中栏请求头上的 ☆ 和左栏历史里的「收藏」共用这一个框，两处存出来的东西一样、起名的方式也一样。
// 收藏是整份替换保存的：没读到现有收藏时绝不能保存，否则会把磁盘上已有的全盖掉。

import { useState } from 'react';
import { Button, Checkbox, Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle, Select, Spinner, TextField } from '../../shared/ui';
import { useDebugCollections, useSaveCollections } from '../../hooks/debug/useDebugCollections';
import { pushInfoBar } from '../../hooks/ui/globalInfoBarStore';
import { channelShortLabel } from '../../core/domain/debug/channelCopy';
import { MAX_NAME_LENGTH, addRequest, collectionsView, findSameRequest } from '../../core/domain/debug/collectionsOps';
import { countOmittedInValue } from '../../core/domain/debug/omittedParams';
import type { DebugChannelId } from '../../core/ipc/generated/debug/DebugChannelId';

const ROOT = '__root__';

export interface SaveRequestDialogProps {
    open: boolean;
    onOpenChange: (open: boolean) => void;
    action: string;
    /** 解析好的参数；JSON 写坏时是 null，这时不让收藏 */
    params: Record<string, unknown> | null;
    /** 可以一起记下的通道：标签自己指定的，或者历史里当时走的；没有是 null */
    channel: DebugChannelId | null;
    /**
     * 通道从哪来，决定勾选框怎么说、默认勾不勾：标签指定的默认记住（用户专门选过）；
     * 历史里当时走的默认不记（多半是「自动」挑的，重发时跟着顶栏走更合适）
     */
    channelFrom?: 'tab' | 'history';
    suggestedName: string;
}

export function SaveRequestDialog(props: SaveRequestDialogProps) {
    // 每次打开重新挂载表单：名字按当时的动作重新给默认值；关的动画期间内容还在，收起时不闪空，退场播完才真卸
    const [mounted, setMounted] = useState(props.open);
    if (props.open && !mounted) setMounted(true);
    return (
        <Dialog open={props.open} onOpenChange={props.onOpenChange}>
            <DialogContent size="sm" dismissOnOutsideClick={false} onExited={() => setMounted(false)}>
                {mounted && <SaveForm {...props} />}
            </DialogContent>
        </Dialog>
    );
}

function SaveForm({ onOpenChange, action, params, channel, channelFrom = 'tab', suggestedName }: SaveRequestDialogProps) {
    const collections = useDebugCollections();
    const save = useSaveCollections();
    const [name, setName] = useState(suggestedName);
    const [folder, setFolder] = useState<string>(ROOT);
    const [keepChannel, setKeepChannel] = useState(channel !== null && channelFrom === 'tab');

    const data = collections.data;
    const folders = data ? collectionsView(data).folders : [];
    const same = data && params ? findSameRequest(data, action, params) : null;
    // 历史 / 回放在存盘时被瘦过身的参数是不完整的：收进来是永久坏数据，补上原文再来
    const omitted = params ? countOmittedInValue(params) : 0;
    const blocker = !params
        ? '参数 JSON 有错，改好再收藏'
        : omitted > 0
          ? `有 ${omitted} 处超长参数在存盘时被省略，补上原文再收藏`
          : !data
            ? (collections.isError ? '读不到现有收藏，现在保存会把它们盖掉' : null)
            : null;

    const submit = () => {
        if (!data || !params || blocker) return;
        const { next } = addRequest(
            data,
            {
                name: name.trim() || action,
                action,
                params,
                channel: keepChannel ? channel : null,
                folderId: folder === ROOT ? null : folder,
            },
            Date.now(),
        );
        const label = name.trim() || action;
        save.mutate(next, {
            onSuccess: () =>
                pushInfoBar({ key: 'debug-save-request', tone: 'success', title: '已收藏', content: label, autoDismissMs: 2500 }),
        });
        onOpenChange(false);
    };

    return (
        <form
            onSubmit={(e) => {
                e.preventDefault();
                submit();
            }}
        >
            <DialogHeader>
                <DialogTitle>收藏请求</DialogTitle>
                <DialogDescription>
                    存下 <code className="font-mono text-text">{action}</code> 和这份参数，在左栏「收藏」里一键重发。
                </DialogDescription>
            </DialogHeader>
            <div className="space-y-3">
                <TextField
                    label="名字"
                    value={name}
                    maxLength={MAX_NAME_LENGTH}
                    autoFocus
                    onValueChange={setName}
                    placeholder={action}
                />
                {collections.isPending ? (
                    <p className="flex items-center gap-2 text-xs text-text-tertiary">
                        <Spinner size="xs" />
                        正在读取收藏…
                    </p>
                ) : (
                    <Select
                        label="放进文件夹"
                        value={folder}
                        onValueChange={setFolder}
                        items={[{ value: ROOT, label: '不放进文件夹' }, ...folders.map((f) => ({ value: f.id, label: f.name }))]}
                    />
                )}
                {channel && (
                    <Checkbox
                        checked={keepChannel}
                        onCheckedChange={setKeepChannel}
                        label={
                            channelFrom === 'tab'
                                ? `记住这个标签指定的通道（${channelShortLabel(channel)}）`
                                : `记住当时走的通道（${channelShortLabel(channel)}）`
                        }
                        hint="不记的话，重发时跟着顶栏选的通道走"
                    />
                )}
                {same && <p className="text-2xs text-warning">已经收藏过一份一样的请求：「{same.name}」。再存会多一份。</p>}
                {blocker && <p className="text-2xs text-danger">{blocker}</p>}
            </div>
            <DialogFooter>
                <Button variant="ghost" size="sm" onClick={() => onOpenChange(false)}>
                    取消
                </Button>
                <Button type="submit" variant="primary" size="sm" disabled={!!blocker || !data}>
                    收藏
                </Button>
            </DialogFooter>
        </form>
    );
}
