// 文件 / 图片 / 语音 / 视频参数：填 URL、Bot 所在机器上的路径，或 base64。
// 把本机文件传给远端 Bot 是第二期的事，这里只是个文本框，提示写清楚能填什么。

import { TextField } from './TextField';
import type { FieldProps } from './fieldKit';

export const FILE_HINT = 'URL、Bot 所在机器上的路径，或 base64://…';

export function FileField(props: FieldProps) {
    return <TextField {...props} mono placeholder={FILE_HINT} />;
}
