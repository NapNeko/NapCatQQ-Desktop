// 本机目录选择框。webview 拿不到真实文件系统路径，只能走对话框插件；modules 层不直接碰 transport，经这里调。
// 用户取消返回 null；对话框打不开（浏览器预览里没有插件）时弹错误条，同样返回 null。

import { useCallback } from 'react';
import { pickDirectory } from '../core/ipc/transport';
import { pushErrorBar } from './ui/pushErrorBar';
import { errorText } from '../core/domain/errors';

export function usePickDirectory() {
    return useCallback(async (title: string): Promise<string | null> => {
        try {
            return await pickDirectory(title);
        } catch (err) {
            pushErrorBar({ key: 'pick-directory', title: '打不开目录选择框', raw: errorText(err) });
            return null;
        }
    }, []);
}
