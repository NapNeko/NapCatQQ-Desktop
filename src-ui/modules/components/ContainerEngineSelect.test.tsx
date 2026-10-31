// @vitest-environment jsdom
import { cleanup, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import { preferencesStore } from '../../core/domain/settings/preferencesStore';
import { ContainerEngineSelect } from './ContainerEngineSelect';

beforeAll(() => {
    Element.prototype.hasPointerCapture ??= () => false;
    Element.prototype.releasePointerCapture ??= () => {};
    Element.prototype.scrollIntoView ??= () => {};
});
beforeEach(() => {
    preferencesStore.setMotionEnabled(false);
});
afterEach(() => {
    cleanup();
    preferencesStore.reset();
});

describe('Linux host container engine selector', () => {
    it('switches engines through inline radio buttons without opening a popup', async () => {
        const user = userEvent.setup();
        const select = vi.fn();
        const { rerender } = render(
            <ContainerEngineSelect value="docker" disabled={false} onValueChange={select} />,
        );
        expect(screen.getByRole('radio', { name: 'Docker' })).toBeChecked();
        await user.click(screen.getByRole('radio', { name: 'Podman' }));
        expect(select).toHaveBeenCalledWith('podman');
        rerender(<ContainerEngineSelect value="podman" disabled={false} onValueChange={select} />);
        expect(screen.getByRole('radio', { name: 'Podman' })).toBeChecked();
        expect(screen.queryByRole('listbox')).not.toBeInTheDocument();
    });

    it('prevents switching while a mutation is pending', () => {
        render(<ContainerEngineSelect value="docker" disabled onValueChange={vi.fn()} />);
        expect(screen.getByRole('radio', { name: 'Docker' })).toBeDisabled();
        expect(screen.getByRole('radio', { name: 'Podman' })).toBeDisabled();
    });
});
