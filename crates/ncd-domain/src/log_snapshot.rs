// LogSnapshot: 日志尾部快照
//
// Bot、应用端实例和 Desktop 自己的日志页共用这一个形状，前端直接用生成的类型。

use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct LogSnapshot {
    pub lines: Vec<String>,
    /// 截尾前的总行数
    pub total_lines: usize,
}
