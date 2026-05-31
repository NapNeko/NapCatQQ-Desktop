// 网络 Tab。当前只有 GitHub PAT 一项（对齐 legacy Network.GitHubPersonalToken）。
//
// PAT 走后端 SecretStore（keyring），不与其它设置同文件落盘。改动进草稿，底部保存条落盘。
// 用 password 输入避免肩窥；配了 token 后检查更新 / 下载 release 走认证额度（5000 次/小时）。

import { useState } from 'react';
import { Eye, EyeOff } from 'lucide-react';
import type { BackendSettings } from '../../../core/services/settings.service';
import { TextField } from '../../../shared/ui';
import { FieldRow } from '../_shared';

interface Props {
    draft: BackendSettings | null;
    patchDraft: (patch: Partial<BackendSettings>) => void;
}

export function NetworkTab({ draft, patchDraft }: Props) {
    const [reveal, setReveal] = useState(false);

    return (
        <FieldRow
            label="GitHub Personal Token"
            description="可选。配置后检查更新 / 下载 release 走认证额度，避开匿名 60 次/小时限制。仅需 public_repo 或无权限的 classic token；存于系统密钥库，不明文落盘"
            isLast
        >
            <div className="flex items-center gap-1.5">
                <TextField
                    className="w-72"
                    type={reveal ? 'text' : 'password'}
                    placeholder="ghp_..."
                    autoComplete="off"
                    value={draft?.githubPat ?? ''}
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
