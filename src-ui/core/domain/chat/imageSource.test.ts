import { describe, expect, it } from 'vitest';
import { withLocalImageSources } from './imageSource';

describe('image source preservation', () => {
    it('keeps the image ordinal when a remote image precedes a local attachment', () => {
        const previous = [
            { type: 'image', data: { url: 'https://cdn.example/first.png' } },
            { type: 'text', data: { text: '说明' } },
            { type: 'image', data: { file: 'ncd-local-file://D:/second.png' } },
        ];
        const incoming = [
            { type: 'image', data: { file: 'first', url: 'https://cdn.example/first-new.png' } },
            { type: 'image', data: { file: 'second' } },
        ];
        const merged = withLocalImageSources(incoming, previous);
        expect(merged[0]).toBe(incoming[0]);
        expect(merged[1].data.local_file).toBe('ncd-local-file://D:/second.png');
        expect(previous[2].data).not.toHaveProperty('local_file');
    });

    it('does not guess image correspondence when the protocol changes the image count', () => {
        const incoming = [{ type: 'image', data: { file: 'only-image' } }];
        expect(
            withLocalImageSources(incoming, [
                { type: 'image', data: { file: 'ncd-local-file://D:/first.png' } },
                { type: 'image', data: { file: 'ncd-local-file://D:/second.png' } },
            ]),
        ).toEqual(incoming);
    });

    it('keeps recoverable self-forwarded content when an echo supplies only a resource id', () => {
        const content = [
            {
                type: 'node',
                data: {
                    message_id: '70',
                    content: [
                        { type: 'image', data: { local_file: 'ncd-local-file://D:/sent.png' } },
                    ],
                },
            },
        ];
        expect(
            withLocalImageSources(
                [{ type: 'forward', data: { id: 'server-resource' } }],
                [{ type: 'forward', data: { content } }],
            )[0].data,
        ).toEqual({ id: 'server-resource', content });
    });
});
