// NapCatQQ Desktop - Shared Utilities

/**
 * Format bytes to human readable string (KB, MB, GB, etc.)
 */
export function formatBytes(bytes: number | null | undefined, decimals = 2): string {
  if (bytes === null || bytes === undefined || isNaN(bytes)) return '0 Bytes';
  if (bytes === 0) return '0 Bytes';

  const k = 1024;
  const dm = decimals < 0 ? 0 : decimals;
  const sizes = ['Bytes', 'KB', 'MB', 'GB', 'TB'];

  const i = Math.floor(Math.log(bytes) / Math.log(k));

  return parseFloat((bytes / Math.pow(k, i)).toFixed(dm)) + ' ' + sizes[i];
}

/**
 * Format epoch timestamp to locale date time string
 */
export function formatTimestamp(timestamp: number | null | undefined): string {
  if (!timestamp) return '-';
  // Check if timestamp is in seconds or milliseconds
  const ms = timestamp < 1000000000000 ? timestamp * 1000 : timestamp;
  return new Date(ms).toLocaleString('zh-CN', {
    hour12: false,
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
  });
}

/**
 * Clean up long paths for elegant desktop display
 */
export function compactPath(path: string | null | undefined, maxLength = 40): string {
  if (!path) return '-';
  if (path.length <= maxLength) return path;
  
  const separator = path.includes('/') ? '/' : '\\';
  const parts = path.split(separator);
  if (parts.length <= 2) return path;

  const first = parts[0];
  const last = parts[parts.length - 1];
  return `${first}${separator}...${separator}${last}`;
}
