//! 运行身份仅用于卸载门禁，不构成任何恢复意图。
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum RunningIdentity {
    Verified {
        pid: u32,
        created: u64,
        session: u32,
    },
    Unknown {
        worker_pid: u32,
        worker_created: u64,
    },
}

pub(super) fn record(
    root: &Path,
    id: &str,
    identity: Option<(u32, u64, u32)>,
) -> Result<(), String> {
    let state = match identity {
        Some((pid, created, session)) => RunningIdentity::Verified {
            pid,
            created,
            session,
        },
        None => RunningIdentity::Unknown {
            worker_pid: std::process::id(),
            worker_created: crate::native::created(std::process::id())?,
        },
    };
    crate::store::write(
        &root.join("session-bot-state").join(format!("{id}.json")),
        &state,
    )
}

pub(super) fn clear(root: &Path, id: &str) -> Result<(), String> {
    match std::fs::remove_file(root.join("session-bot-state").join(format!("{id}.json"))) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

pub(super) fn is_recorded(root: &Path, id: &str) -> bool {
    root.join("session-bot-state")
        .join(format!("{id}.json"))
        .is_file()
}

pub(super) fn blocks_uninstall(root: &Path) -> Result<bool, String> {
    let directory = root.join("session-bot-state");
    if !directory.exists() {
        return Ok(false);
    }
    for item in std::fs::read_dir(directory).map_err(|error| error.to_string())? {
        let path = item.map_err(|error| error.to_string())?.path();
        if path.extension().is_none_or(|extension| extension != "json") {
            continue;
        }
        match crate::store::read::<RunningIdentity>(&path)? {
            RunningIdentity::Unknown {
                worker_pid,
                worker_created,
            } => match crate::native::process_identity(worker_pid) {
                Ok(Some((created, _))) if created == worker_created => return Ok(true),
                Err(_) => return Ok(true),
                _ => {}
            },
            RunningIdentity::Verified {
                pid,
                created,
                session,
            } => match crate::native::process_identity(pid) {
                Ok(Some(actual)) if actual == (created, session) => return Ok(true),
                Err(_) => return Ok(true),
                _ => {}
            },
        }
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleared_desire_does_not_hide_a_live_process_from_uninstall() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("users/owner");
        let pid = std::process::id();
        let (created, session) = crate::native::process_identity(pid).unwrap().unwrap();
        record(&root, "123", Some((pid, created, session))).unwrap();
        assert!(!root.join("session-bots/123.json").exists());
        assert!(super::super::has_managed_bots(directory.path()).unwrap());
        clear(&root, "123").unwrap();
        assert!(!super::super::has_managed_bots(directory.path()).unwrap());
    }

    #[test]
    fn a_reused_pid_is_not_mistaken_for_the_managed_process() {
        let root = tempfile::tempdir().unwrap();
        let pid = std::process::id();
        let (created, session) = crate::native::process_identity(pid).unwrap().unwrap();
        record(root.path(), "123", Some((pid, created + 1, session))).unwrap();
        assert!(!blocks_uninstall(root.path()).unwrap());
    }

    #[test]
    fn unknown_identity_blocks_uninstall_without_becoming_desired_state() {
        let root = tempfile::tempdir().unwrap();
        record(root.path(), "123", None).unwrap();
        assert!(blocks_uninstall(root.path()).unwrap());
        assert!(!root.path().join("session-bots").exists());
    }

    #[test]
    fn an_unknown_identity_does_not_survive_its_owning_worker_generation() {
        let root = tempfile::tempdir().unwrap();
        let pid = std::process::id();
        let created = crate::native::created(pid).unwrap();
        crate::store::write(
            &root.path().join("session-bot-state/123.json"),
            &RunningIdentity::Unknown {
                worker_pid: pid,
                worker_created: created + 1,
            },
        )
        .unwrap();
        assert!(!blocks_uninstall(root.path()).unwrap());
    }
}
