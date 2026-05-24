//! 嵌入 Windows manifest 显式声明 `asInvoker`,
//! 防止 Windows installer detection heuristic 因 binary 名含 "update" 触发 UAC。
//!
//! 见 https://learn.microsoft.com/en-us/windows/win32/sbscs/application-manifests

#[cfg(windows)]
fn main() {
    use std::env;
    use std::fs;
    use std::path::Path;

    let out_dir = env::var("OUT_DIR").expect("OUT_DIR not set");
    let manifest = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
    <security>
      <requestedPrivileges>
        <requestedExecutionLevel level="asInvoker" uiAccess="false"/>
      </requestedPrivileges>
    </security>
  </trustInfo>
</assembly>
"#;

    let manifest_path = Path::new(&out_dir).join("ncd-update.manifest");
    fs::write(&manifest_path, manifest).expect("write manifest");

    // 仅对 MSVC linker 有效;mingw 上 cargo 会忽略
    // `rustc-link-arg` 对该 crate 所有产物(lib / test / bin)生效
    println!(
        "cargo:rustc-link-arg=/MANIFEST:EMBED"
    );
    println!(
        "cargo:rustc-link-arg=/MANIFESTINPUT:{}",
        manifest_path.display()
    );
}

#[cfg(not(windows))]
fn main() {}
