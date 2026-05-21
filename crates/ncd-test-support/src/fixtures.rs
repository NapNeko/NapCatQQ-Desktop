use std::fs;
use std::io;
use std::path::{Path, PathBuf};

fn fixtures_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures")
}

pub fn fixture_path(relative: impl AsRef<Path>) -> PathBuf {
    fixtures_root().join(relative)
}

pub fn read_fixture(relative: impl AsRef<Path>) -> io::Result<String> {
    fs::read_to_string(fixture_path(relative))
}

pub fn fixture_bytes(relative: impl AsRef<Path>) -> io::Result<Vec<u8>> {
    fs::read(fixture_path(relative))
}

pub fn legacy_config_fixture() -> io::Result<String> {
    read_fixture("legacy/config.json")
}

pub fn legacy_bot_fixture() -> io::Result<String> {
    read_fixture("legacy/bot.json")
}

pub fn legacy_servers_fixture() -> io::Result<String> {
    read_fixture("legacy/servers.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_legacy_config_fixture() {
        let content = legacy_config_fixture().unwrap();
        assert!(content.contains("runtime_target"));
    }
}
