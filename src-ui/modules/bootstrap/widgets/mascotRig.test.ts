import { describe, expect, it } from 'vitest';
import rawCatGirl from '../../../assets/cat_girl.svg?raw';
import { MASCOT_PARTS, approxBBox, buildRiggedMascotMarkup } from './mascotRig';

// 素材一换，node-id 和坐标全变；这里挂了就说明要重新标定 mascotRig。
describe('MASCOT_PARTS 与 cat_girl.svg 对得上', () => {
    it('每个 node-id 在素材里恰好出现一次，且都是 path', () => {
        for (const ids of Object.values(MASCOT_PARTS)) {
            for (const id of ids) {
                const matches = rawCatGirl.match(new RegExp(`<path[^>]*node-id="${id}"`, 'g')) ?? [];
                expect(matches, `node-id=${id}`).toHaveLength(1);
            }
        }
    });

    it('部件之间不共用 path', () => {
        const all = Object.values(MASCOT_PARTS).flat();
        expect(new Set(all).size).toBe(all.length);
    });
});

describe('approxBBox', () => {
    it('按 (x, y) 交替取极值', () => {
        expect(approxBBox('M 10 20 C 30 40 5 60 12 8 Z')).toEqual([5, 8, 30, 60]);
        expect(approxBBox('')).toBeNull();
    });

    it('素材只用绝对坐标命令，包围盒近似才成立', () => {
        for (const m of rawCatGirl.matchAll(/ d="([^"]*)"/g)) {
            expect(m[1]).not.toMatch(/[ \d][mlhvcsqtaz]/);
        }
    });
});

describe('buildRiggedMascotMarkup', () => {
    const markup = buildRiggedMascotMarkup(rawCatGirl, 'p1');
    const count = (needle: string) => markup.split(needle).length - 1;
    const layerOf = (nodeId: string) => {
        const at = markup.indexOf(`node-id="${nodeId}"`);
        const before = markup.slice(0, at);
        return before.slice(before.lastIndexOf('data-part="') + 'data-part="'.length).split('"')[0];
    };

    it('五层加 figure 组，clipPath 带前缀', () => {
        for (const part of ['ground', 'body', 'heldCat', 'head', 'floorCat', 'figure']) {
            expect(count(`data-part="${part}"`), part).toBe(1);
        }
        expect(count('<clipPath id="p1-')).toBe(5);
    });

    it('全身剪影每层都有，脸部部件只落在自己那层', () => {
        expect(count('node-id="290"')).toBe(5);
        expect(count('node-id="591"')).toBe(5);
        for (const id of MASCOT_PARTS.eyes) {
            expect(count(`node-id="${id}"`), id).toBe(1);
            expect(layerOf(id)).toBe('head');
        }
        expect(layerOf(MASCOT_PARTS.heldCatEyes[0])).toBe('heldCat');
        expect(layerOf(MASCOT_PARTS.floorCatEyes[0])).toBe('floorCat');
        expect(layerOf(MASCOT_PARTS.bow[0])).toBe('head');
    });

    it('画布外的废 path 被丢掉', () => {
        expect(count('node-id="589"')).toBe(0);
    });

    it('path 全部自闭合，不会互相嵌套', () => {
        expect(count('<path')).toBe(count('/>') - count('<rect') - count('<polygon'));
        expect(count('</path>')).toBe(0);
    });
});
