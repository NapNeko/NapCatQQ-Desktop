//! 产品双窗验收入口；独立 identifier 避免单实例串扰，临时数据根仍可能触发旧配置迁移。
fn main() {
    ncd_tauri::run();
}
