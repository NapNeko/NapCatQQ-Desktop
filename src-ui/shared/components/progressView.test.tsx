import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, render, screen } from '@testing-library/react';
import { initialActionProgress } from '../../core/domain/components/progress';
import { ProgressLine } from './progressView';

vi.mock('../ui/motion', () => ({ MotionIcon: () => <span aria-hidden /> }));
vi.mock('../ui', () => ({ Progress: () => null }));

afterEach(cleanup);

describe('下载来源和切换原因', () => {
    it('下载时显示当前镜像，同时保留速度与进度数字', () => {
        const message = '镜像 gh-proxy.com · 下载中 · 1.00 MB / 4.00 MB · 2.00 MB/s';
        render(
            <ProgressLine
                progress={{
                    ...initialActionProgress,
                    status: 'running',
                    downloadStage: 'streaming',
                    percent: 25,
                    speedBps: 2 * 1024 * 1024,
                    downloadedBytes: 1024 * 1024,
                    totalBytes: 4 * 1024 * 1024,
                    message,
                }}
            />,
        );
        expect(screen.getByText(message)).toHaveAttribute('title', message);
        expect(screen.getByText('2.0 MB/s')).toBeInTheDocument();
        expect(screen.getByText('25%')).toBeInTheDocument();
    });

    it('切换时展示低速原因，避免只显示通用阶段标签', () => {
        const message = '镜像 ghfast.top · 下载速度持续偏低，正在切换镜像';
        render(
            <ProgressLine
                progress={{
                    ...initialActionProgress,
                    status: 'running',
                    downloadStage: 'switching_mirror',
                    message,
                }}
            />,
        );
        expect(screen.getByText(message)).toBeInTheDocument();
        expect(screen.getByText('切换镜像')).toBeInTheDocument();
    });

    it('卡片摘要保留阶段、速度和百分比，将完整来源与字节信息放入可访问提示', () => {
        const message = '镜像 ghfast.top · 下载中，重试后已恢复连接';
        render(
            <ProgressLine
                compact
                progress={{
                    ...initialActionProgress,
                    status: 'running',
                    downloadStage: 'streaming',
                    percent: 25,
                    speedBps: 2 * 1024 * 1024,
                    downloadedBytes: 1024 * 1024,
                    totalBytes: 4 * 1024 * 1024,
                    message,
                }}
            />,
        );

        const summary = screen.getByRole('group', { name: new RegExp(message) });
        expect(summary).toHaveAttribute('title', expect.stringContaining('1.0 MB / 4.0 MB'));
        expect(summary).toHaveAttribute('title', expect.stringContaining(message));
        expect(screen.getByText('下载中')).toBeInTheDocument();
        expect(screen.getByText('2.0 MB/s')).toBeInTheDocument();
        expect(screen.getByText('25%')).toBeInTheDocument();
        expect(screen.queryByText(message)).not.toBeInTheDocument();
    });
});
