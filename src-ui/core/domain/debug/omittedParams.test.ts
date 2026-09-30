import { describe, expect, it } from 'vitest';
import { countOmittedInValue, countOmittedParams, isFullyOmitted, omittedBlocker } from './omittedParams';

describe('省略占位检测', () => {
    it('超长字符串占位按叶子计数，嵌套的也数', () => {
        const params = {
            group_id: 1,
            file: '<已省略 65537 字节>',
            message: [{ type: 'image', data: { file: '<已省略 200000 字节>' } }],
            ok_text: '<已省略的提示>',
        };
        expect(countOmittedInValue(params)).toBe(2);
    });

    it('整体摘要看成一处，里面留着的短标量不数', () => {
        expect(countOmittedInValue({ _omitted: '<参数共 300000 字节，已省略>' })).toBe(1);
        expect(isFullyOmitted({ _omitted: '<参数共 300000 字节，已省略>' })).toBe(true);
        expect(isFullyOmitted({ a: 1 })).toBe(false);
    });

    it('占位文字要和后端的写法完全一致才算，用户随手打的相似文字不算', () => {
        expect(countOmittedInValue({ file: '<已省略 65537 字节> ' })).toBe(0);
        expect(countOmittedInValue({ file: '已省略 65537 字节' })).toBe(0);
        expect(countOmittedInValue({ file: '<已省略 x 字节>' })).toBe(0);
    });

    it('文本版：JSON 写坏了算 0；没有子串时不解析直接 0；正常参数是 0', () => {
        expect(countOmittedParams('{"file":')).toBe(0);
        expect(countOmittedParams('{"group_id": 123, "message": "你好"}')).toBe(0);
        expect(countOmittedParams(JSON.stringify({ file: '<已省略 65537 字节>', message: [{ data: { file: '<已省略 1 字节>' } }] }))).toBe(2);
    });

    it('拦截文案按条数拼', () => {
        expect(omittedBlocker(0)).toBeNull();
        expect(omittedBlocker(2)).toBe('有 2 处超长参数在存盘时被省略，补上原文再发');
    });
});
