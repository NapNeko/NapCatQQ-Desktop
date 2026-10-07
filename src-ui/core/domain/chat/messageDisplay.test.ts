import { expect, it } from 'vitest';
import { projectMessageDisplay } from './messageDisplay';
import type { Segment } from '../debug/segments';

it('folds exactly one marketplace caption and preserves real text and the wire source', () => {
    const image: Segment = {
        type: 'image',
        data: {
            emoji_id: 'market-face',
            emoji_package_id: 42,
            key: 'wire-key',
            summary: '[好]',
            url: 'https://cdn.example/face.gif',
        },
    };
    const caption: Segment = { type: 'text', data: { text: '[好]' } };
    const text: Segment = { type: 'text', data: { text: '[好] 今晚见' } };
    const source = [image, caption, text];
    const snapshot = JSON.stringify(source);
    expect(projectMessageDisplay(source)).toEqual([image, text]);
    expect(JSON.stringify(source)).toBe(snapshot);
    expect(
        projectMessageDisplay([
            {
                type: 'image',
                data: { url: 'https://cdn.example/face.gif', summary: '[好]', sub_type: 1 },
            },
            caption,
        ]),
    ).toHaveLength(2);
    expect(projectMessageDisplay([{ type: 'mface', data: { name: '好' } }, caption])).toHaveLength(
        1,
    );
    expect(
        projectMessageDisplay([
            { type: 'image', data: { emoji_id: 7, emoji_package_id: 42, summary: '[好]' } },
            { type: 'text', data: { text: '\n[好] ' } },
        ]),
    ).toHaveLength(1);
    expect(
        projectMessageDisplay([image, { type: 'text', data: { text: '[好] [好]' } }]),
    ).toHaveLength(2);
});
