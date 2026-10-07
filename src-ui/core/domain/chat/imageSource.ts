export const INLINE_IMAGE_PREFIX = 'ncd-inline-image://';
export const isInlineImageReference = (
    value: unknown,
): value is `${typeof INLINE_IMAGE_PREFIX}${string}` =>
    typeof value === 'string' && value.startsWith(INLINE_IMAGE_PREFIX);
