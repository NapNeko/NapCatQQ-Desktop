//! 崩溃诊断包(对齐 legacy crash_bundle.py 最小子集)

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use chrono::Local;
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

#[derive(Debug, Clone)]
pub struct CrashBundleInput {
    pub trigger: String,
    pub exception_summary: String,
    pub traceback_text: String,
    pub log_path: Option<PathBuf>,
    pub data_root: PathBuf,
    pub app_version: String,
}

/// 将诊断包写入 <data_root>/output/crash_<timestamp>.zip
pub fn write_crash_bundle(input: &CrashBundleInput) -> std::io::Result<PathBuf> {
    let out_dir = input.data_root.join("output");
    fs::create_dir_all(&out_dir)?;
    let stamp = Local::now().format("%Y%m%d_%H%M%S");
    let zip_path = out_dir.join(format!("crash_{stamp}.zip"));

    let file = File::create(&zip_path)?;
    let mut zip = ZipWriter::new(file);
    let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    let meta = format!(
        "trigger={}\nversion={}\nsummary={}\n",
        input.trigger, input.app_version, input.exception_summary
    );
    zip.start_file("meta.txt", opts)?;
    zip.write_all(meta.as_bytes())?;

    zip.start_file("traceback.txt", opts)?;
    zip.write_all(input.traceback_text.as_bytes())?;

    if let Some(log_path) = &input.log_path {
        if log_path.is_file() {
            if let Ok(mut f) = File::open(log_path) {
                let mut buf = Vec::new();
                if f.read_to_end(&mut buf).is_ok() {
                    let name = log_path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("session.log");
                    zip.start_file(format!("logs/{name}"), opts)?;
                    zip.write_all(&buf)?;
                }
            }
        }
    }

    zip.finish()?;
    Ok(zip_path)
}

/// 解析桌面可写输出目录(优先 data_root/output)
pub fn desktop_output_dir(data_root: &Path) -> PathBuf {
    data_root.join("output")
}