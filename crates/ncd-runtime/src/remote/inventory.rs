//! 远端 Linux 安装库存：一次 SSH 探测、解析 HIT 行、选中路径。
//!
//! 禁止 `find` / `locate` / 扫盘。路径是真相。

use std::collections::HashSet;
use std::time::Duration;

use ncd_domain::{
    BackendType, DeploymentType, DiscoveredRemoteBot, DiscoveredRemoteBotSource,
    REMOTE_INVENTORY_VERSION, RemoteInventory, RemoteInventoryItem, RemoteInventoryKind,
    RemoteInventorySource, RemotePathOverrides, RemoteSelectedPaths,
};
use ncd_host::{Host, HostCommand};

use crate::components::action_policy::{RemoteHostProbe, RemoteLayout};

/// 探测命令超时
pub const INVENTORY_PROBE_TIMEOUT: Duration = Duration::from_secs(8);
/// 组件页打开时库存超过此时长则重探
pub const INVENTORY_STALE_AFTER: Duration = Duration::from_secs(6 * 3600);

/// SnowLuma 指纹：index.mjs 且（runtime.json 含 webuiPort 或存在 webui.json）
pub fn snowluma_fingerprint_ok(
    has_index_mjs: bool,
    runtime_json_has_webui_port: bool,
    has_webui_json: bool,
) -> bool {
    has_index_mjs && (runtime_json_has_webui_port || has_webui_json)
}

/// `~/.config/QQ` 是用户数据，不是安装根
pub fn is_qq_user_data_path(path: &str) -> bool {
    let n = path.replace('\\', "/");
    n.contains("/.config/QQ") || n.ends_with("/.config/QQ")
}

/// `{base}/opt/QQ/qq` → base（系统包为 `/`）
pub fn qq_install_base_from_qq_bin(qq_bin: &str) -> Option<String> {
    let p = qq_bin.trim_end_matches('/');
    let suffix = "/opt/QQ/qq";
    if let Some(prefix) = p.strip_suffix(suffix) {
        if prefix.is_empty() {
            return Some("/".into());
        }
        if is_qq_user_data_path(prefix) {
            return None;
        }
        return Some(prefix.to_string());
    }
    None
}

/// `{dir}/snowluma` → 父目录为 workspace，否则扁平布局 workspace = dir
pub fn snowluma_workspace_from_dir(snowluma_dir: &str) -> String {
    let p = snowluma_dir.trim_end_matches('/');
    if let Some(parent) = p.strip_suffix("/snowluma") {
        if !parent.is_empty() {
            return parent.to_string();
        }
    }
    p.to_string()
}

pub fn needs_sudo_for_qq(qq_install_base: Option<&str>, qq_bin: Option<&str>) -> bool {
    qq_install_base == Some("/") || qq_bin == Some("/opt/QQ/qq")
}

pub fn layout_from_selected(selected: &RemoteSelectedPaths) -> RemoteLayout {
    if selected.needs_sudo {
        RemoteLayout::System
    } else {
        RemoteLayout::Rootless
    }
}

pub fn probe_from_inventory(inv: &RemoteInventory) -> RemoteHostProbe {
    RemoteHostProbe {
        home: Some(inv.home.clone()),
        layout: layout_from_selected(&inv.selected),
    }
}

pub fn inventory_is_stale(probed_at: &str, now: chrono::DateTime<chrono::Utc>) -> bool {
    let Ok(then) = chrono::DateTime::parse_from_rfc3339(probed_at) else {
        return true;
    };
    let then = then.with_timezone(&chrono::Utc);
    now.signed_duration_since(then)
        .to_std()
        .map(|d| d > INVENTORY_STALE_AFTER)
        .unwrap_or(true)
}

/// 从探测脚本 stdout 构建库存
pub fn inventory_from_stdout(
    stdout: &str,
    previous: Option<&RemoteInventory>,
    probed_at: impl Into<String>,
) -> Result<RemoteInventory, String> {
    let (home, items) = parse_inventory_stdout(stdout)?;
    let items = dedupe_kind_root(items);
    let selected = select_paths(&home, &items, previous.map(|p| &p.selected));
    let bots = dedupe_discovered_bots(parse_bot_hits(stdout));
    Ok(RemoteInventory {
        v: REMOTE_INVENTORY_VERSION,
        probed_at: probed_at.into(),
        home,
        items,
        selected,
        bots,
    })
}

fn parse_bot_hits(stdout: &str) -> Vec<DiscoveredRemoteBot> {
    let mut bots = Vec::new();
    for raw in stdout.lines() {
        let line = raw.trim();
        let Some(rest) = line.strip_prefix("HIT ") else {
            continue;
        };
        if let Some(bot) = parse_bot_hit(rest) {
            bots.push(bot);
        }
    }
    bots
}

fn parse_bot_hit(rest: &str) -> Option<DiscoveredRemoteBot> {
    let mut kind = None;
    let mut backend = None;
    let mut deployment = None;
    let mut qq_id = None;
    let mut source = None;
    let mut docker_name = None;
    for token in rest.split_whitespace() {
        let Some((k, v)) = token.split_once('=') else {
            continue;
        };
        match k {
            "kind" => kind = Some(v),
            "backend" => {
                backend = match v {
                    "napcat" => Some(BackendType::NapCat),
                    "snowluma" => Some(BackendType::SnowLuma),
                    _ => None,
                }
            }
            "deployment" => {
                deployment = match v {
                    "native" => Some(DeploymentType::Native),
                    "docker" => Some(DeploymentType::Docker),
                    _ => None,
                }
            }
            "qq" => qq_id = v.parse().ok().filter(|n: &u64| *n > 0),
            "source" => {
                source = match v {
                    "configFile" => Some(DiscoveredRemoteBotSource::ConfigFile),
                    "runtimeStatus" => Some(DiscoveredRemoteBotSource::RuntimeStatus),
                    "dockerContainer" => Some(DiscoveredRemoteBotSource::DockerContainer),
                    _ => None,
                }
            }
            "dockerName" => docker_name = Some(v.to_string()),
            _ => {}
        }
    }
    if kind != Some("bot") {
        return None;
    }
    Some(DiscoveredRemoteBot {
        qq_id: qq_id?,
        backend: backend?,
        deployment: deployment?,
        source: source?,
        docker_name,
    })
}

fn dedupe_discovered_bots(bots: Vec<DiscoveredRemoteBot>) -> Vec<DiscoveredRemoteBot> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for bot in bots {
        let key = (bot.backend, bot.deployment, bot.qq_id);
        if seen.insert(key) {
            out.push(bot);
        }
    }
    out
}

pub fn parse_inventory_stdout(stdout: &str) -> Result<(String, Vec<RemoteInventoryItem>), String> {
    let mut home = String::new();
    let mut items = Vec::new();
    for raw in stdout.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix("NCD_INV_V=") {
            let _ = rest;
            continue;
        }
        if let Some(rest) = line.strip_prefix("HOME=") {
            home = rest.trim().to_string();
            continue;
        }
        if let Some(rest) = line.strip_prefix("HIT ") {
            if let Some(item) = parse_hit_line(rest) {
                if item.kind == RemoteInventoryKind::Qq && is_qq_user_data_path(&item.root) {
                    continue;
                }
                items.push(item);
            }
            continue;
        }
    }
    if home.is_empty() {
        return Err("无法探测远端 $HOME，请确认 SSH 用户家目录可用。".into());
    }
    Ok((home, items))
}

fn parse_hit_line(rest: &str) -> Option<RemoteInventoryItem> {
    let mut kind = None;
    let mut source = None;
    let mut verified = false;
    let mut root = String::new();
    let mut qq_bin = None;
    let mut napcat_mjs = None;
    let mut load_napcat_js = None;
    let mut index_mjs = None;
    let mut runtime_json = None;
    let mut node_bin = None;
    let mut docker_name = None;

    for token in rest.split_whitespace() {
        let Some((k, v)) = token.split_once('=') else {
            continue;
        };
        match k {
            "kind" => kind = parse_kind(v),
            "source" => source = parse_source(v),
            "verified" => verified = v == "1" || v.eq_ignore_ascii_case("true"),
            "root" => root = v.to_string(),
            "qqBin" => qq_bin = Some(v.to_string()),
            "napcatMjs" => napcat_mjs = Some(v.to_string()),
            "loadNapcatJs" => load_napcat_js = Some(v.to_string()),
            "indexMjs" => index_mjs = Some(v.to_string()),
            "runtimeJson" => runtime_json = Some(v.to_string()),
            "nodeBin" => node_bin = Some(v.to_string()),
            "dockerName" => docker_name = Some(v.to_string()),
            _ => {}
        }
    }
    let kind = kind?;
    let source = source?;
    if root.is_empty() {
        return None;
    }
    Some(RemoteInventoryItem {
        kind,
        root,
        source,
        verified,
        qq_bin,
        napcat_mjs,
        load_napcat_js,
        index_mjs,
        runtime_json,
        node_bin,
        docker_name,
    })
}

fn parse_kind(v: &str) -> Option<RemoteInventoryKind> {
    match v {
        "qq" => Some(RemoteInventoryKind::Qq),
        "napcat" => Some(RemoteInventoryKind::NapCat),
        "snowluma" => Some(RemoteInventoryKind::SnowLuma),
        "nodejs" => Some(RemoteInventoryKind::NodeJs),
        "ncd_watch" => Some(RemoteInventoryKind::NcdWatch),
        "docker_container" => Some(RemoteInventoryKind::DockerContainer),
        _ => None,
    }
}

fn parse_source(v: &str) -> Option<RemoteInventorySource> {
    match v {
        "userOverride" => Some(RemoteInventorySource::UserOverride),
        "desktopOwned" => Some(RemoteInventorySource::DesktopOwned),
        "officialInstaller" => Some(RemoteInventorySource::OfficialInstaller),
        "systemPackage" => Some(RemoteInventorySource::SystemPackage),
        "pathLookup" => Some(RemoteInventorySource::PathLookup),
        "process" => Some(RemoteInventorySource::Process),
        _ => None,
    }
}

fn dedupe_kind_root(items: Vec<RemoteInventoryItem>) -> Vec<RemoteInventoryItem> {
    let mut seen = HashSet::new();
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        let key = (item.kind, item.root.clone());
        if seen.insert(key) {
            out.push(item);
        }
    }
    out
}

fn source_rank(source: RemoteInventorySource) -> u8 {
    match source {
        RemoteInventorySource::UserOverride => 0,
        RemoteInventorySource::DesktopOwned => 2,
        RemoteInventorySource::OfficialInstaller => 3,
        RemoteInventorySource::SystemPackage => 4,
        RemoteInventorySource::PathLookup => 5,
        RemoteInventorySource::Process => 6,
    }
}

fn is_system_qq_root(root: &str) -> bool {
    root == "/" || root == "/opt/QQ" || root.ends_with("/opt/QQ") && root.starts_with("/opt/")
}

pub fn select_paths(
    home: &str,
    items: &[RemoteInventoryItem],
    previous: Option<&RemoteSelectedPaths>,
) -> RemoteSelectedPaths {
    let qq = pick(items, RemoteInventoryKind::Qq, previous, |p| {
        p.qq_install_base.as_deref()
    });
    let napcat = pick(items, RemoteInventoryKind::NapCat, previous, |p| {
        p.napcat_root.as_deref()
    });
    let snowluma = pick(items, RemoteInventoryKind::SnowLuma, previous, |p| {
        p.snowluma_dir.as_deref()
    });
    let node = pick(items, RemoteInventoryKind::NodeJs, previous, |p| {
        p.node_bin.as_deref()
    });
    let watch = pick(items, RemoteInventoryKind::NcdWatch, previous, |p| {
        p.ncd_watch_root.as_deref()
    });

    let qq_install_base = qq.as_ref().map(|i| i.root.clone());
    let qq_bin = qq
        .as_ref()
        .and_then(|i| i.qq_bin.clone())
        .or_else(|| qq_install_base.as_ref().map(|b| format!("{b}/opt/QQ/qq")));
    let napcat_root = napcat.as_ref().map(|i| i.root.clone());
    let snowluma_dir = snowluma.as_ref().map(|i| i.root.clone());
    let snowluma_workspace = snowluma_dir.as_deref().map(snowluma_workspace_from_dir);
    let node_bin = node
        .as_ref()
        .and_then(|i| i.node_bin.clone())
        .or_else(|| node.as_ref().map(|i| i.root.clone()));
    let ncd_watch_root = watch.as_ref().map(|i| i.root.clone());
    let needs_sudo = needs_sudo_for_qq(qq_install_base.as_deref(), qq_bin.as_deref());

    RemoteSelectedPaths {
        home: home.to_string(),
        qq_install_base,
        qq_bin,
        napcat_root,
        snowluma_dir,
        snowluma_workspace,
        node_bin,
        ncd_watch_root,
        needs_sudo,
    }
}

fn pick(
    items: &[RemoteInventoryItem],
    kind: RemoteInventoryKind,
    previous: Option<&RemoteSelectedPaths>,
    prev_root: impl Fn(&RemoteSelectedPaths) -> Option<&str>,
) -> Option<RemoteInventoryItem> {
    let candidates: Vec<&RemoteInventoryItem> = items
        .iter()
        .filter(|i| i.kind == kind && i.verified)
        .collect();
    if candidates.is_empty() {
        return None;
    }
    let prev = previous.and_then(&prev_root);
    let mut best = candidates[0];
    let mut best_rank = effective_rank(best, prev);
    for item in candidates.iter().skip(1) {
        let rank = effective_rank(item, prev);
        if rank < best_rank
            || (rank == best_rank
                && is_system_qq_root(&best.root)
                && !is_system_qq_root(&item.root))
        {
            best = item;
            best_rank = rank;
        }
    }
    Some(best.clone())
}

fn effective_rank(item: &RemoteInventoryItem, previous_root: Option<&str>) -> u8 {
    if previous_root == Some(item.root.as_str()) {
        return 1;
    }
    source_rank(item.source)
}

fn posix_safe(path: &str) -> bool {
    !path.is_empty()
        && path
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '.' | '_' | '-' | '+' | '~'))
}

/// 生成远端探测脚本（不含 find/locate）
pub fn build_probe_script(overrides: Option<&RemotePathOverrides>) -> String {
    let ov = overrides.cloned().unwrap_or_default();
    let emit_ov = |name: &str, val: Option<&str>| {
        val.filter(|p| posix_safe(p))
            .map(|p| format!("{name}='{p}'\n"))
            .unwrap_or_else(|| format!("{name}=''\n"))
    };
    let mut s = String::new();
    s.push_str(&emit_ov("OV_QQ_BASE", ov.qq_install_base.as_deref()));
    s.push_str(&emit_ov("OV_NAPCAT", ov.napcat_root.as_deref()));
    s.push_str(&emit_ov("OV_SL", ov.snowluma_dir.as_deref()));
    s.push_str(&emit_ov("OV_NODE", ov.node_bin.as_deref()));
    s.push_str(&emit_ov("OV_WATCH", ov.ncd_watch_root.as_deref()));
    s.push_str(PROBE_SCRIPT_BODY);
    s
}

const PROBE_SCRIPT_BODY: &str = r#"
echo NCD_INV_V=1
echo HOME="$HOME"

emit_qq() {
  base="$1"; src="$2"
  qq="$base/opt/QQ/qq"
  pkg="$base/opt/QQ/resources/app/package.json"
  if [ -x "$qq" ] && [ -f "$pkg" ]; then
    echo "HIT kind=qq source=$src verified=1 root=$base qqBin=$qq"
    app="$base/opt/QQ/resources/app"
    mjs="$app/app_launcher/napcat/napcat.mjs"
    load="$app/loadNapCat.js"
    if [ -f "$mjs" ] && [ -f "$load" ]; then
      echo "HIT kind=napcat source=$src verified=1 root=$app/app_launcher/napcat napcatMjs=$mjs loadNapcatJs=$load"
      emit_nc_bots "$app/app_launcher/napcat"
    fi
  fi
}

emit_sl() {
  dir="$1"; src="$2"
  if [ -f "$dir/index.mjs" ] && { [ -f "$dir/config/webui.json" ] || grep -q webuiPort "$dir/config/runtime.json" 2>/dev/null; }; then
    rt=""
    [ -f "$dir/config/runtime.json" ] && rt=" runtimeJson=$dir/config/runtime.json"
    echo "HIT kind=snowluma source=$src verified=1 root=$dir indexMjs=$dir/index.mjs$rt"
    if [ -x "$dir/node" ]; then
      emit_node "$dir/node" desktopOwned
    fi
    emit_sl_bots "$dir"
  fi
}

emit_nc_bots() {
  cfg="$1/config"
  [ -d "$cfg" ] || return 0
  for f in "$cfg"/onebot11_*.json "$cfg"/napcat_*.json; do
    [ -f "$f" ] || continue
    base=${f##*/}
    qq=${base#onebot11_}
    qq=${qq#napcat_}
    qq=${qq%.json}
    case "$qq" in
      *[!0-9]*|"") continue ;;
    esac
    echo "HIT kind=bot backend=napcat deployment=native qq=$qq source=configFile"
  done
}

emit_sl_bots() {
  dir="$1"
  cfg="$dir/config"
  if [ -d "$cfg" ]; then
    for f in "$cfg"/onebot_*.json; do
      [ -f "$f" ] || continue
      base=${f##*/}
      case "$base" in
        onebot11_*) continue ;;
      esac
      qq=${base#onebot_}
      qq=${qq%.json}
      case "$qq" in
        *[!0-9]*|"") continue ;;
      esac
      echo "HIT kind=bot backend=snowluma deployment=native qq=$qq source=configFile"
    done
  fi
  ws="$dir"
  case "$dir" in
    */snowluma) ws="${dir%/snowluma}" ;;
  esac
  rt="$ws/runtime"
  [ -d "$rt" ] || return 0
  for f in "$rt"/status_bot_*.json "$rt"/pid_bot_*; do
    [ -e "$f" ] || continue
    base=${f##*/}
    qq=${base#status_bot_}
    qq=${qq#pid_bot_}
    qq=${qq%.json}
    case "$qq" in
      *[!0-9]*|"") continue ;;
    esac
    echo "HIT kind=bot backend=snowluma deployment=native qq=$qq source=runtimeStatus"
  done
}

emit_node() {
  bin="$1"; src="$2"
  if [ -x "$bin" ]; then
    echo "HIT kind=nodejs source=$src verified=1 root=$bin nodeBin=$bin"
  fi
}

emit_watch() {
  root="$1"; src="$2"
  bin=""
  if [ -x "$root/ncd-watch" ]; then bin="$root/ncd-watch"
  elif [ -x "$root/bin/ncd-watch" ]; then bin="$root/bin/ncd-watch"
  fi
  if [ -n "$bin" ]; then
    echo "HIT kind=ncd_watch source=$src verified=1 root=$root"
  fi
}

if [ -n "$OV_QQ_BASE" ]; then emit_qq "$OV_QQ_BASE" userOverride; fi
if [ -n "$OV_NAPCAT" ] && [ -f "$OV_NAPCAT/napcat.mjs" ]; then
  load="$(dirname "$(dirname "$OV_NAPCAT")")/loadNapCat.js"
  ver=0
  [ -f "$load" ] && ver=1
  echo "HIT kind=napcat source=userOverride verified=$ver root=$OV_NAPCAT napcatMjs=$OV_NAPCAT/napcat.mjs loadNapcatJs=$load"
  emit_nc_bots "$OV_NAPCAT"
fi
if [ -n "$OV_SL" ]; then emit_sl "$OV_SL" userOverride; fi
if [ -n "$OV_NODE" ]; then emit_node "$OV_NODE" userOverride; fi
if [ -n "$OV_WATCH" ]; then emit_watch "$OV_WATCH" userOverride; fi

emit_qq "$HOME/Napcat" officialInstaller
emit_qq / systemPackage
emit_sl "$HOME/snowluma-remote/workspace/snowluma" desktopOwned
emit_sl "$HOME/Napcat/snowluma-workspace/snowluma" desktopOwned
emit_node "$HOME/snowluma-remote/workspace/node/bin/node" desktopOwned
emit_node "$HOME/Napcat/usr/node/bin/node" desktopOwned
emit_watch "$HOME/ncd-watch" desktopOwned

if command -v dpkg >/dev/null 2>&1; then
  dpkg -L linuxqq 2>/dev/null | while IFS= read -r p; do
    case "$p" in
      */opt/QQ/qq)
        if [ -x "$p" ]; then
          case "$p" in
            /opt/QQ/qq) emit_qq / systemPackage ;;
            *)
              base="${p%/opt/QQ/qq}"
              [ -n "$base" ] && emit_qq "$base" systemPackage
              ;;
          esac
        fi
        ;;
    esac
  done
elif command -v rpm >/dev/null 2>&1; then
  rpm -ql linuxqq 2>/dev/null | while IFS= read -r p; do
    case "$p" in
      */opt/QQ/qq)
        if [ -x "$p" ]; then
          case "$p" in
            /opt/QQ/qq) emit_qq / systemPackage ;;
            *)
              base="${p%/opt/QQ/qq}"
              [ -n "$base" ] && emit_qq "$base" systemPackage
              ;;
          esac
        fi
        ;;
    esac
  done
fi

if command -v node >/dev/null 2>&1; then
  nb=$(command -v node)
  emit_node "$nb" pathLookup
fi
if command -v qq >/dev/null 2>&1; then
  qb=$(command -v qq)
  case "$qb" in
    */opt/QQ/qq)
      base="${qb%/opt/QQ/qq}"
      [ -z "$base" ] && base=/
      emit_qq "$base" pathLookup
      ;;
  esac
fi

for d in /proc/[0-9]*; do
  [ -r "$d/cmdline" ] || continue
  cmd=$(tr '\0' ' ' < "$d/cmdline" 2>/dev/null) || continue
  case "$cmd" in
    *napcat.mjs*|*loadNapCat.js*)
      exe=$(readlink -f "$d/exe" 2>/dev/null || true)
      case "$exe" in
        */opt/QQ/qq)
          base="${exe%/opt/QQ/qq}"
          [ -z "$base" ] && base=/
          emit_qq "$base" process
          ;;
      esac
      cwd=$(readlink -f "$d/cwd" 2>/dev/null || true)
      if [ -n "$cwd" ]; then
        case "$cwd" in
          */opt/QQ) emit_qq "$(dirname "$cwd" | sed 's|/opt$||')" process ;;
        esac
      fi
      if [ -r "$d/environ" ]; then
        wd=$(tr '\0' '\n' < "$d/environ" 2>/dev/null | grep '^NAPCAT_WORKDIR=' | head -n 1 | sed 's/^NAPCAT_WORKDIR=//')
        if [ -n "$wd" ] && [ -f "$wd/napcat.mjs" ]; then
          echo "HIT kind=napcat source=process verified=1 root=$wd napcatMjs=$wd/napcat.mjs"
          emit_nc_bots "$wd"
        fi
      fi
      ;;
    *index.mjs*)
      cwd=$(readlink -f "$d/cwd" 2>/dev/null || true)
      [ -n "$cwd" ] && emit_sl "$cwd" process
      ;;
  esac
done

if command -v docker >/dev/null 2>&1; then
  docker ps -a --filter name=ncbot- --format '{{.Names}}' 2>/dev/null | while IFS= read -r name; do
    [ -n "$name" ] || continue
    echo "HIT kind=docker_container source=pathLookup verified=1 root=$name dockerName=$name"
    qq="${name#ncbot-}"
    case "$qq" in *[!0-9]*|"") continue ;; esac
    echo "HIT kind=bot backend=napcat deployment=docker qq=$qq source=dockerContainer dockerName=$name"
  done
  docker ps -a --filter name=slbot- --format '{{.Names}}' 2>/dev/null | while IFS= read -r name; do
    [ -n "$name" ] || continue
    echo "HIT kind=docker_container source=pathLookup verified=1 root=$name dockerName=$name"
    qq="${name#slbot-}"
    case "$qq" in *[!0-9]*|"") continue ;; esac
    echo "HIT kind=bot backend=snowluma deployment=docker qq=$qq source=dockerContainer dockerName=$name"
  done
fi
"#;

/// 在 Host 上跑探测脚本并选中路径
pub async fn probe_remote_inventory(
    host: &dyn Host,
    overrides: Option<&RemotePathOverrides>,
    previous: Option<&RemoteInventory>,
) -> Result<RemoteInventory, String> {
    let script = build_probe_script(overrides);
    let cmd = HostCommand::new("sh")
        .arg("-c")
        .arg(script)
        .timeout(INVENTORY_PROBE_TIMEOUT);
    let out = host
        .run_to_string(cmd)
        .await
        .map_err(|e| format!("探测远端安装库存失败: {e}"))?;
    if !out.success() {
        return Err(format!(
            "探测远端安装库存失败: exit={:?} stderr={}",
            out.exit_code,
            out.stderr.trim()
        ));
    }
    let probed_at = chrono::Utc::now().to_rfc3339();
    inventory_from_stdout(&out.stdout, previous, probed_at)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hit_qq(root: &str, source: &str) -> String {
        format!("HIT kind=qq source={source} verified=1 root={root} qqBin={root}/opt/QQ/qq")
    }

    fn sample_stdout(home: &str, extra: &str) -> String {
        format!("NCD_INV_V=1\nHOME={home}\n{extra}")
    }

    #[test]
    fn parse_hit_line_normal_and_unknown_keys() {
        let stdout = sample_stdout(
            "/home/u",
            "HIT kind=qq source=officialInstaller verified=1 root=/home/u/Napcat qqBin=/home/u/Napcat/opt/QQ/qq extra=ignored\n",
        );
        let (home, items) = parse_inventory_stdout(&stdout).unwrap();
        assert_eq!(home, "/home/u");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].kind, RemoteInventoryKind::Qq);
        assert_eq!(items[0].source, RemoteInventorySource::OfficialInstaller);
        assert_eq!(items[0].qq_bin.as_deref(), Some("/home/u/Napcat/opt/QQ/qq"));
    }

    #[test]
    fn parse_skips_non_hit_and_missing_kind() {
        let stdout = sample_stdout(
            "/home/u",
            "noise\nHIT source=process verified=1 root=/x\nHIT kind=qq source=nope verified=1 root=/y\n",
        );
        let (_, items) = parse_inventory_stdout(&stdout).unwrap();
        assert!(items.is_empty());
    }

    #[test]
    fn parse_empty_home_is_error() {
        let err =
            parse_inventory_stdout("NCD_INV_V=1\nHIT kind=qq source=process verified=1 root=/x\n")
                .unwrap_err();
        assert!(err.contains("$HOME"));
    }

    #[test]
    fn config_qq_is_not_install_root() {
        assert!(is_qq_user_data_path("/home/u/.config/QQ"));
        let stdout = sample_stdout(
            "/home/u",
            "HIT kind=qq source=process verified=1 root=/home/u/.config/QQ qqBin=/home/u/.config/QQ/opt/QQ/qq\n",
        );
        let (_, items) = parse_inventory_stdout(&stdout).unwrap();
        assert!(items.is_empty());
        assert!(qq_install_base_from_qq_bin("/home/u/.config/QQ/opt/QQ/qq").is_none());
        assert_eq!(
            qq_install_base_from_qq_bin("/opt/QQ/qq").as_deref(),
            Some("/")
        );
        assert_eq!(
            qq_install_base_from_qq_bin("/home/u/Napcat/opt/QQ/qq").as_deref(),
            Some("/home/u/Napcat")
        );
    }

    #[test]
    fn snowluma_fingerprint_requires_config() {
        assert!(!snowluma_fingerprint_ok(true, false, false));
        assert!(snowluma_fingerprint_ok(true, true, false));
        assert!(snowluma_fingerprint_ok(true, false, true));
        assert!(!snowluma_fingerprint_ok(false, true, true));
    }

    #[test]
    fn select_prefers_override_then_previous_then_rootless() {
        let items = vec![
            parse_hit_line("kind=qq source=systemPackage verified=1 root=/ qqBin=/opt/QQ/qq").unwrap(),
            parse_hit_line(
                "kind=qq source=officialInstaller verified=1 root=/home/u/Napcat qqBin=/home/u/Napcat/opt/QQ/qq",
            )
            .unwrap(),
            parse_hit_line(
                "kind=qq source=userOverride verified=1 root=/data/qq qqBin=/data/qq/opt/QQ/qq",
            )
            .unwrap(),
        ];
        let sel = select_paths("/home/u", &items, None);
        assert_eq!(sel.qq_install_base.as_deref(), Some("/data/qq"));
        assert!(!sel.needs_sudo);

        let without_ov: Vec<_> = items
            .into_iter()
            .filter(|i| i.source != RemoteInventorySource::UserOverride)
            .collect();
        let sel = select_paths("/home/u", &without_ov, None);
        assert_eq!(sel.qq_install_base.as_deref(), Some("/home/u/Napcat"));
        assert!(!sel.needs_sudo);

        let prev = RemoteSelectedPaths {
            home: "/home/u".into(),
            qq_install_base: Some("/".into()),
            qq_bin: Some("/opt/QQ/qq".into()),
            ..RemoteSelectedPaths::default()
        };
        let sel = select_paths("/home/u", &without_ov, Some(&prev));
        assert_eq!(sel.qq_install_base.as_deref(), Some("/"));
        assert!(sel.needs_sudo);
    }

    #[test]
    fn process_fills_custom_prefix_when_whitelist_empty() {
        let items =
            vec![parse_hit_line(
            "kind=snowluma source=process verified=1 root=/data/sl indexMjs=/data/sl/index.mjs",
        )
        .unwrap()];
        let sel = select_paths("/home/u", &items, None);
        assert_eq!(sel.snowluma_dir.as_deref(), Some("/data/sl"));
        assert_eq!(sel.snowluma_workspace.as_deref(), Some("/data/sl"));
    }

    #[test]
    fn workspace_parent_when_dir_ends_with_snowluma() {
        assert_eq!(
            snowluma_workspace_from_dir("/home/u/snowluma-remote/workspace/snowluma"),
            "/home/u/snowluma-remote/workspace"
        );
    }

    #[test]
    fn needs_sudo_only_for_system_qq() {
        assert!(needs_sudo_for_qq(Some("/"), Some("/opt/QQ/qq")));
        assert!(!needs_sudo_for_qq(
            Some("/home/u/Napcat"),
            Some("/home/u/Napcat/opt/QQ/qq")
        ));
        assert!(!needs_sudo_for_qq(None, None));
        let sel = RemoteSelectedPaths {
            needs_sudo: true,
            ..RemoteSelectedPaths::default()
        };
        assert_eq!(layout_from_selected(&sel), RemoteLayout::System);
    }

    #[test]
    fn inventory_from_stdout_whitelist_dpkg_and_process() {
        let stdout = sample_stdout(
            "/home/u",
            &(hit_qq("/home/u/Napcat", "officialInstaller")
                + "\n"
                + &hit_qq("/", "systemPackage")
                + "\nHIT kind=nodejs source=pathLookup verified=1 root=/usr/bin/node nodeBin=/usr/bin/node\nHIT kind=snowluma source=process verified=1 root=/data/sl indexMjs=/data/sl/index.mjs\nHIT kind=docker_container source=pathLookup verified=1 root=ncbot-1001 dockerName=ncbot-1001\n"),
        );
        let inv = inventory_from_stdout(&stdout, None, "2026-08-25T00:00:00Z").unwrap();
        assert_eq!(
            inv.selected.qq_install_base.as_deref(),
            Some("/home/u/Napcat")
        );
        assert_eq!(inv.selected.node_bin.as_deref(), Some("/usr/bin/node"));
        assert_eq!(inv.selected.snowluma_dir.as_deref(), Some("/data/sl"));
        assert!(
            inv.items
                .iter()
                .any(|i| i.kind == RemoteInventoryKind::DockerContainer)
        );
        assert!(inv.selected.ncd_watch_root.is_none());
    }

    #[test]
    fn inventory_parses_bot_hits_without_putting_them_in_items() {
        let stdout = sample_stdout(
            "/home/u",
            "HIT kind=bot backend=napcat deployment=native qq=10001 source=configFile\nHIT kind=bot backend=snowluma deployment=native qq=10001 source=configFile\nHIT kind=bot backend=napcat deployment=docker qq=20002 source=dockerContainer dockerName=ncbot-20002\nHIT kind=bot backend=napcat deployment=native qq=10001 source=runtimeStatus\nHIT kind=qq source=officialInstaller verified=1 root=/home/u/Napcat qqBin=/home/u/Napcat/opt/QQ/qq\n",
        );
        let inv = inventory_from_stdout(&stdout, None, "2026-08-25T00:00:00Z").unwrap();
        assert_eq!(inv.items.len(), 1);
        assert_eq!(inv.bots.len(), 3);
        assert_eq!(inv.bots[0].qq_id, 10001);
        assert_eq!(inv.bots[0].backend, BackendType::NapCat);
        assert_eq!(inv.bots[0].source, DiscoveredRemoteBotSource::ConfigFile);
        assert_eq!(inv.bots[1].backend, BackendType::SnowLuma);
        assert_eq!(inv.bots[2].qq_id, 20002);
        assert_eq!(inv.bots[2].docker_name.as_deref(), Some("ncbot-20002"));
    }

    #[test]
    fn probe_script_has_fingerprints_not_find() {
        let script = build_probe_script(None);
        assert!(!script.contains("find /"));
        assert!(!script.contains("locate "));
        assert!(script.contains("$HOME/Napcat"));
        assert!(script.contains("/opt/QQ"));
        assert!(script.contains("snowluma-remote/workspace/snowluma"));
        assert!(script.contains("napcat.mjs"));
        assert!(script.contains("webuiPort"));
        assert!(script.contains("NAPCAT_WORKDIR"));
        assert!(script.contains("$dir/node"));
        assert!(script.contains("onebot11_"));
        assert!(script.contains("onebot_"));
        assert!(script.contains("kind=bot"));
        assert!(script.contains("slbot-"));
        assert!(!script.contains("SNOWLUMA_WEBUI_BOOTSTRAP_PASSWORD"));
        assert!(!script.contains("WEBUI_TOKEN"));
        let with_ov = build_probe_script(Some(&RemotePathOverrides {
            qq_install_base: Some("/data/qq".into()),
            ..RemotePathOverrides::default()
        }));
        assert!(with_ov.contains("OV_QQ_BASE='/data/qq'"));
    }

    #[test]
    fn stale_when_unparseable_or_old() {
        let now = chrono::DateTime::parse_from_rfc3339("2026-08-25T12:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        assert!(inventory_is_stale("nope", now));
        assert!(!inventory_is_stale("2026-08-25T10:00:00Z", now));
        assert!(inventory_is_stale("2026-08-24T12:00:00Z", now));
    }

    struct ScriptedHost {
        stdout: String,
    }

    #[async_trait::async_trait]
    impl Host for ScriptedHost {
        fn os(&self) -> ncd_host::Os {
            ncd_host::Os::Linux
        }
        fn arch(&self) -> ncd_host::Arch {
            ncd_host::Arch::X86_64
        }
        fn locality(&self) -> ncd_host::Locality {
            ncd_host::Locality::Remote
        }
        fn id(&self) -> &str {
            "scripted"
        }
        fn shell(&self) -> &dyn ncd_host::HostShell {
            &ncd_host::shell::BashShell
        }
        fn pkg_manager(&self) -> Option<&dyn ncd_host::PackageManager> {
            None
        }
        async fn read_file(
            &self,
            _: &ncd_host::HostPath,
        ) -> Result<bytes::Bytes, ncd_host::HostError> {
            Err(ncd_host::HostError::Unsupported { operation: "stub" })
        }
        async fn write_file(
            &self,
            _: &ncd_host::HostPath,
            _: &[u8],
        ) -> Result<(), ncd_host::HostError> {
            Err(ncd_host::HostError::Unsupported { operation: "stub" })
        }
        async fn list_dir(
            &self,
            _: &ncd_host::HostPath,
        ) -> Result<Vec<ncd_host::DirEntry>, ncd_host::HostError> {
            Err(ncd_host::HostError::Unsupported { operation: "stub" })
        }
        async fn create_dir_all(&self, _: &ncd_host::HostPath) -> Result<(), ncd_host::HostError> {
            Err(ncd_host::HostError::Unsupported { operation: "stub" })
        }
        async fn remove_file(&self, _: &ncd_host::HostPath) -> Result<(), ncd_host::HostError> {
            Err(ncd_host::HostError::Unsupported { operation: "stub" })
        }
        async fn remove_dir_all(&self, _: &ncd_host::HostPath) -> Result<(), ncd_host::HostError> {
            Err(ncd_host::HostError::Unsupported { operation: "stub" })
        }
        async fn exists(&self, _: &ncd_host::HostPath) -> Result<bool, ncd_host::HostError> {
            Err(ncd_host::HostError::Unsupported { operation: "stub" })
        }
        async fn upload(
            &self,
            _: &std::path::Path,
            _: &ncd_host::HostPath,
        ) -> Result<(), ncd_host::HostError> {
            Err(ncd_host::HostError::Unsupported { operation: "stub" })
        }
        async fn download(
            &self,
            _: &ncd_host::HostPath,
            _: &std::path::Path,
        ) -> Result<(), ncd_host::HostError> {
            Err(ncd_host::HostError::Unsupported { operation: "stub" })
        }
        async fn extract_archive(
            &self,
            _: &ncd_host::HostPath,
            _: &ncd_host::HostPath,
            _: ncd_host::ArchiveKind,
        ) -> Result<(), ncd_host::HostError> {
            Err(ncd_host::HostError::Unsupported { operation: "stub" })
        }
        async fn spawn(
            &self,
            _: HostCommand,
        ) -> Result<Box<dyn ncd_host::HostProcess>, ncd_host::HostError> {
            Err(ncd_host::HostError::Unsupported { operation: "stub" })
        }
        async fn run_to_string(
            &self,
            cmd: HostCommand,
        ) -> Result<ncd_host::CommandOutput, ncd_host::HostError> {
            assert_eq!(cmd.program, "sh");
            assert_eq!(cmd.timeout, Some(INVENTORY_PROBE_TIMEOUT));
            Ok(ncd_host::CommandOutput {
                exit_code: Some(0),
                stdout: self.stdout.clone(),
                stderr: String::new(),
            })
        }
    }

    #[tokio::test]
    async fn probe_remote_inventory_parses_scripted_host_stdout() {
        let stdout = sample_stdout(
            "/home/u",
            "HIT kind=qq source=officialInstaller verified=1 root=/home/u/Napcat qqBin=/home/u/Napcat/opt/QQ/qq\nHIT kind=snowluma source=process verified=1 root=/data/sl indexMjs=/data/sl/index.mjs\n",
        );
        let host = ScriptedHost { stdout };
        let inv = probe_remote_inventory(&host, None, None).await.unwrap();
        assert_eq!(inv.home, "/home/u");
        assert_eq!(
            inv.selected.qq_install_base.as_deref(),
            Some("/home/u/Napcat")
        );
        assert_eq!(inv.selected.snowluma_dir.as_deref(), Some("/data/sl"));
    }
}
