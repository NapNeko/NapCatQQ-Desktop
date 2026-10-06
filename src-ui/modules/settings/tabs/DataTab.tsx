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

export function DataTab({ dataRoot, onOpenDataDir, isOpeningDir, draft, patchDraft }: Props) {
    const [revealPat, setRevealPat] = useState(false);
    const [migrateOpen, setMigrateOpen] = useState(false);
    const {
        exportConfig,
        openImportWizard,
        importOpen,
        setImportOpen,
        onImported,
        isExporting: isExportingCfg,
        canRetryPreferences,
        retryPreferences,
        pendingFrameworks,
        frameworkPendingError,
        retryFrameworks,
        isRestoringFrameworks,
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
                            迁移
                        </Button>
                    </div>
                </FieldRow>
            </SettingsSection>

            <SettingsSection
                title="配置备份"
                description="备份桌面端与框架配置、界面偏好。框架与插件程序、系统密钥库、SSH 私钥和聊天记录保持独立；换盘请使用数据根目录迁移。"
            >
                <FieldRow
                    label="导出当前配置"
                    description="包含应用设置、Bot、远端档案、实例与关联、框架核心与模型设置、插件配置、麦麦自定义提示词、聊天设置、API 调试工作区与收藏、SnowLuma 设置和界面偏好。"
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
                    isLast={
                        !canRetryPreferences && !pendingFrameworks.length && !frameworkPendingError
                    }
                >
                    <Button variant="secondary" size="sm" onClick={openImportWizard}>
                        打开导入向导
                    </Button>
                </FieldRow>
                {canRetryPreferences && (
                    <FieldRow
                        label="界面偏好待恢复"
                        description="配置文件已经导入，修复浏览器存储问题后可单独重试界面与终端偏好。"
                        isLast={!pendingFrameworks.length && !frameworkPendingError}
                    >
                        <Button variant="secondary" size="sm" onClick={retryPreferences}>
                            重试恢复界面偏好
                        </Button>
                    </FieldRow>
                )}
                {pendingFrameworks.length > 0 && (
                    <FieldRow
                        label="框架配置待恢复"
                        description={`已保留恢复副本：${pendingFrameworks.join('；')}。完成框架安装、停止实例并连接远端后，可单独重试。`}
                        isLast
                    >
                        <Button
                            variant="secondary"
                            size="sm"
                            onClick={retryFrameworks}
                            disabled={isRestoringFrameworks}
                        >
                            {isRestoringFrameworks ? '恢复中…' : '重试恢复框架配置'}
                        </Button>
                    </FieldRow>
                )}
                {frameworkPendingError && (
                    <p className="text-[12px] text-warning">
                        待恢复框架配置读取失败：{frameworkPendingError.message}
                    </p>
                )}
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

            <SettingsSection title="GitHub" description="可选；填写后组件页检查更新走认证额度">
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
