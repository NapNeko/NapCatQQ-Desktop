//! 在实例目录里起一个 node 现取插件信息：schema（Schemastery 的 `{uid, refs}` JSON，控制台也是拿这个画表单）、
//! 用法说明、已装插件包的版本和简介。停着也能用，跑着也不打扰它：只是 require 一下，不建 Context。
//!
//! 远端一个个读 package.json 要几十次 SFTP 往返，所以已装列表也交给同一个脚本一次吐完。

use std::time::Duration;

use ncd_domain::AppInstance;
use ncd_host::{Host, HostCommand, HostPath};
use ncd_traits::AppFrameworkError;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use super::yarn::{path_env, resolve_node};

const PROBE_TIMEOUT: Duration = Duration::from_secs(60);

/// 插件名解析规则同上游 `Installer.resolveName` / loader：短名先试官方前缀再试社区前缀
const PROBE_SCRIPT: &str = r#"
const fs = require('fs'), path = require('path');
const base = process.cwd();
const mode = process.env.NCD_KOISHI_MODE;
const res = (id) => require.resolve(id, { paths: [base] });
const candidates = (name) => {
  if (name.startsWith('@koishijs/plugin-') || /(^|\/)koishi-plugin-/.test(name)) return [name];
  if (name.startsWith('@')) { const [l, r] = name.split('/'); return [`${l}/koishi-plugin-${r}`]; }
  return [`@koishijs/plugin-${name}`, `koishi-plugin-${name}`];
};
const shortOf = (pkg) => pkg.replace(/(koishi-|^@koishijs\/)plugin-/, '');
const readPkg = (pkg) => {
  try { return JSON.parse(fs.readFileSync(path.join(base, 'node_modules', pkg, 'package.json'), 'utf8')); } catch { return null; }
};
const desc = (meta) => {
  const d = meta && meta.koishi && meta.koishi.description;
  if (d && typeof d === 'object') return d.zh || d['zh-CN'] || d.en || Object.values(d)[0] || '';
  return (meta && meta.description) || '';
};
const out = [];
if (mode === 'packages') {
  let deps = {};
  try { deps = JSON.parse(fs.readFileSync(path.join(base, 'package.json'), 'utf8')).dependencies || {}; } catch {}
  for (const pkg of Object.keys(deps)) {
    if (!(pkg.startsWith('@koishijs/plugin-') || /(^|\/)koishi-plugin-/.test(pkg))) continue;
    const meta = readPkg(pkg);
    out.push({ package: pkg, name: shortOf(pkg), request: String(deps[pkg]), version: meta ? meta.version || null : null, description: desc(meta) });
  }
} else {
  const names = JSON.parse(process.env.NCD_KOISHI_NAMES || '[]');
  for (const name of names) {
    const row = { name, package: null, version: null, schema: null, usage: null, error: null };
    try {
      if (name === '') {
        row.package = 'koishi';
        row.schema = JSON.parse(JSON.stringify(require(res('koishi')).Context.Config));
      } else {
        let mod, err;
        for (const id of candidates(name)) {
          try { mod = require(res(id)); row.package = id; break; } catch (e) { err = e; }
        }
        if (!mod) throw err || new Error('not found');
        const meta = readPkg(row.package);
        row.version = meta ? meta.version || null : null;
        const e = mod.default || mod;
        const schema = e && (e.Config || e.schema);
        row.schema = schema ? JSON.parse(JSON.stringify(schema)) : null;
        row.usage = e && typeof e.usage === 'string' ? e.usage : null;
      }
    } catch (e) { row.error = String((e && e.message) || e).split('\n')[0]; }
    out.push(row);
  }
}
process.stdout.write('\n@@NCD@@' + JSON.stringify(out));
"#;

/// 一个插件的表单来源
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/koishi/")]
pub struct KoishiPluginSchema {
    /// 插件树里的短名；空串 = 全局设置
    pub name: String,
    pub package: Option<String>,
    pub version: Option<String>,
    /// Schemastery 序列化（`{uid, refs}`）；插件没声明配置时为 null
    #[ts(type = "unknown")]
    pub schema: Option<Value>,
    /// 插件自己写的使用说明（Markdown）
    pub usage: Option<String>,
    /// 包没装上、导入报错时的原因
    pub error: Option<String>,
}

/// package.json 里的一个插件依赖
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/koishi/")]
pub struct KoishiPackageInfo {
    pub package: String,
    /// 插件树里用的短名
    pub name: String,
    /// package.json 里写的版本范围
    pub request: String,
    /// node_modules 里实际装的；没装上为 null
    pub version: Option<String>,
    pub description: String,
}

async fn run_probe(
    host: &dyn Host,
    instance: &AppInstance,
    mode: &str,
    names: &[String],
) -> Result<Value, AppFrameworkError> {
    let install_dir = HostPath::from_posix(&instance.install_dir);
    let tc = resolve_node(host, &install_dir, None)
        .await
        .map_err(|e| AppFrameworkError::Runtime(e.to_string()))?;
    let names_json = serde_json::to_string(names).unwrap_or_else(|_| "[]".into());
    let cmd = HostCommand::new(tc.node_bin.as_posix())
        .arg("-e")
        .arg(PROBE_SCRIPT.trim())
        .working_dir(install_dir.clone())
        .env("PATH", path_env(&tc, host.os(), host.locality()))
        .env("NCD_KOISHI_MODE", mode)
        .env("NCD_KOISHI_NAMES", names_json)
        .timeout(PROBE_TIMEOUT);
    let out = host
        .run_to_string(cmd)
        .await
        .map_err(|e| AppFrameworkError::Host(e.to_string()))?;
    parse_probe_output(&out.stdout).ok_or_else(|| {
        AppFrameworkError::Runtime(format!(
            "读取 Koishi 插件信息失败（exit={:?}）：{}",
            out.exit_code,
            out.stderr.trim().lines().last().unwrap_or_default()
        ))
    })
}

/// 插件加载时可能往 stdout 打东西，只认最后的标记之后那段
pub fn parse_probe_output(stdout: &str) -> Option<Value> {
    let (_, tail) = stdout.rsplit_once("@@NCD@@")?;
    serde_json::from_str(tail.trim()).ok()
}

pub async fn plugin_schemas(
    host: &dyn Host,
    instance: &AppInstance,
    names: &[String],
) -> Result<Vec<KoishiPluginSchema>, AppFrameworkError> {
    if names.is_empty() {
        return Ok(Vec::new());
    }
    let v = run_probe(host, instance, "schema", names).await?;
    serde_json::from_value(v)
        .map_err(|e| AppFrameworkError::Runtime(format!("插件信息格式不对: {e}")))
}

pub async fn installed_packages(
    host: &dyn Host,
    instance: &AppInstance,
) -> Result<Vec<KoishiPackageInfo>, AppFrameworkError> {
    let v = run_probe(host, instance, "packages", &[]).await?;
    serde_json::from_value(v)
        .map_err(|e| AppFrameworkError::Runtime(format!("插件列表格式不对: {e}")))
}

/// 包名 → 插件树短名（同上游 `loader.keyFor`）
pub fn short_name(package: &str) -> String {
    if let Some(rest) = package.strip_prefix("@koishijs/plugin-") {
        return rest.to_string();
    }
    if let Some(rest) = package.strip_prefix("koishi-plugin-") {
        return rest.to_string();
    }
    package.replacen("/koishi-plugin-", "/", 1)
}

pub fn is_plugin_package(package: &str) -> bool {
    package.starts_with("@koishijs/plugin-")
        || package.starts_with("koishi-plugin-")
        || package.contains("/koishi-plugin-")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_names_follow_loader() {
        assert_eq!(short_name("@koishijs/plugin-server"), "server");
        assert_eq!(short_name("koishi-plugin-adapter-onebot"), "adapter-onebot");
        assert_eq!(short_name("@foo/koishi-plugin-bar"), "@foo/bar");
        assert!(is_plugin_package("@foo/koishi-plugin-bar"));
        assert!(!is_plugin_package("koishi"));
        assert!(!is_plugin_package("@koishijs/client"));
    }

    #[test]
    fn probe_output_ignores_noise_before_marker() {
        let out = "warn: something\n\n@@NCD@@[{\"name\":\"\",\"package\":\"koishi\",\"version\":null,\"schema\":{\"uid\":1,\"refs\":{}},\"usage\":null,\"error\":null}]";
        let v = parse_probe_output(out).unwrap();
        let rows: Vec<KoishiPluginSchema> = serde_json::from_value(v).unwrap();
        assert_eq!(rows[0].package.as_deref(), Some("koishi"));
        assert!(parse_probe_output("no marker").is_none());
    }
}
