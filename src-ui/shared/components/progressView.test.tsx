import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, render, screen } from '@testing-library/react';
import { initialActionProgress } from '../../core/domain/components/progress';
import { ProgressLine } from './progressView';

vi.mock('../ui/motion', () => ({ MotionIcon: () => <span aria-hidden /> }));
vi.mock('../ui', () => ({ Progress: () => null }));

afterEach(cleanup);

describe('下载来源和切换原因', () => {
    it('下载时用一行展示来源与阶段，右侧只补百分比', () => {
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
        const line = screen.getByText(message);
        expect(line.closest('[title]')).toHaveAttribute('title', expect.stringContaining(message));
        expect(screen.getByText('25%')).toBeInTheDocument();
        // 完整信息只占一行，不再额外渲染一行相同的 message。
        expect(screen.getAllByText(/下载中/)).toHaveLength(1);
        expect(screen.queryByText('2.0 MB/s')).not.toBeInTheDocument();
        expect(screen.queryByText(/1\.0 MB \/ 4\.0 MB/)).not.toBeInTheDocument();
    });

    it.each(['streaming', 'resuming'] as const)(
        '%s 的普通阶段文案仍显示结构化字节和速度',
        (downloadStage) => {
            render(
                <ProgressLine
                    progress={{
                        ...initialActionProgress,
                        status: 'running',
                        downloadStage,
                        percent: 25,
                        speedBps: 2 * 1024 * 1024,
                        downloadedBytes: 1024 * 1024,
                        totalBytes: 4 * 1024 * 1024,
                        message: '正在安装 uv',
                    }}
                />,
            );
            expect(screen.getByText('正在安装 uv · 1.0 MB / 4.0 MB')).toBeInTheDocument();
            expect(screen.getByText('2.0 MB/s')).toBeInTheDocument();
            expect(screen.getByText('25%')).toBeInTheDocument();
        },
    );

    it('文案只有速度时仍补字节，且不重复显示速度', () => {
        render(
            <ProgressLine
                progress={{
                    ...initialActionProgress,
                    status: 'running',
                    downloadStage: 'streaming',
                    speedBps: 2 * 1024 * 1024,
                    downloadedBytes: 1024 * 1024,
                    totalBytes: 4 * 1024 * 1024,
                    message: '下载中 · 2.00 MB/s',
                }}
            />,
        );
        expect(screen.getByText('下载中 · 2.00 MB/s · 1.0 MB / 4.0 MB')).toBeInTheDocument();
        expect(screen.queryByText('2.0 MB/s')).not.toBeInTheDocument();
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
        const line = screen.getByText(message);
        expect(line.closest('[title]')).toHaveAttribute(
            'title',
            expect.stringContaining('切换镜像'),
        );
    });

    it('失败态只给一句结论，不引用别处的提示', () => {
        render(
            <ProgressLine
                progress={{
                    ...initialActionProgress,
                    status: 'failed',
                    message: '失败',
                }}
            />,
        );
        expect(screen.getByText('任务失败')).toBeInTheDocument();
        expect(screen.queryByText(/详见顶部提示/)).not.toBeInTheDocument();
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
