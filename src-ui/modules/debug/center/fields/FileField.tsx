// 文件 / 图片 / 语音 / 视频参数：URL、Bot 所在机器上的路径、base64，或点「选本机文件」
// 把这台机器上的文件传给远端 / 容器里的 Bot。本机文件不落参数本身：落成
// `ncd-local-file://<路径>` 占位，发送时后端把它传到 Bot 一侧再替换（见 stream.rs 的编排）。

import { FolderOpen, X } from 'lucide-react';
import { useDebugFileOps } from '../../../../hooks/debug/useDebugFileOps';
import {
    LOCAL_FILE_PREFIX,
    isLocalFileToken,
    localFileTokenFor,
} from '../../../../core/domain/debug/streamActions';
import { cn } from '../../../../shared/utils/cn';
import { FIELD_INPUT_CLASS, IconTip, fieldBorder } from '../centerParts';
import { TextField } from './TextField';
import type { FieldProps } from './fieldKit';

export const FILE_HINT = 'URL、Bot 所在机器上的路径，或 base64://…';

export function FileField(props: FieldProps) {
    const { value, onChange, invalid, inputId, describedBy, disabled } = props;
    const { pickLocalFile } = useDebugFileOps();
    const picking = pickLocalFile.isPending;

    if (isLocalFileToken(value)) {
        const path = value.slice(LOCAL_FILE_PREFIX.length);
        const name = path.split(/[\\/]/).filter(Boolean).pop() ?? path;
        return (
            <div
                id={inputId}
                aria-describedby={describedBy}
                className={cn(FIELD_INPUT_CLASS, fieldBorder(invalid), 'flex items-center gap-2')}
            >
                <span className="min-w-0 flex-1 truncate font-mono text-[13px]" title={path}>
                    {name}
                </span>
                <span className="shrink-0 rounded-xs bg-brand-soft px-1 text-[10px] leading-4 text-brand">
                    本机
                </span>
                <IconTip
                    icon={X}
                    label="去掉本机文件，改回手填"
                    size="sm"
                    onClick={() => onChange(undefined)}
                />
            </div>
        );
    }

    const pick = () => {
        pickLocalFile.mutate(undefined, {
            onSuccess: (picked) => {
                if (picked) onChange(localFileTokenFor(picked.path));
            },
        });
    };

    return (
        <div className="flex items-center gap-1.5">
            <div className="min-w-0 flex-1">
                <TextField {...props} mono placeholder={FILE_HINT} />
            </div>
            <IconTip
                icon={FolderOpen}
                label="选本机文件"
                hint="发送前传到 Bot 一侧再替换参数"
                disabled={disabled || picking}
                aria-busy={picking || undefined}
                onClick={pick}
            />
        </div>
    );
}
