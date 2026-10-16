// Desktop 网络设置的诊断与内置下载线路。

import type {
    DownloadMirror,
    NetworkProbeResult,
    NetworkSettings,
} from '../domain/settings/network';
import { invoke, isTauri } from '../ipc/transport';

export const networkService = {
    listDownloadMirrors: async (): Promise<DownloadMirror[]> => {
        if (isTauri) return invoke<DownloadMirror[]>('list_download_mirrors');
        return [
            {
                id: 'github-gh-proxy',
                label: 'gh-proxy.com',
                url: 'https://gh-proxy.com',
                kind: 'github',
            },
            {
                id: 'node-npmmirror',
                label: 'npmmirror',
                url: 'https://registry.npmmirror.com/-/binary/node',
                kind: 'node',
            },
        ];
    },
    test: async (
        settings: NetworkSettings,
        proxyPassword: string,
    ): Promise<NetworkProbeResult[]> => {
        if (!isTauri) throw new Error('浏览器预览不执行真实网络测试，请在桌面端测试连接');
        return invoke<NetworkProbeResult[]>('test_network_settings', { settings, proxyPassword });
    },
};
