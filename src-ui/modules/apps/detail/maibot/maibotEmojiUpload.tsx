// 上传表情包：挑好 / 拖进来的图先在本机过一遍（预览、传不了的当场说），这一批共用一组情绪标签。

import { ImagePlus, Upload, X } from 'lucide-react';
import { Button, StringListField } from '../../../../shared/ui';
import { cn } from '../../../../shared/utils/cn';
import { normalizeEmojiTags } from '../../../../core/domain/apps/maibotEmoji';
import type { MaiBotLocalImage } from '../../../../core/ipc/types';
import { FormDialog } from '../entityParts';

function formatSize(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`;
    return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

export const EmojiUploadDialog: React.FC<{
    /** null 是关着 */
    files: readonly MaiBotLocalImage[] | null;
    tags: string[];
    busy: boolean;
    onTags: (tags: string[]) => void;
    onRemove: (path: string) => void;
    onAddMore: () => void;
    onCancel: () => void;
    onConfirm: () => void;
}> = ({ files, tags, busy, onTags, onRemove, onAddMore, onCancel, onConfirm }) => {
    if (!files) return null;
    const ok = files.filter((f) => !f.problem);
    return (
        <FormDialog
            open
            size="lg"
            title="上传表情包"
            description="传上去就算收下了，下一轮表情包维护后麦麦开始用。"
            confirmLabel={ok.length > 0 ? `上传 ${ok.length} 张` : '上传'}
            confirmDisabled={ok.length === 0}
            busy={busy}
            onCancel={onCancel}
            onConfirm={onConfirm}
        >
            <div className="grid grid-cols-[repeat(auto-fill,minmax(6.5rem,1fr))] gap-2">
                {files.map((f) => (
                    <div
                        key={f.path}
                        className={cn(
                            'group relative flex flex-col overflow-hidden rounded-md border',
                            f.problem
                                ? 'border-danger/40 bg-danger-soft/40'
                                : 'border-border-subtle bg-surface',
                        )}
                    >
                        <div className="flex aspect-square items-center justify-center bg-field p-1.5">
                            {f.preview ? (
                                <img
                                    src={f.preview}
                                    alt=""
                                    draggable={false}
                                    className="h-full w-full object-contain"
                                />
                            ) : (
                                <ImagePlus size={20} className="text-text-disabled" />
                            )}
                        </div>
                        <div className="px-2 py-1.5">
                            <p className="truncate text-2xs text-text" title={f.name}>
                                {f.name}
                            </p>
                            <p
                                className={cn(
                                    'text-2xs',
                                    f.problem
                                        ? 'line-clamp-2 text-danger'
                                        : 'truncate text-text-tertiary',
                                )}
                                title={f.problem}
                            >
                                {f.problem ?? formatSize(f.size)}
                            </p>
                        </div>
                        <button
                            type="button"
                            aria-label={`不传 ${f.name}`}
                            onClick={() => onRemove(f.path)}
                            className="absolute right-1 top-1 inline-flex h-5 w-5 items-center justify-center rounded-full bg-black/50 text-white opacity-0 transition-opacity focus-visible:opacity-100 group-hover:opacity-100"
                        >
                            <X size={11} />
                        </button>
                    </div>
                ))}
                <button
                    type="button"
                    onClick={onAddMore}
                    className="flex aspect-square flex-col items-center justify-center gap-1 rounded-md border border-dashed border-border text-text-tertiary transition-colors hover:border-brand/50 hover:text-brand"
                >
                    <ImagePlus size={18} />
                    <span className="text-2xs">再加几张</span>
                </button>
            </div>
            <StringListField
                label="情绪标签（这一批共用）"
                hint="麦麦按标签挑表情，没标签的挑不中。传完也能一张张改"
                mono={false}
                placeholder="比如：开心、得意，回车加上"
                value={tags}
                onChange={(next) => onTags(normalizeEmojiTags(next))}
            />
        </FormDialog>
    );
};

/** 往窗口里拖图时盖在表情包页上的一层 */
export const EmojiDropOverlay: React.FC<{ visible: boolean }> = ({ visible }) =>
    visible ? (
        <div className="pointer-events-none absolute inset-0 z-20 flex items-center justify-center rounded-md border-2 border-dashed border-brand/60 bg-brand-soft/60 backdrop-blur-[1px]">
            <div className="flex flex-col items-center gap-2 text-brand">
                <Upload size={26} />
                <span className="text-sm font-medium">松手就加进来</span>
            </div>
        </div>
    ) : null;

export const UploadButton: React.FC<{ onClick: () => void; busy?: boolean }> = ({
    onClick,
    busy,
}) => (
    <Button size="sm" variant="secondary" disabled={busy} onClick={onClick}>
        <Upload size={13} />
        上传
    </Button>
);
