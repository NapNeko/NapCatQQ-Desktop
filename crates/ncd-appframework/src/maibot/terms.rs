//! 启动前核对 EULA / 隐私条款：上游要求 md5(原文) 等于 `eula.confirmed` / `privacy.confirmed` 的内容，
//! 否则 Worker 卡在 `input()` 等「同意」，没有 stdin 就直接退出。
//!
//! 上游用文本模式读原文（换行统一成 `\n`）再算 md5，这里照做：本机开了 autocrlf 的检出是 CRLF，
//! 按原始字节算永远对不上。确认文件上游是整文件读出来原样比对的，写的时候不能带换行。

use ncd_domain::AppPendingTerms;
use ncd_host::{Host, HostPath};
use ncd_traits::AppFrameworkError;

use super::manifest::{
    EULA_CONFIRMED, EULA_MD, EULA_URL, PRIVACY_CONFIRMED, PRIVACY_MD, PRIVACY_URL,
};

struct TermsFile {
    id: &'static str,
    title: &'static str,
    url: &'static str,
    doc: &'static str,
    confirmed: &'static str,
}

const TERMS: [TermsFile; 2] = [
    TermsFile {
        id: "eula",
        title: "MaiBot 最终用户许可协议",
        url: EULA_URL,
        doc: EULA_MD,
        confirmed: EULA_CONFIRMED,
    },
    TermsFile {
        id: "privacy",
        title: "MaiBot 用户隐私条款",
        url: PRIVACY_URL,
        doc: PRIVACY_MD,
        confirmed: PRIVACY_CONFIRMED,
    },
];

/// Python 文本模式的换行规则：`\r\n` 和单独的 `\r` 都算 `\n`
pub fn normalize_newlines(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

pub fn terms_hash(text: &str) -> String {
    format!("{:x}", md5::compute(normalize_newlines(text).as_bytes()))
}

fn host_err(e: ncd_host::HostError) -> AppFrameworkError {
    AppFrameworkError::Host(e.to_string())
}

async fn read_text(host: &dyn Host, path: &HostPath) -> Result<Option<String>, AppFrameworkError> {
    if !host.exists(path).await.map_err(host_err)? {
        return Ok(None);
    }
    let bytes = host.read_file(path).await.map_err(host_err)?;
    Ok(Some(String::from_utf8_lossy(&bytes).into_owned()))
}

/// 还没同意、或更新后原文变了的条款。原文缺失的跳过：那是装坏了，上游启动会自己报
pub async fn pending_terms(
    host: &dyn Host,
    install_dir: &HostPath,
) -> Result<Vec<AppPendingTerms>, AppFrameworkError> {
    let mut out = Vec::new();
    for t in &TERMS {
        let Some(text) = read_text(host, &install_dir.join(t.doc)).await? else {
            continue;
        };
        let confirmed = read_text(host, &install_dir.join(t.confirmed)).await?;
        if confirmed.as_deref() == Some(terms_hash(&text).as_str()) {
            continue;
        }
        out.push(AppPendingTerms {
            id: t.id.to_string(),
            title: t.title.to_string(),
            url: t.url.to_string(),
            text,
        });
    }
    Ok(out)
}

/// 把实例目录里现在这版条款记成已同意。`only_missing` 为真时已有确认文件的不动：
/// 首装写确认（用户在新建对话框里同意过），更新时绝不替用户把新条款标成已同意
pub async fn write_confirmations(
    host: &dyn Host,
    install_dir: &HostPath,
    only_missing: bool,
) -> Result<(), AppFrameworkError> {
    for t in &TERMS {
        let confirmed = install_dir.join(t.confirmed);
        if only_missing && host.exists(&confirmed).await.map_err(host_err)? {
            continue;
        }
        let Some(text) = read_text(host, &install_dir.join(t.doc)).await? else {
            return Err(AppFrameworkError::Validation(format!(
                "实例目录缺少 {}，重新安装可修复",
                t.doc
            )));
        };
        host.write_file(&confirmed, terms_hash(&text).as_bytes())
            .await
            .map_err(host_err)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crlf_and_lf_checkouts_hash_the_same() {
        let lf = "# 协议\n\n第一条\n";
        assert_eq!(terms_hash(lf), terms_hash("# 协议\r\n\r\n第一条\r\n"));
        assert_eq!(terms_hash("a\n"), format!("{:x}", md5::compute(b"a\n")));
        assert_ne!(terms_hash(lf), terms_hash("# 协议\n\n第二条\n"));
    }

    #[test]
    fn lone_cr_counts_as_newline() {
        assert_eq!(normalize_newlines("a\rb\r\nc"), "a\nb\nc");
    }
}
