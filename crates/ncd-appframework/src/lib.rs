//! 应用框架的安装、启动、WebUI 入口与 OneBot 对接适配器。

pub mod adapter;
pub mod adopt;
pub mod astrbot;
pub mod config_backup;
pub mod config_doc;
pub mod env_file;
pub mod karin;
pub mod koishi;
pub mod maibot;
pub mod neobot;
pub mod node_tooling;
pub mod nonebot2;
pub mod ports;
pub mod registry;
pub mod terminal;
pub mod toml_patch;
pub mod uv_tooling;
pub mod yaml_patch;
pub mod yunzai;

pub(crate) fn remote_windows_requirements(
    mut requirements: Vec<ncd_component::Requirement>,
    os: ncd_host::Os,
    locality: ncd_host::Locality,
) -> Vec<ncd_component::Requirement> {
    if os == ncd_host::Os::Windows && locality == ncd_host::Locality::Remote {
        requirements.push(ncd_component::Requirement::component(
            ncd_component::ComponentId::WindowsAgent,
        ));
    }
    requirements
}

pub use adapter::{AppComponentSpec, AppFrameworkAdapter};
pub use adopt::{
    AdoptRestoreScope, AdoptedFile, capture_adopted_files, remove_ncd_debris, restore_adopted_files,
};
pub use astrbot::join_webui_url;
pub use karin::KarinAdapter;
pub use koishi::KoishiComponent;
pub use maibot::MaiBotAdapterConfig;
pub use neobot::PackageVersions;
pub use nonebot2::NoneBot2Adapter;
pub use ports::remote_listening_ports;
pub use registry::AppFrameworkRegistry;
pub use terminal::AppTerminalProfile;
