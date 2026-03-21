# Migration Template

## Summary

- Migration ID:
- Target version:
- Affected local range:
- Script URL:

## Why This Migration Exists

说明为什么普通覆盖更新不够，需要额外迁移步骤。

## Preconditions

- Desktop must be fully stopped
- Backup must be created before mutation
- Rollback path must be verified

## Files And Directories Touched

- `path/to/config`
- `path/to/runtime`

## Migration Steps

1. Validate current install layout.
2. Backup current app data.
3. Apply file moves, renames, or cleanup.
4. Verify final layout.
5. Exit with non-zero code on failure.

## Rollback

说明回滚时恢复哪些目录、哪些文件，以及哪些副作用不能自动回退。

## Manual Recovery

如果自动迁移失败，用户应如何手动恢复。

## Validation Checklist

- Backup generated
- Migration success on supported lowest version
- Migration success on supported highest version
- Rollback works
- Existing ZIP updater still works after migration
