import { describe, expect, it } from 'vitest';
import type { ContainerInfo, ContainerState, DockerStatus, ImageInfo } from '../../ipc/types';
import {
    compactPorts,
    containerMatchesFlavor,
    containerStateBadge,
    dockerStatusSummary,
    imageDisplayRef,
    imageRemoveConflictNeedsForce,
    imageRemoveFailureHint,
    imageRemoveRef,
    isDanglingImage,
    isManagedImage,
    isManagedImageRef,
} from './status';

const status = (over: Partial<DockerStatus> = {}): DockerStatus => ({
    installed: true,
    version: '27.3.1',
    composeAvailable: true,
    daemonRunning: true,
    ...over,
});

const image = (over: Partial<ImageInfo> = {}): ImageInfo => ({
    id: 'a1b2c3d4e5f6',
    repository: 'mlikiowa/napcat-docker',
    tag: 'latest',
    size: '1.2GB',
    createdSince: '2 weeks ago',
    ...over,
});

const container = (over: Partial<ContainerInfo> = {}): ContainerInfo => ({
    id: '1234567890ab',
    name: 'napcat-main',
    image: 'mlikiowa/napcat-docker:latest',
    state: 'running',
    status: 'Up 3 hours',
    ports: [],
    ...over,
});

describe('containerStateBadge', () => {
    it('每种已知状态给语义色与中文标签', () => {
        expect(containerStateBadge('running')).toEqual({ label: '运行中', tone: 'success' });
        expect(containerStateBadge('exited')).toEqual({ label: '已停止', tone: 'neutral' });
        expect(containerStateBadge('created')).toEqual({ label: '已创建', tone: 'neutral' });
        expect(containerStateBadge('restarting')).toEqual({ label: '重启中', tone: 'warning' });
        expect(containerStateBadge('paused')).toEqual({ label: '已暂停', tone: 'warning' });
    });

    it('未知状态兜底 danger 而不是抛', () => {
        expect(containerStateBadge('other')).toEqual({ label: '未知', tone: 'danger' });
        expect(containerStateBadge('removing' as unknown as ContainerState).tone).toBe('danger');
    });
});

describe('dockerStatusSummary', () => {
    it('未安装时其余字段不参与判断', () => {
        expect(dockerStatusSummary(status({ installed: false }))).toEqual({
            ready: false,
            label: '未安装 Docker',
        });
    });

    it('装了但 daemon 没起来：ready=false 并带上版本号', () => {
        const s = dockerStatusSummary(status({ daemonRunning: false }));
        expect(s.ready).toBe(false);
        expect(s.label).toContain('27.3.1');
        expect(s.label).toContain('守护进程未运行');
    });

    it('缺 compose 插件单独报告', () => {
        const s = dockerStatusSummary(status({ composeAvailable: false }));
        expect(s.ready).toBe(false);
        expect(s.label).toContain('compose');
    });

    it('全就绪时 ready=true', () => {
        expect(dockerStatusSummary(status())).toEqual({ ready: true, label: 'Docker 27.3.1 就绪' });
    });
});

describe('isManagedImage / isManagedImageRef / containerMatchesFlavor', () => {
    it('本工程镜像按 napcat-docker / snowluma 子串识别', () => {
        expect(isManagedImage('mlikiowa/napcat-docker:latest')).toBe(true);
        expect(isManagedImage('ghcr.io/fuyuan/snowluma:1.2')).toBe(true);
        expect(isManagedImage('nginx:alpine')).toBe(false);
        expect(isManagedImageRef('mlikiowa/napcat-docker')).toBe(true);
        expect(isManagedImageRef('redis')).toBe(false);
    });

    it('flavor 匹配只看镜像名，容器名骗不过去', () => {
        const napcat = container();
        expect(containerMatchesFlavor(napcat, 'napcat')).toBe(true);
        expect(containerMatchesFlavor(napcat, 'snowluma')).toBe(false);
        const renamed = container({ name: 'napcat-fake', image: 'slimaize/snowluma:dev' });
        expect(containerMatchesFlavor(renamed, 'snowluma')).toBe(true);
        expect(containerMatchesFlavor(container({ image: 'postgres:16' }), 'napcat')).toBe(false);
    });
});

describe('imageDisplayRef / isDanglingImage / imageRemoveRef', () => {
    it('正常 repo:tag 直接拼', () => {
        expect(imageDisplayRef(image())).toBe('mlikiowa/napcat-docker:latest');
        expect(imageRemoveRef(image())).toBe('mlikiowa/napcat-docker:latest');
        expect(isDanglingImage(image())).toBe(false);
    });

    it('repo 有、tag 是 <none> 时只显示 repo', () => {
        const i = image({ tag: '<none>' });
        expect(imageDisplayRef(i)).toBe('mlikiowa/napcat-docker');
        expect(isDanglingImage(i)).toBe(false);
    });

    it('悬空镜像显示短 id 并被判定 dangling', () => {
        const dangling = image({ repository: '<none>', tag: '<none>' });
        expect(imageDisplayRef(dangling)).toBe('a1b2c3d4e5f6');
        expect(imageRemoveRef(dangling)).toBe('a1b2c3d4e5f6');
        expect(isDanglingImage(dangling)).toBe(true);
        const empty = image({ repository: '  ', tag: '' });
        expect(isDanglingImage(empty)).toBe(true);
        expect(imageDisplayRef(empty)).toBe('a1b2c3d4e5f6');
    });
});

describe('imageRemoveConflictNeedsForce / imageRemoveFailureHint', () => {
    it('识别两类「仍被容器引用」报错，大小写无关', () => {
        expect(
            imageRemoveConflictNeedsForce(
                'Error response from daemon: conflict: unable to delete a1b2c3 (must be forced) - container is running',
            ),
        ).toBe(true);
        expect(imageRemoveConflictNeedsForce('CONFLICT: UNABLE TO DELETE x')).toBe(true);
    });

    it('普通删除失败不算冲突', () => {
        expect(imageRemoveConflictNeedsForce('no such image: bogus')).toBe(false);
        expect(imageRemoveConflictNeedsForce('')).toBe(false);
    });

    it('冲突给强制删除指引，其它失败返回空串', () => {
        const hint = imageRemoveFailureHint('conflict: unable to delete a1b2c3 (must be forced)');
        expect(hint).toContain('强制删除');
        expect(imageRemoveFailureHint('permission denied')).toBe('');
    });
});

describe('compactPorts', () => {
    it('IPv4 / IPv6 双记录塌成一条，保留首次出现顺序', () => {
        expect(
            compactPorts([
                '0.0.0.0:3000->3000/tcp',
                '[::]:3000->3000/tcp',
                '0.0.0.0:6099->6099/tcp',
                '[::]:6099->6099/tcp',
            ]),
        ).toEqual(['3000->3000/tcp', '6099->6099/tcp']);
    });

    it('不同宿主端口不互相吞掉', () => {
        expect(compactPorts(['0.0.0.0:8080->80/tcp', '127.0.0.1:8081->80/tcp'])).toEqual([
            '8080->80/tcp',
            '8081->80/tcp',
        ]);
    });

    it('udp 与 tcp 同端口号是两条', () => {
        expect(compactPorts(['0.0.0.0:53->53/tcp', '0.0.0.0:53->53/udp'])).toEqual([
            '53->53/tcp',
            '53->53/udp',
        ]);
    });

    it('无绑定前缀 / 空列表原样通过', () => {
        expect(compactPorts(['3000->3000/tcp'])).toEqual(['3000->3000/tcp']);
        expect(compactPorts([])).toEqual([]);
    });

    it('无数字的异常文案去不了前缀时按原文保留，不丢条目', () => {
        expect(compactPorts(['weird entry'])).toEqual(['weird entry']);
    });
});
