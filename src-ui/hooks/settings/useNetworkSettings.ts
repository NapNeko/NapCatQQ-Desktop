// 使用未保存的网络草稿诊断连接，忽略编辑后的旧请求结果。

import { useEffect, useRef, useState } from 'react';
import type {
    DownloadMirror,
    NetworkProbeResult,
    NetworkSettings,
} from '../../core/domain/settings/network';
import { validateNetworkSettings } from '../../core/domain/settings/network';
import { networkService } from '../../core/services/network.service';

export function useNetworkSettings(settings: NetworkSettings | null, proxyPassword: string) {
    const [mirrors, setMirrors] = useState<DownloadMirror[]>([]);
    const [mirrorError, setMirrorError] = useState<string | null>(null);
    const [busy, setBusy] = useState(false);
    const [outcome, setOutcome] = useState<{
        revision: number;
        results: NetworkProbeResult[];
        error: string | null;
    } | null>(null);
    const fingerprint = JSON.stringify([settings, proxyPassword]);
    const latest = useRef({ fingerprint, revision: 0 });
    if (latest.current.fingerprint !== fingerprint) {
        latest.current = { fingerprint, revision: latest.current.revision + 1 };
    }
    const running = useRef(false);
    const mounted = useRef(false);

    useEffect(() => {
        mounted.current = true;
        let disposed = false;
        void networkService.listDownloadMirrors().then(
            (items) => {
                if (!disposed) setMirrors(items);
            },
            (error: unknown) => {
                if (!disposed)
                    setMirrorError(error instanceof Error ? error.message : String(error));
            },
        );
        return () => {
            disposed = true;
            mounted.current = false;
            latest.current.revision += 1;
        };
    }, []);

    const test = async () => {
        if (!settings || running.current) return;
        const revision = latest.current.revision;
        const validationError = Object.values(validateNetworkSettings(settings, proxyPassword))[0];
        if (validationError) {
            setOutcome({ revision, results: [], error: validationError });
            return;
        }
        running.current = true;
        setBusy(true);
        setOutcome(null);
        try {
            const results = await networkService.test({ ...settings }, proxyPassword);
            if (mounted.current && latest.current.revision === revision) {
                setOutcome({ revision, results, error: null });
            }
        } catch (error) {
            if (mounted.current && latest.current.revision === revision) {
                setOutcome({
                    revision,
                    results: [],
                    error: error instanceof Error ? error.message : String(error),
                });
            }
        } finally {
            running.current = false;
            if (mounted.current) setBusy(false);
        }
    };
    const current = outcome?.revision === latest.current.revision ? outcome : null;
    return {
        mirrors,
        mirrorError,
        busy,
        test,
        results: current?.results ?? [],
        error: current?.error ?? null,
    };
}
