import { describe, expect, it } from 'vitest';
import { isSuperQQFace, projectQQFaceDisplay, qqFaceLarge } from './qqFaces';
import type { Segment } from '../debug/segments';

describe('QQ face wire projection', () => {
    it('uses explicit native types and never infers a received variant from the directory', () => {
        expect(isSuperQQFace('498', { id: '498' })).toBe(false);
        expect(isSuperQQFace('498', { faceType: 3, faceText: '[中!]' })).toBe(true);
        expect(isSuperQQFace('498', { raw: { faceType: 1 }, faceType: 3 })).toBe(false);
        expect(isSuperQQFace('498', { resultId: '1', chainCount: 3 })).toBe(false);
        expect(qqFaceLarge({ large: '0', raw: { faceType: 3 } })).toBe(false);
        expect(qqFaceLarge({ large: 'true' })).toBe(true);
        expect(qqFaceLarge({ id: '498' })).toBeUndefined();
    });
    it('folds one exact adjacent faceText copy while retaining ordinary and mixed text', () => {
        const face: Segment = {
            type: 'face',
            data: { id: '498', raw: { faceType: 3, faceText: '[中!]' } },
        };
        const caption: Segment = { type: 'text', data: { text: '[中!]' } };
        const authored: Segment = { type: 'text', data: { text: '[中!] 明天见' } };
        expect(projectQQFaceDisplay([face, caption, authored])).toEqual([face, authored]);
        expect(projectQQFaceDisplay([face, caption, caption])).toEqual([face, caption]);
        expect(projectQQFaceDisplay([{ ...face, data: { id: '498' } }, caption])).toEqual([
            { ...face, data: { id: '498' }, displayLarge: true },
        ]);
        expect(projectQQFaceDisplay([face, authored])).toEqual([face, authored]);
        const wireFace: Segment = {
            type: 'face',
            data: { id: '498', raw: { faceType: 3, faceText: '/中' } },
        };
        expect(projectQQFaceDisplay([wireFace, caption])).toEqual([wireFace]);
        expect(projectQQFaceDisplay([wireFace, authored])).toEqual([wireFace, authored]);
    });
    it('projects markerless SnowLuma screenshots without changing source segments or wire facts', () => {
        const original: Segment[] = [
            { type: 'face', data: { id: '498' } },
            { type: 'text', data: { text: '[中!]' } },
            { type: 'face', data: { id: '494' } },
            { type: 'text', data: { text: '[举杯邀月]' } },
            { type: 'face', data: { id: '495' } },
            { type: 'text', data: { text: '[兔来]' } },
        ];
        const snapshot = JSON.stringify(original);
        const projected = projectQQFaceDisplay(original);
        expect(projected).toHaveLength(3);
        expect(projected.every((face) => face.displayLarge)).toBe(true);
        expect(projected.map((face) => face.data)).toEqual([
            original[0].data,
            original[2].data,
            original[4].data,
        ]);
        expect(qqFaceLarge(original[0].data)).toBeUndefined();
        expect(JSON.stringify(original)).toBe(snapshot);
    });
    it('preserves explicit small variants and authored text while limiting bare-face compatibility display', () => {
        const caption: Segment = { type: 'text', data: { text: '[兔来]' } };
        const small: Segment = { type: 'face', data: { id: '495', large: false } };
        expect(projectQQFaceDisplay([small, caption])).toEqual([small, caption]);
        const unknown: Segment = { type: 'face', data: { id: '495' } };
        const authored: Segment = { type: 'text', data: { text: '今晚见' } };
        expect(projectQQFaceDisplay([unknown, caption, authored])).toEqual([
            unknown,
            caption,
            authored,
        ]);
        expect(
            projectQQFaceDisplay([unknown, { type: 'text', data: { text: '[兔来] 今晚见' } }]),
        ).toHaveLength(2);
        expect(projectQQFaceDisplay([unknown])[0].displayLarge).toBe(true);
        for (const id of ['5', '53', '74', '114', '450'])
            expect(
                projectQQFaceDisplay([{ type: 'face', data: { id } }])[0].displayLarge,
            ).toBeUndefined();
    });
    it('recognizes a complete known alias without guessing from partial text', () => {
        const face: Segment = { type: 'face', data: { id: '495' } };
        const lookup = () => ({ id: '495', name: '兔来', aliases: ['月兔来了'], super: true });
        expect(
            projectQQFaceDisplay([face, { type: 'text', data: { text: '[月兔来了]' } }], lookup)[0]
                .displayLarge,
        ).toBe(true);
        expect(
            projectQQFaceDisplay([face, { type: 'text', data: { text: '[月兔来了]!' } }], lookup),
        ).toHaveLength(2);
    });
});
