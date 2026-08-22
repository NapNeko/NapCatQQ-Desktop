//! 数据根:布局收敛(layout v1)与整树换根迁移。

pub mod consolidate;
pub mod relocate;

pub use consolidate::{ConsolidateReport, consolidate_data_root};
pub use relocate::{
    RelocateError, STAGING_DIR_NAME, delete_retired_data_root, estimate_copy_bytes,
    execute_relocate, normalize_root, preflight_relocate, read_retired_marker,
    target_inside_source,
};
