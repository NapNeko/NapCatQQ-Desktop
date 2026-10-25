import { useState } from 'react';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { BotConfig } from '../../../../core/ipc/generated/domain/BotConfig';
import type { BotDockerConfig } from '../../../../core/ipc/generated/domain/BotDockerConfig';
import { createDefaultBotConfig } from '../../../../core/domain/bot/config-defaults';
import { defaultBotDockerConfig } from '../../../../core/domain/bot/docker-config';
import { preferencesStore } from '../../../../core/domain/settings/preferencesStore';
import { DockerTab } from './DockerTab';

beforeEach(() => preferencesStore.setMotionEnabled(false));
afterEach(() => {
    cleanup();
    preferencesStore.reset();
});

function dockerBot(): BotConfig {
    const config = createDefaultBotConfig();
    config.bot.QQID = 10001;
    config.bot.name = 'Docker Bot';
    config.bot.deploymentType = 'docker';
    config.bot.runtime_target = 'test-server';
    return config;
}

function Harness({
    initial,
    onChange,
}: {
    initial: BotConfig;
    onChange: (settings: BotDockerConfig) => void;
}) {
    const [config, setConfig] = useState(initial);
    return (
        <DockerTab
            config={config}
            onChange={(settings) => {
                onChange(settings);
                setConfig((previous) => ({ ...previous, docker: settings }));
            }}
        />
    );
}

describe('Bot 容器配置', () => {
    it('开放主机外部访问须确认风险，取消保留本机访问，关闭无需再次确认', async () => {
        const config = dockerBot();
        config.docker = {
            ...defaultBotDockerConfig(),
            memoryMb: 1024,
            environment: { TZ: 'Asia/Shanghai' },
        };
        const onChange = vi.fn();
        render(<Harness initial={config} onChange={onChange} />);

        const externalAccess = screen.getByRole('switch', { name: /允许主机外部访问/ });
        expect(externalAccess).not.toBeChecked();
        fireEvent.click(externalAccess);
        expect(await screen.findByRole('dialog', { name: '开放主机外部访问' })).toBeInTheDocument();
        expect(onChange).not.toHaveBeenCalled();
        expect(externalAccess).not.toBeChecked();
        fireEvent.click(screen.getByRole('button', { name: '取消', exact: true }));
        expect(externalAccess).not.toBeChecked();
        expect(onChange).not.toHaveBeenCalled();

        fireEvent.click(externalAccess);
        fireEvent.click(await screen.findByRole('button', { name: '确认开放', exact: true }));
        expect(externalAccess).toBeChecked();
        expect(onChange).toHaveBeenLastCalledWith({
            ...config.docker,
            publishOnAllInterfaces: true,
        });

        fireEvent.click(externalAccess);
        expect(externalAccess).not.toBeChecked();
        expect(screen.queryByRole('dialog', { name: '开放主机外部访问' })).not.toBeInTheDocument();
        expect(onChange).toHaveBeenLastCalledWith(config.docker);
    });

    it('编辑宿主机端口后可恢复自动，同时保留内存和环境变量', () => {
        const config = dockerBot();
        config.docker = {
            ...defaultBotDockerConfig(),
            memoryMb: 1024,
            environment: { TZ: 'Asia/Shanghai' },
        };
        const onChange = vi.fn();
        render(<Harness initial={config} onChange={onChange} />);

        const port = screen.getByLabelText('容器端口 3000 的宿主机端口');
        expect(port).toHaveValue(3001);
        fireEvent.change(port, { target: { value: '13000' } });
        expect(port).toHaveValue(13000);
        fireEvent.click(screen.getByRole('button', { name: '恢复端口 3000 的自动映射' }));

        expect(port).toHaveValue(3001);
        expect(onChange).toHaveBeenLastCalledWith({
            ...defaultBotDockerConfig(),
            memoryMb: 1024,
            environment: { TZ: 'Asia/Shanghai' },
        });
        expect(screen.getByLabelText('内存上限（MiB）')).toHaveValue(1024);
        expect(screen.getByLabelText('TZ')).toHaveValue('Asia/Shanghai');
        expect(screen.getByText(/运行中的 Bot.*重建容器/)).toBeInTheDocument();
    });

    it('添加额外映射时拒绝与现有监听重复，新增项可以单独删除', () => {
        const onChange = vi.fn();
        render(<Harness initial={dockerBot()} onChange={onChange} />);

        fireEvent.change(screen.getByLabelText('额外容器端口'), { target: { value: '3000' } });
        fireEvent.change(screen.getByLabelText('额外宿主机端口'), { target: { value: '13000' } });
        fireEvent.click(screen.getByRole('button', { name: '添加映射' }));
        expect(screen.getByRole('alert')).toHaveTextContent('此容器端口已在上方列出');
        expect(onChange).not.toHaveBeenCalled();

        fireEvent.change(screen.getByLabelText('额外容器端口'), { target: { value: '8080' } });
        fireEvent.click(screen.getByRole('button', { name: '添加映射' }));
        expect(screen.getByLabelText('容器端口 8080 的宿主机端口')).toHaveValue(13000);
        expect(screen.queryByRole('alert')).not.toBeInTheDocument();
        fireEvent.click(screen.getByRole('button', { name: '删除端口 8080 的映射' }));
        expect(screen.queryByLabelText('容器端口 8080 的宿主机端口')).not.toBeInTheDocument();
        expect(screen.getByLabelText('容器端口 6099 的宿主机端口')).toHaveValue(6100);
    });

    it('拒绝添加受管环境变量，普通变量可编辑删除，内存留空取消上限', () => {
        const onChange = vi.fn();
        render(<Harness initial={dockerBot()} onChange={onChange} />);

        fireEvent.change(screen.getByLabelText('新变量名'), { target: { value: 'WEBUI_TOKEN' } });
        fireEvent.change(screen.getByLabelText('新变量值'), { target: { value: 'custom' } });
        fireEvent.click(screen.getByRole('button', { name: '添加变量' }));
        expect(screen.getByRole('alert')).toHaveTextContent('此变量由 Desktop 管理');
        expect(onChange).not.toHaveBeenCalled();

        fireEvent.change(screen.getByLabelText('新变量名'), { target: { value: 'TZ' } });
        fireEvent.change(screen.getByLabelText('新变量值'), { target: { value: 'Asia/Shanghai' } });
        fireEvent.click(screen.getByRole('button', { name: '添加变量' }));
        expect(screen.queryByRole('alert')).not.toBeInTheDocument();
        fireEvent.change(screen.getByLabelText('TZ'), { target: { value: 'UTC' } });
        expect(onChange).toHaveBeenLastCalledWith(
            expect.objectContaining({ environment: { TZ: 'UTC' } }),
        );

        const memory = screen.getByLabelText('内存上限（MiB）');
        fireEvent.change(memory, { target: { value: '512' } });
        expect(onChange).toHaveBeenLastCalledWith(
            expect.objectContaining({ memoryMb: 512, environment: { TZ: 'UTC' } }),
        );
        fireEvent.change(memory, { target: { value: '' } });
        expect(onChange).toHaveBeenLastCalledWith(
            expect.objectContaining({ memoryMb: undefined, environment: { TZ: 'UTC' } }),
        );
        fireEvent.click(screen.getByRole('button', { name: '删除环境变量 TZ' }));
        expect(screen.queryByLabelText('TZ')).not.toBeInTheDocument();
        expect(onChange).toHaveBeenLastCalledWith(expect.objectContaining({ environment: {} }));
    });
});
