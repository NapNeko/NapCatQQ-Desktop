// Bootstrap 状态 + 全局工具动作 hook。

import { useQuery } from '@tanstack/react-query';
import { useState } from 'react';
import { bootstrapService } from '../../core/services/bootstrap.service';

export function useBootstrap() {
    const query = useQuery({
        queryKey: ['bootstrapStatus'],
        queryFn: bootstrapService.getStatus,
    });

    const [isOpeningDir, setIsOpeningDir] = useState(false);
    const [isExporting, setIsExporting] = useState(false);

    const openDataDir = async (): Promise<string> => {
        setIsOpeningDir(true);
        try {
            return await bootstrapService.openDataDir();
        } finally {
            setIsOpeningDir(false);
        }
    };

    const exportMigrationReport = async (): Promise<string> => {
        setIsExporting(true);
        try {
            return await bootstrapService.exportMigrationReport();
        } finally {
            setIsExporting(false);
        }
    };

    return {
        bootstrap: query.data,
        isLoading: query.isLoading,
        error: query.error,
        openDataDir,
        exportMigrationReport,
        isOpeningDir,
        isExporting,
    };
}
