// 浏览器预览（非 Tauri）用的假主机注册表。纯数据，无 IPC 依赖。
//
// 真后端的 host_id 约定是 "local" / "remote:<id>"。这里模拟一台本机
// + 两台远端，让前端的多主机 UI 能在浏览器预览。

export interface MockHost {
    host_id: string;
    display_name: string;
    os: 'windows' | 'linux' | 'mac_os';
    locality: 'local' | 'remote';
}

export const mockHosts: MockHost[] = [
    { host_id: 'local', display_name: '本机', os: 'windows', locality: 'local' },
    {
        host_id: 'remote:production',
        display_name: 'remote · production',
        os: 'linux',
        locality: 'remote',
    },
    { host_id: 'remote:dev', display_name: 'remote · dev', os: 'linux', locality: 'remote' },
];
