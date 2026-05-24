// 兼容层：原 utils 已下沉到 `core/domain/bootstrap/format.ts`。
// 现有 import 路径保留为 re-export，避免一次性大改 features 文件。

export { formatBytes, formatTimestamp, compactPath } from '../core/domain/bootstrap/format';
