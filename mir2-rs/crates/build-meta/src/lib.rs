//! 构建元数据注入（卡 `openmir2-rust-build-rev-embedding`）。
//!
//! 每个 bin 的 `build.rs` 调 [`emit`]：
//! - `MIR2_BUILD_REV`：`git rev-parse HEAD` 的**完整 40 位** hex；取不到或不是 40 hex ⇒ `unknown`；
//! - `MIR2_BUILD_DIRTY`：`git status --porcelain` 的非空行数；取不到 ⇒ `unknown`；
//! - `MIR2_BUILD_AT`：构建时刻（UTC RFC3339，秒精度）。
//!
//! 为什么不让构建脚本自行"按目录猜"：`sha256` 会随构建路径变化（同一源码、同一工具链、
//! 同大小、不同 sha 已实测），**rev 才是跨机器/跨目录的稳定身份**；自报 rev 必须来自 git，
//! 取不到就明说 `unknown`，**不许写空串**（空串会被 `starts_with("")` 之类的守卫静默放过）。

use std::process::Command;

/// 跑 git 并返回 trim 后的 stdout；**命令失败才返回 `None`**。
///
/// 空输出是合法结果（`git status --porcelain` 在干净树就是空）：把它折叠成 `None` 会让
/// "干净" 与 "取不到" 不可区分——实测曾因此把 `dirty=0` 写成 `dirty=unknown`。
fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// 40 位 hex 校验：git 对象名只接受完整 sha1；短号/脏串一律回落 `unknown`。
fn is_full_hex40(s: &str) -> bool {
    s.len() == 40 && s.chars().all(|c| c.is_ascii_hexdigit())
}

/// 供各 bin 的 `build.rs` 调用：打印 `cargo:rustc-env=…` 与重跑触发。
pub fn emit() {
    let rev = git(&["rev-parse", "HEAD"])
        .filter(|s| is_full_hex40(s))
        .unwrap_or_else(|| "unknown".to_string());
    let dirty = match git(&["status", "--porcelain"]) {
        Some(text) => text
            .lines()
            .filter(|line| !line.trim().is_empty())
            .count()
            .to_string(),
        None => "unknown".to_string(),
    };
    let built_at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);

    println!("cargo:rustc-env=MIR2_BUILD_REV={rev}");
    println!("cargo:rustc-env=MIR2_BUILD_DIRTY={dirty}");
    println!("cargo:rustc-env=MIR2_BUILD_AT={built_at}");

    // 重跑触发：HEAD/index 变化（提交、切分支、暂存）与显式环境覆盖。
    if let Some(gitdir) = git(&["rev-parse", "--absolute-git-dir"]) {
        println!("cargo:rerun-if-changed={gitdir}/HEAD");
        println!("cargo:rerun-if-changed={gitdir}/index");
    }
    // 包内文件变化也重跑（只发 .git 触发会丢掉 Cargo 默认的"包内任何文件"语义）——
    // dirty 是**构建时**快照，源文件改了就必须重新记录，否则脏标记会停留在旧值。
    println!("cargo:rerun-if-changed=.");
    println!("cargo:rerun-if-env-changed=MIR2_BUILD_REV");
}

#[cfg(test)]
mod tests {
    use super::is_full_hex40;

    #[test]
    fn full_hex40_accepts_only_complete_object_names() {
        assert!(is_full_hex40("0123456789abcdef0123456789abcdef01234567"));
        assert!(is_full_hex40(&"A".repeat(40)));
        // 短号 / 长号 / 非 hex / 空串一律拒绝（空串会被 starts_with("") 之类静默放过）。
        assert!(!is_full_hex40("0123456"));
        assert!(!is_full_hex40(&"a".repeat(39)));
        assert!(!is_full_hex40(&"a".repeat(41)));
        assert!(!is_full_hex40("0123456789abcdef0123456789abcdef0123456z"));
        assert!(!is_full_hex40(""));
    }
}
