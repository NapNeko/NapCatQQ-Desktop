use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TransferKind {
    AppConfig,
    BotConfig,
    AppSettings,
    Servers,
    Instances,
    Chat,
    DebugWorkspace,
    DebugCollections,
    SnowLuma,
    Frontend,
    Adopt,
    Framework,
}

pub(super) struct TransferFile {
    pub data_relative: &'static str,
    pub archive_name: &'static str,
    pub label: &'static str,
    pub kind: TransferKind,
}

pub(super) const TRANSFER_FILES: &[TransferFile] = &[
    TransferFile {
        data_relative: "config/config.json",
        archive_name: "config.json",
        label: "应用配置",
        kind: TransferKind::AppConfig,
    },
    TransferFile {
        data_relative: "config/bot.json",
        archive_name: "bot.json",
        label: "Bot 配置",
        kind: TransferKind::BotConfig,
    },
    TransferFile {
        data_relative: "config/app-settings.json",
        archive_name: "app-settings.json",
        label: "应用设置",
        kind: TransferKind::AppSettings,
    },
    TransferFile {
        data_relative: "config/servers.json",
        archive_name: "servers.json",
        label: "远端服务器档案",
        kind: TransferKind::Servers,
    },
    TransferFile {
        data_relative: "config/app-instances.json",
        archive_name: "app-instances.json",
        label: "应用实例与关联",
        kind: TransferKind::Instances,
    },
    TransferFile {
        data_relative: "config/chat-desktop.json",
        archive_name: "chat-desktop.json",
        label: "聊天账号与通知偏好",
        kind: TransferKind::Chat,
    },
    TransferFile {
        data_relative: "onebot-debug/workspace.json",
        archive_name: "onebot-debug/workspace.json",
        label: "API 调试工作区",
        kind: TransferKind::DebugWorkspace,
    },
    TransferFile {
        data_relative: "onebot-debug/collections.json",
        archive_name: "onebot-debug/collections.json",
        label: "API 调试收藏",
        kind: TransferKind::DebugCollections,
    },
    TransferFile {
        data_relative: "state/snowluma/app-config.json",
        archive_name: "state/snowluma/app-config.json",
        label: "SnowLuma 全局设置",
        kind: TransferKind::SnowLuma,
    },
    TransferFile {
        data_relative: "config/frontend-preferences.json",
        archive_name: "frontend-preferences.json",
        label: "界面与终端偏好",
        kind: TransferKind::Frontend,
    },
];

#[derive(Debug, Clone)]
pub(super) struct TransferEntry {
    pub data_relative: String,
    pub archive_name: String,
    pub label: String,
    pub kind: TransferKind,
}

impl From<&TransferFile> for TransferEntry {
    fn from(file: &TransferFile) -> Self {
        Self {
            data_relative: file.data_relative.into(),
            archive_name: file.archive_name.into(),
            label: file.label.into(),
            kind: file.kind,
        }
    }
}

pub(super) fn is_safe_instance_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 160
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

pub(super) fn validate_relative_path(name: &str) -> Result<(), String> {
    if name.is_empty()
        || name.starts_with('/')
        || name.contains(['\\', ':', '\0'])
        || name
            .split('/')
            .any(|part| part.is_empty() || matches!(part, "." | ".."))
        || Path::new(name).is_absolute()
    {
        return Err(format!("非法相对路径，已中止导入: {name}"));
    }
    Ok(())
}

pub(super) fn identify_source_path(name: &str) -> Result<Option<TransferEntry>, String> {
    if let Some(file) = TRANSFER_FILES
        .iter()
        .find(|file| name == file.archive_name || name == file.data_relative)
    {
        return Ok(Some(file.into()));
    }
    let (relative, directory, kind, label) = if let Some(relative) = name
        .strip_prefix("config/app-adopts/")
        .or_else(|| name.strip_prefix("app-adopts/"))
    {
        (
            relative,
            "app-adopts",
            TransferKind::Adopt,
            "应用领养回滚快照",
        )
    } else if let Some(relative) = name
        .strip_prefix("config/framework-configs/")
        .or_else(|| name.strip_prefix("framework-configs/"))
    {
        (
            relative,
            "framework-configs",
            TransferKind::Framework,
            "框架配置",
        )
    } else {
        return Ok(None);
    };
    let Some(id) = relative.strip_suffix(".json") else {
        return Ok(None);
    };
    if relative.contains('/') {
        return Ok(None);
    }
    if !is_safe_instance_id(id) {
        return Err(format!("{label}文件名含非法实例 ID: {name}"));
    }
    let data_relative = format!("config/{directory}/{id}.json");
    Ok(Some(TransferEntry {
        archive_name: data_relative.clone(),
        data_relative,
        label: format!("{label} ({id})"),
        kind,
    }))
}
