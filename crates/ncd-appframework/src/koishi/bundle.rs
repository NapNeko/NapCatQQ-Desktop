//! 整包 zip 重打成 tar.gz：远端最小化系统常常没有 unzip（装它还要 root），tar 几乎都有。
//! Linux 包里 `node_modules/.bin` 是符号链接（zip 用 unix 模式位记着），要照原样打成 tar 的链接项。

use std::fs::File;
use std::io::Read;
use std::path::Path;

const S_IFMT: u32 = 0o170000;
const S_IFLNK: u32 = 0o120000;

pub fn zip_to_tar_gz(zip_path: &Path, out_path: &Path) -> Result<(), String> {
    let file = File::open(zip_path).map_err(|e| format!("打开整包失败: {e}"))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("整包不是有效的 zip: {e}"))?;
    let out = File::create(out_path).map_err(|e| format!("创建 tar.gz 失败: {e}"))?;
    let gz = flate2::write::GzEncoder::new(out, flate2::Compression::fast());
    let mut tar = tar::Builder::new(gz);
    tar.follow_symlinks(false);

    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| format!("读整包第 {i} 项失败: {e}"))?;
        let Some(name) = entry.enclosed_name() else {
            return Err(format!("整包里有越界路径：{}", entry.name()));
        };
        let name = name.to_string_lossy().replace('\\', "/");
        let mode = entry.unix_mode();
        let mut header = tar::Header::new_gnu();
        header.set_mtime(0);
        if entry.is_dir() {
            header.set_entry_type(tar::EntryType::Directory);
            header.set_mode(mode.map(|m| m & 0o7777).unwrap_or(0o755));
            header.set_size(0);
            tar.append_data(
                &mut header,
                format!("{}/", name.trim_end_matches('/')),
                std::io::empty(),
            )
            .map_err(|e| format!("写 tar 失败: {e}"))?;
            continue;
        }
        let mut body = Vec::with_capacity(entry.size() as usize);
        entry
            .read_to_end(&mut body)
            .map_err(|e| format!("解整包 {name} 失败: {e}"))?;
        if mode.is_some_and(|m| m & S_IFMT == S_IFLNK) {
            let target = String::from_utf8_lossy(&body).into_owned();
            header.set_entry_type(tar::EntryType::Symlink);
            header.set_mode(0o777);
            header.set_size(0);
            tar.append_link(&mut header, &name, &target)
                .map_err(|e| format!("写 tar 链接 {name} 失败: {e}"))?;
            continue;
        }
        header.set_entry_type(tar::EntryType::Regular);
        header.set_mode(
            mode.map(|m| m & 0o7777)
                .filter(|m| *m != 0)
                .unwrap_or(0o644),
        );
        header.set_size(body.len() as u64);
        tar.append_data(&mut header, &name, body.as_slice())
            .map_err(|e| format!("写 tar {name} 失败: {e}"))?;
    }
    let gz = tar
        .into_inner()
        .map_err(|e| format!("收尾 tar 失败: {e}"))?;
    gz.finish().map_err(|e| format!("收尾 gzip 失败: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn repacks_files_dirs_modes_and_symlinks() {
        let dir = tempfile::tempdir().unwrap();
        let zip_path = dir.path().join("b.zip");
        {
            let mut w = zip::ZipWriter::new(File::create(&zip_path).unwrap());
            let opts = zip::write::SimpleFileOptions::default();
            w.add_directory("node_modules/", opts.unix_permissions(0o755))
                .unwrap();
            w.start_file("koishi.yml", opts.unix_permissions(0o644))
                .unwrap();
            w.write_all(b"plugins: {}\n").unwrap();
            w.start_file("node_modules/x/cli.js", opts.unix_permissions(0o755))
                .unwrap();
            w.write_all(b"#!/usr/bin/env node\n").unwrap();
            w.add_symlink("node_modules/.bin/x", "../x/cli.js", opts)
                .unwrap();
            w.finish().unwrap();
        }
        let out = dir.path().join("b.tar.gz");
        zip_to_tar_gz(&zip_path, &out).unwrap();

        let gz = flate2::read::GzDecoder::new(File::open(&out).unwrap());
        let mut tar = tar::Archive::new(gz);
        let mut seen = Vec::new();
        for e in tar.entries().unwrap() {
            let e = e.unwrap();
            let path = e.path().unwrap().to_string_lossy().into_owned();
            let kind = e.header().entry_type();
            let link = e
                .link_name()
                .unwrap()
                .map(|p| p.to_string_lossy().into_owned());
            seen.push((path, kind, e.header().mode().unwrap() & 0o777, link));
        }
        assert!(
            seen.iter()
                .any(|(p, k, _, _)| p == "node_modules/" && k.is_dir())
        );
        assert!(
            seen.iter()
                .any(|(p, _, m, _)| p == "node_modules/x/cli.js" && *m == 0o755)
        );
        assert!(seen.iter().any(|(p, k, _, l)| {
            p == "node_modules/.bin/x" && k.is_symlink() && l.as_deref() == Some("../x/cli.js")
        }));
        assert!(
            seen.iter()
                .any(|(p, _, m, _)| p == "koishi.yml" && *m == 0o644)
        );
    }
}
