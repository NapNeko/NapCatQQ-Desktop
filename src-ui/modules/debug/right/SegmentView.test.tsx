import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { SegmentList } from './SegmentView';

describe('QQ face segments', () => {
    it('renders QQ faces inline with surrounding text', () => {
        render(<SegmentList mine={false} segments={[{ type: 'text', data: { text: '你好' } }, { type: 'face', data: { id: 14 } }, { type: 'text', data: { text: '！' } }]} />);
        const face = screen.getByRole('img', { name: 'QQ 表情 14' });
        expect(face).toHaveAttribute('src', 'https://koishi.js.org/QFace/assets/qq_emoji/14/png/14.png');
        expect(face).toHaveAttribute('width', '24');
        expect(screen.getByText('你好')).toBeInTheDocument();
        expect(screen.getByText('！')).toBeInTheDocument();
    });

    it('keeps a readable fallback when an asset fails and retries a changed face', () => {
        const { rerender } = render(<SegmentList mine={false} segments={[{ type: 'face', data: { id: '277' } }]} />);
        fireEvent.error(screen.getByRole('img', { name: 'QQ 表情 277' }));
        expect(screen.queryByRole('img')).not.toBeInTheDocument();
        expect(screen.getByText('表情 277')).toBeInTheDocument();
        rerender(<SegmentList mine={false} segments={[{ type: 'face', data: { id: '0' } }]} />);
        expect(screen.getByRole('img', { name: 'QQ 表情 0' })).toBeInTheDocument();
    });

    it('does not interpolate malformed identifiers into resource URLs', () => {
        render(<SegmentList mine={false} segments={[{ type: 'face', data: { id: '../external' } }, { type: 'face', data: {} }]} />);
        expect(screen.queryByRole('img')).not.toBeInTheDocument();
        expect(screen.getAllByText('表情')).toHaveLength(2);
    });
});
