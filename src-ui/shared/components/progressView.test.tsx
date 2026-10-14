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
});
