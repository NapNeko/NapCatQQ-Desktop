import type { ButtonHTMLAttributes, HTMLAttributes, ReactNode } from 'react';
import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import type { ConfigDrift } from '../../../core/ipc/generated/ConfigDrift';
import { ConfigDriftDialog } from './ConfigDriftDialog';

vi.mock('../../../shared/ui/motion', () => ({
    ActionMotionIcon: () => <span />,
    EMPHASIS_MOTION: {},
    LIVE_MOTION: {},
    infoToneMotion: () => ({}),
}));

vi.mock('../../../shared/ui', () => ({
    Dialog: ({ children }: { children: ReactNode }) => <>{children}</>,
    DialogContent: ({
        children,
        dismissOnOutsideClick: _dismiss,
        ...props
    }: HTMLAttributes<HTMLDivElement> & { dismissOnOutsideClick?: boolean }) => <div {...props}>{children}</div>,
    DialogHeader: ({ children, ...props }: HTMLAttributes<HTMLDivElement>) => <div {...props}>{children}</div>,
    DialogTitle: ({ children }: { children: ReactNode }) => <h2>{children}</h2>,
    DialogDescription: ({ children }: { children: ReactNode }) => <p>{children}</p>,
    DialogFooter: ({ children, ...props }: HTMLAttributes<HTMLDivElement>) => <div {...props}>{children}</div>,
    Button: ({ children, size: _size, variant: _variant, ...props }: ButtonHTMLAttributes<HTMLButtonElement> & { size?: unknown; variant?: unknown }) => (
        <button {...props}>{children}</button>
    ),
    Badge: ({ children }: { children: ReactNode }) => <span>{children}</span>,
    Switch: ({ checked, onCheckedChange }: { checked: boolean; onCheckedChange: (v: boolean) => void }) => (
        <button type="button" aria-pressed={checked} onClick={() => onCheckedChange(!checked)} />
    ),
}));

function manyConflicts(): ConfigDrift {
    const paths = [
        'musicSignUrl',
        'networks.httpServers.0.accessToken',
        'networks.httpServers.0.enabled',
        'networks.wsClients.0.enabled',
        'networks.wsServers.0.accessToken',
        'networks.httpClients.0.url',
    ];
    return {
        bot_id: '2295336188',
        backend_type: 'snowluma',
        added: [],
        modified: paths.map((path) => ({
            file: 'onebot_2295336188.json',
            path,
            internal: path.endsWith('enabled') ? true : '',
            external: path.endsWith('enabled') ? null : '',
        })),
    };
}

describe('ConfigDriftDialog', () => {
    it('lets the conflict list shrink and scroll when many items overflow the sheet', () => {
        render(
            <ConfigDriftDialog
                open
                drift={manyConflicts()}
                onConfirm={vi.fn()}
                onCancel={vi.fn()}
            />,
        );

        expect(screen.getByText('共 6 处配置差异')).toBeInTheDocument();
        expect(screen.getByText('musicSignUrl')).toBeInTheDocument();
        expect(screen.getByText('networks.httpClients.0.url')).toBeInTheDocument();
        expect(screen.getByRole('button', { name: '应用并启动 Bot' })).toBeInTheDocument();

        const list = screen.getByTestId('config-drift-conflict-list');
        expect(list.className.split(/\s+/)).toEqual(
            expect.arrayContaining(['min-h-0', 'flex-1', 'overflow-y-auto']),
        );
        expect(list.className.split(/\s+/)).not.toContain('scrollbar-hide');
    });

    it('uses save copy when resolving drift before write', () => {
        render(
            <ConfigDriftDialog
                open
                intent="save"
                drift={manyConflicts()}
                onConfirm={vi.fn()}
                onCancel={vi.fn()}
            />,
        );

        expect(screen.getByRole('button', { name: '应用并保存' })).toBeInTheDocument();
        expect(screen.getByRole('button', { name: '取消' })).toBeInTheDocument();
    });
});
