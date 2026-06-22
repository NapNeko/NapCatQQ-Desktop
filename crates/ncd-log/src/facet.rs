//! 对齐 legacy log_enum.LogType / LogSource 的展示段(非强类型业务枚举)

/// 六段格式第 3 段
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogType {
    NoneType,
    FileFunc,
    Network,
    Process,
    Config,
}

impl LogType {
    pub fn segment(self) -> &'static str {
        match self {
            LogType::NoneType => "[ NONE_TYPE ]",
            LogType::FileFunc => "[ FILE_FUNC ]",
            LogType::Network => "[ NETWORK ]",
            LogType::Process => "[ PROCESS ]",
            LogType::Config => "[ CONFIG ]",
        }
    }
}

/// 六段格式第 4 段
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogSource {
    None,
    Core,
    Ui,
    Bot,
    Remote,
    Component,
}

impl LogSource {
    pub fn segment(self) -> &'static str {
        match self {
            LogSource::None => "[ NONE ]",
            LogSource::Core => "[ CORE ]",
            LogSource::Ui => "[  UI  ]",
            LogSource::Bot => "[ BOT ]",
            LogSource::Remote => "[ REMOTE ]",
            LogSource::Component => "[ COMPONENT ]",
        }
    }
}
