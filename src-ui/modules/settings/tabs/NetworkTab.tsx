// 网络 Tab。GitHub PAT 改动进入统一草稿，右上角保存后写入 SecretStore。

import { useState } from 'react';
import { Eye, EyeOff } from 'lucide-react';
import { TextField } from '../../../shared/ui';
import type { SettingsDraft } from '../settings-draft';
import { FieldRow } from '../_shared';

interface Props {
    draft: SettingsDraft | null;
    patchDraft: (patch: Partial<SettingsDraft>) => void;
}

export function NetworkTab({ draft, patchDraft }: Props) {
    const [reveal, setReveal] = useState(false);

    if (!draft) {
        return (
            <p className="text-[13px] text-text-tertiary">正在加载设置…</p>
        );
    }

    return (
        <FieldRow
            label="GitHub Personal Token"
            description="可选。配置后检查更新 / 下载 release 走认证额度。仅需 public_repo 或无权限 classic token；存于系统密钥库"
            isLast
        >
            <div className="flex items-center gap-1.5">
                <TextField
                    className="w-72"
                    type={reveal ? 'text' : 'password'}
                    placeholder="ghp_..."
                    autoComplete="off"
                    value={draft.githubPat}
                    onValueChange={(v) => patchDraft({ githubPat: v })}
                />
                <button
                    type="button"
                    onClick={() => setReveal((r) => !r)}
                    className="flex h-8 w-8 items-center justify-center rounded-sm text-text-tertiary transition-colors hover:bg-inset hover:text-text"
                    aria-label={reveal ? '隐藏 token' : '显示 token'}
                >
                    {reveal ? <EyeOff size={15} /> : <Eye size={15} />}
                </button>
            </div>
        </FieldRow>
    );
}