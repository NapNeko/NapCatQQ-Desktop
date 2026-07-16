// 数据 Tab：数据根目录、整树迁移、配置导入导出、GitHub Token。

import { useState } from 'react';
import { Eye, EyeOff } from 'lucide-react';
import { useConfigTransfer } from '../../../hooks/preferences/useConfigTransfer';
import { ConfigImportDialog } from '../ConfigImportDialog';
import { DataRootMigrateDialog } from '../DataRootMigrateDialog';
import { Button, TextField } from '../../../shared/ui';
import { ActionMotionIcon } from '../../../shared/ui/motion';
import type { SettingsDraft } from '../settings-draft';
import { FieldRow, SettingsSection, SettingsTabSections } from '../_shared';

interface Props {
    dataRoot: string;
    onOpenDataDir: () => Promise<string>;
    isOpeningDir: boolean;
    draft: SettingsDraft | null;
    patchDraft: (patch: Partial<SettingsDraft>) => void;
}

export function DataTab({
    dataRoot,
    onOpenDataDir,
    isOpeningDir,
    draft,
    patchDraft,
}: Props) {
    const [revealPat, setRevealPat] = useState(false);
    const [migrateOpen, setMigrateOpen] = useState(false);
    const {
        exportConfig,
        openImportWizard,
        importOpen,
        setImportOpen,
        onImported,
        isExporting: isExportingCfg,
    } = useConfigTransfer();

    const handleOpen = async () => {
        try {
            await onOpenDataDir();
        } catch (err) {
            // eslint-disable-next-line no-console
            console.warn('打开数据目录失败:', err);
        }
    };

    return (
        <SettingsTabSections>
            <SettingsSection title="存储">
                <FieldRow
                    label="数据根目录"
                    description={
                        <span className="break-all font-mono text-[12px] text-text-tertiary">
                            {dataRoot}
                        </span>
                    }
                    isLast
                >
                    <div className="flex shrink-0 items-center gap-1.5">
                        <Button
                            variant="secondary"
                            size="sm"
                            onClick={handleOpen}
                            disabled={isOpeningDir}
                        >
                            打开
                        </Button>
                        <Button
                            variant="secondary"
                            size="sm"
                            onClick={() => setMigrateOpen(true)}
                            disabled={!dataRoot || dataRoot === '—'}
                        >
                            迁移…
                        </Button>
                    </div>
                </FieldRow>
            </SettingsSection>

            <SettingsSection
                title="配置备份"
                description="仅配置 JSON 子集；不含 secrets / 组件安装树。换盘请用「数据根目录」旁的迁移。"
            >
                <FieldRow
                    label="导出当前配置"
                    description="保存为 ZIP 包（config.json、bot.json、servers.json 与元数据；不含密钥）"
                >
                    <Button
                        variant="secondary"
                        size="sm"
                        onClick={() => exportConfig()}
                        disabled={isExportingCfg}
                    >
                        {isExportingCfg ? '导出中…' : '导出 ZIP'}
                    </Button>
                </FieldRow>

                <FieldRow
                    label="导入配置"
                    description="从 ZIP 或文件夹恢复；向导内可预览将写入的项"
                    isLast
                >
                    <Button variant="secondary" size="sm" onClick={openImportWizard}>
                        打开导入向导
                    </Button>
                </FieldRow>
            </SettingsSection>

            <ConfigImportDialog
                open={importOpen}
                onOpenChange={setImportOpen}
                onImported={onImported}
            />

            <DataRootMigrateDialog
                open={migrateOpen}
                onOpenChange={setMigrateOpen}
                currentDataRoot={dataRoot}
            />

            <SettingsSection
                title="GitHub"
                description="可选；组件页检查 NapCat / SnowLuma 更新时可走认证额度"
            >
                {!draft ? (
                    <p className="text-[13px] text-text-tertiary">正在加载设置…</p>
                ) : (
                    <FieldRow
                        label="Personal Access Token"
                        description="仅需 public_repo 或无权限 classic token；保存后写入系统密钥库"
                        isLast
                    >
                        <div className="flex items-center gap-1.5">
                            <TextField
                                className="w-72"
                                type={revealPat ? 'text' : 'password'}
                                placeholder="ghp_..."
                                autoComplete="off"
                                value={draft.githubPat}
                                onValueChange={(v) => patchDraft({ githubPat: v })}
                            />
                            <button
                                type="button"
                                onClick={() => setRevealPat((r) => !r)}
                                className="flex h-8 w-8 items-center justify-center rounded-sm text-text-tertiary transition-colors hover:bg-inset hover:text-text"
                                aria-label={revealPat ? '隐藏 token' : '显示 token'}
                            >
                                {revealPat ? (
                                    <ActionMotionIcon icon={EyeOff} size={15} />
                                ) : (
                                    <ActionMotionIcon icon={Eye} size={15} />
                                )}
                            </button>
                        </div>
                    </FieldRow>
                )}
            </SettingsSection>
        </SettingsTabSections>
    );
}