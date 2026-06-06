// 配置导入导出 hook。

import { useMutation } from '@tanstack/react-query';
import { useState } from 'react';
import { configTransferService } from '../../core/services/config-transfer.service';
import { pushInfoBar } from '../ui/globalInfoBarStore';

export function useConfigTransfer() {
    const [importOpen, setImportOpen] = useState(false);

    const exportMutation = useMutation({
        mutationFn: configTransferService.export,
        onSuccess: (result) => {
            if (!result) return;
            const files = result.files.length ? result.files.join('、') : '无可导出项';
            pushInfoBar({
                key: 'config-export',
                tone: 'success',
                title: '配置已导出',
                content: `${files} → ${result.export_path}`,
            });
        },
        onError: (err: Error) => {
            pushInfoBar({
                key: 'config-export',
                tone: 'danger',
                title: '导出失败',
                content: err.message || String(err),
            });
        },
    });

    const handleImported = (result: { files: string[]; skipped: string[] }) => {
        const skippedNote = result.skipped.length
            ? `；未覆盖：${result.skipped.join('、')}`
            : '';
        pushInfoBar({
            key: 'config-import',
            tone: 'success',
            title: '配置已导入，重启后生效',
            content: `已导入：${result.files.join('、')}${skippedNote}。密钥不随包导入，需重新配置`,
        });
        setImportOpen(false);
    };

    return {
        exportConfig: () => exportMutation.mutate(),
        openImportWizard: () => setImportOpen(true),
        importOpen,
        setImportOpen,
        onImported: handleImported,
        isExporting: exportMutation.isPending,
        isImporting: false,
    };
}