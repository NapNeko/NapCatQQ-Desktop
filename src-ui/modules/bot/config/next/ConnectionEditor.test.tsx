import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import gsap from 'gsap';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { createDefaultConnection } from '../../../../core/domain/bot/connections';
import { preferencesStore } from '../../../../core/domain/settings/preferencesStore';
import { ConnectionEditor } from './ConnectionEditor';

function finishMotion() {
    act(() => {
        gsap.globalTimeline.getChildren().forEach((animation) => animation.totalProgress(1));
    });
}

afterEach(cleanup);

describe('连接编辑器底座字段切换', () => {
    it('切到 SnowLuma 时保留退出内容，并立即禁止隐藏字段参与交互', () => {
        const initialData = createDefaultConnection('httpServer');
        const props = {
            kind: 'httpServer' as const,
            initialData,
            existingNames: [],
            onSave: vi.fn(),
            onCancel: vi.fn(),
        };
        const { rerender } = render(<ConnectionEditor {...props} backendType="napcat" />);
        finishMotion();
        const cors = screen.getByRole('checkbox', { name: '允许跨域请求 (CORS)' });

        rerender(<ConnectionEditor {...props} backendType="snowluma" />);

        expect(cors).toBeInTheDocument();
        expect(cors.closest('[inert]')).not.toBeNull();
        expect(cors.closest('[aria-hidden="true"]')).not.toBeNull();
        finishMotion();
        expect(screen.queryByText('允许跨域请求 (CORS)')).not.toBeInTheDocument();
    });

    it('快速切走再切回 SnowLuma 时保留已编辑的超时值', () => {
        const initialData = createDefaultConnection('httpClient');
        const props = {
            kind: 'httpClient' as const,
            initialData,
            existingNames: [],
            onSave: vi.fn(),
            onCancel: vi.fn(),
        };
        const { rerender } = render(<ConnectionEditor {...props} backendType="snowluma" />);
        finishMotion();
        fireEvent.change(screen.getByLabelText('请求超时 (ms)'), { target: { value: '8500' } });

        rerender(<ConnectionEditor {...props} backendType="napcat" />);
        rerender(<ConnectionEditor {...props} backendType="snowluma" />);
        finishMotion();

        const timeout = screen.getByLabelText('请求超时 (ms)');
        expect(timeout).toHaveValue(8500);
        expect(timeout.closest('[inert]')).toBeNull();
    });
});

describe('连接监听的主机外部访问', () => {
    beforeEach(() => preferencesStore.setMotionEnabled(false));
    afterEach(() => {
        cleanup();
        preferencesStore.reset();
    });

    it.each(['httpServer', 'httpSseServer', 'websocketServer'] as const)(
        '%s 默认监听回环地址，确认开放后才切换到通配地址',
        async (kind) => {
            const onSave = vi.fn();
            render(
                <ConnectionEditor
                    kind={kind}
                    initialData={createDefaultConnection(kind)}
                    existingNames={[]}
                    backendType="napcat"
                    onSave={onSave}
                    onCancel={vi.fn()}
                />,
            );
            const host = screen.getByLabelText(/^监听 IP \(Host\)/);
            const externalAccess = screen.getByRole('switch', { name: /允许主机外部访问/ });
            expect(host).toHaveValue('127.0.0.1');
            expect(externalAccess).not.toBeChecked();

            fireEvent.click(externalAccess);
            expect(
                await screen.findByRole('dialog', { name: '开放主机外部访问' }),
            ).toBeInTheDocument();
            expect(host).toHaveValue('127.0.0.1');
            expect(onSave).not.toHaveBeenCalled();
            fireEvent.click(screen.getByRole('button', { name: '取消', exact: true }));
            expect(host).toHaveValue('127.0.0.1');

            fireEvent.click(externalAccess);
            fireEvent.click(await screen.findByRole('button', { name: '确认开放', exact: true }));
            expect(host).toHaveValue('0.0.0.0');
            expect(externalAccess).toBeChecked();
            expect(onSave).not.toHaveBeenCalled();
            fireEvent.click(screen.getByRole('button', { name: '保存连接' }));
            expect(onSave).toHaveBeenLastCalledWith(expect.objectContaining({ host: '0.0.0.0' }));

            fireEvent.click(externalAccess);
            expect(host).toHaveValue('127.0.0.1');
            expect(externalAccess).not.toBeChecked();
            fireEvent.click(screen.getByRole('button', { name: '保存连接' }));
            expect(onSave).toHaveBeenLastCalledWith(expect.objectContaining({ host: '127.0.0.1' }));
        },
    );

    it('Docker 监听的外部发布由容器页控制，连接编辑器不展示开放开关', () => {
        render(
            <ConnectionEditor
                kind="httpServer"
                initialData={createDefaultConnection('httpServer')}
                existingNames={[]}
                backendType="napcat"
                dockerDeployment
                onSave={vi.fn()}
                onCancel={vi.fn()}
            />,
        );
        expect(screen.queryByRole('switch', { name: /允许主机外部访问/ })).not.toBeInTheDocument();
        expect(screen.getByText(/容器.*外部|外部.*容器/)).toBeInTheDocument();
    });

    it('直接输入外部监听 IP 后保存也必须确认，取消不会写入连接', async () => {
        const onSave = vi.fn();
        render(
            <ConnectionEditor
                kind="httpServer"
                initialData={createDefaultConnection('httpServer')}
                existingNames={[]}
                backendType="napcat"
                onSave={onSave}
                onCancel={vi.fn()}
            />,
        );
        fireEvent.change(screen.getByLabelText(/^监听 IP \(Host\)/), {
            target: { value: ' 192.168.1.10 ' },
        });
        fireEvent.click(screen.getByRole('button', { name: '保存连接' }));
        expect(await screen.findByRole('dialog', { name: '开放主机外部访问' })).toBeInTheDocument();
        expect(onSave).not.toHaveBeenCalled();
        fireEvent.click(screen.getByRole('button', { name: '取消', exact: true }));
        expect(onSave).not.toHaveBeenCalled();

        fireEvent.click(screen.getByRole('button', { name: '保存连接' }));
        fireEvent.click(await screen.findByRole('button', { name: '确认开放', exact: true }));
        expect(onSave).toHaveBeenCalledTimes(1);
        expect(onSave).toHaveBeenLastCalledWith(expect.objectContaining({ host: '192.168.1.10' }));
    });
});
