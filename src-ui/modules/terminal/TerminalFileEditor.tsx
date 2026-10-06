// 文件栏里双击小文本文件直接改；Ctrl+S 保存，行尾按原样写回。

import { useEffect, useState } from 'react';
import {
    Button,
    Dialog,
    DialogContent,
    DialogFooter,
    DialogHeader,
    DialogTitle,
    SyntaxTextEditor,
} from '../../shared/ui';
import { editorSyntaxOf, baseName } from '../../core/domain/terminal/paths';
import type { TerminalTextFile } from '../../core/ipc/generated/domain/TerminalTextFile';

interface Props {
    file: TerminalTextFile | null;
    onSave(file: TerminalTextFile, content: string): Promise<boolean>;
    onClose(): void;
}

export function TerminalFileEditor({ file, onSave, onClose }: Props) {
    const [draft, setDraft] = useState('');
    const [saving, setSaving] = useState(false);

    useEffect(() => {
        setDraft(file?.content ?? '');
    }, [file]);

    const dirty = file !== null && draft !== file.content;

    const save = async () => {
        if (!file || !dirty || saving) return;
        setSaving(true);
        const ok = await onSave(file, draft);
        setSaving(false);
        if (ok) onClose();
    };

    return (
        <Dialog open={file !== null} onOpenChange={(open) => !open && onClose()}>
            <DialogContent size="sheetWide" dismissOnOutsideClick={false}>
                <div
                    className="flex min-h-0 flex-1 flex-col gap-3"
                    onKeyDown={(e) => {
                        if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 's') {
                            e.preventDefault();
                            void save();
                        }
                    }}
                >
                    <DialogHeader>
                        <DialogTitle>{file ? baseName(file.path) : ''}</DialogTitle>
                        <p className="truncate font-mono text-[11px] text-text-tertiary">
                            {file?.path}
                        </p>
                    </DialogHeader>
                    <SyntaxTextEditor
                        value={draft}
                        onChange={setDraft}
                        mode={file ? editorSyntaxOf(file.path) : 'plain'}
                        aria-label="文件内容"
                        className="min-h-[360px] flex-1"
                    />
                    <DialogFooter>
                        <Button variant="ghost" onClick={onClose}>
                            {dirty ? '不保存' : '关闭'}
                        </Button>
                        <Button
                            variant="primary"
                            disabled={!dirty || saving}
                            onClick={() => void save()}
                        >
                            {saving ? '保存中…' : '保存'}
                        </Button>
                    </DialogFooter>
                </div>
            </DialogContent>
        </Dialog>
    );
}
