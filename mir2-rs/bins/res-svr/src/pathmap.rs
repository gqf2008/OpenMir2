//! 请求路径的归一化与根内包含检查（卡 A4 的安全承重件）。
//!
//! 入口是 `/res/file/` 之后的**原始（仍百分号编码）**路径。算法：
//!
//! 1. 百分号解码（`%2e%2e` ⇒ `..`）；解码失败/非 UTF-8 ⇒ 403；
//! 2. 拒绝控制字符（含 `NUL`）、拒绝任何空组件（绝对路径 `/etc/passwd`、UNC `//host/share`、
//!    双斜杠、尾斜杠都因此被拒）；
//! 3. `\` 视为分隔符（Windows 口径），统一成 `/` 后再切分；
//! 4. 每段（**含第一段**）逐段校验：非空、不是 `.`/`..`、不以点/空格结尾（Windows 会静默
//!    剥掉尾点尾空格）、不含 `<>:"|?*` 与控制字符、不是 Windows 保留设备名（`CON`/`NUL`/`COM1`…）；
//!    盘符形态的 `C:` 首段在这一步就被 403（不是「前缀没命中」的 404）；
//! 5. 第一段必须**大小写不敏感**命中某个映射前缀，命中不了 ⇒ 404（没有该资产面）；
//! 6. 末段扩展名必须命中白名单（否则 403 —— 白名单是策略拒绝，不是「不存在」）；
//! 7. 拼到映射根后 `canonicalize`（解析符号链接），**必须仍以该根的 canonical 路径为前缀**
//!    （否则 403：`..` 逃逸、符号链逃逸都在这里兜底）；不存在/不是普通文件 ⇒ 404。
//!
//! 顺序是刻意的：`Data/../Data/a.wzl` 这类「逃出去又回来」的路径在**第 5 步**就 403，
//! 而不是靠第 7 步的包含检查兜底——两处判据互为纵深（M2 变异分别钉住）。

use std::path::PathBuf;

use percent_encoding::percent_decode_str;

use crate::config::RootMap;

/// 路径裁决结果：策略拒绝（403）或不存在（404）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolveError {
    /// 归一化/包含检查/白名单拒绝 ⇒ 调用方回 403。
    Rejected(&'static str),
    /// 映射前缀不存在或目标文件不存在 ⇒ 调用方回 404。
    NotFound,
}

/// 通过校验的目标文件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    /// canonicalize 后的真实路径（已证明在该映射根内）。
    pub path: PathBuf,
    /// 文件大小（字节）。
    pub size: u64,
    /// 对外的相对路径（`<前缀>/<相对段>`，供日志/测试使用）。
    pub display_path: String,
}

/// Windows 保留设备名（不区分大小写；带扩展名形式如 `NUL.txt` 同样保留）。
const RESERVED_DEVICE_NAMES: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "CONIN$", "CONOUT$", "COM1", "COM2", "COM3", "COM4", "COM5",
    "COM6", "COM7", "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8",
    "LPT9",
];

/// Windows 文件名非法字符（`\` 与 `/` 已在切分阶段处理，`:` 拦住盘符与 NTFS ADS）。
const INVALID_PATH_CHARS: &[char] = &['<', '>', ':', '"', '|', '?', '*'];

/// 解析并校验 `/res/file/` 之后的原始路径。
///
/// # Errors
/// [`ResolveError::Rejected`]（403）或 [`ResolveError::NotFound`]（404），理由见模块文档。
pub fn resolve(
    roots: &[RootMap],
    allowed_exts: &[String],
    raw_tail: &str,
) -> Result<Resolved, ResolveError> {
    if raw_tail.is_empty() {
        return Err(ResolveError::NotFound);
    }
    let decoded = percent_decode_str(raw_tail)
        .decode_utf8()
        .map_err(|_| ResolveError::Rejected("URL 百分号编码非法"))?;
    if decoded.chars().any(char::is_control) {
        return Err(ResolveError::Rejected("路径含控制字符"));
    }
    let normalized = decoded.replace('\\', "/");
    let parts: Vec<&str> = normalized.split('/').collect();
    if parts.iter().any(|part| part.is_empty()) {
        // 绝对路径（首段为空）、UNC（前两段为空）、双斜杠、尾斜杠都落在这里。
        return Err(ResolveError::Rejected("路径含空组件（绝对路径/UNC/空段）"));
    }
    // 先逐段校验（含首段）：盘符/ADS 形态的 `C:...` 首段也必须在归一化层被 403，
    // 而不是因为「前缀没命中」落到 404。
    for part in &parts {
        check_component(part)?;
    }
    let prefix = parts[0];
    let Some(root) = roots
        .iter()
        .find(|map| map.prefix.eq_ignore_ascii_case(prefix))
    else {
        return Err(ResolveError::NotFound);
    };
    let Some(file_name) = parts.last() else {
        return Err(ResolveError::NotFound);
    };
    let Some((_, extension)) = file_name.rsplit_once('.') else {
        return Err(ResolveError::Rejected("扩展名不在白名单"));
    };
    let extension = format!(".{}", extension.to_ascii_lowercase());
    if !allowed_exts.iter().any(|allowed| allowed == &extension) {
        return Err(ResolveError::Rejected("扩展名不在白名单"));
    }

    let mut joined = root.root.clone();
    for part in &parts[1..] {
        joined.push(part);
    }
    let canonical = std::fs::canonicalize(&joined).map_err(|_| ResolveError::NotFound)?;
    if !canonical.starts_with(&root.root) {
        return Err(ResolveError::Rejected("路径越出映射根（符号链逃逸）"));
    }
    let metadata = std::fs::metadata(&canonical).map_err(|_| ResolveError::NotFound)?;
    if !metadata.is_file() {
        return Err(ResolveError::NotFound);
    }
    Ok(Resolved {
        path: canonical,
        size: metadata.len(),
        display_path: format!("{}/{}", root.prefix, parts[1..].join("/")),
    })
}

fn check_component(component: &str) -> Result<(), ResolveError> {
    if component.is_empty() || component == "." || component == ".." {
        return Err(ResolveError::Rejected("路径含 . / .. 组件"));
    }
    if component.ends_with('.') || component.ends_with(' ') {
        return Err(ResolveError::Rejected("路径组件以点或空格结尾"));
    }
    if component
        .chars()
        .any(|c| c.is_control() || INVALID_PATH_CHARS.contains(&c))
    {
        return Err(ResolveError::Rejected("路径含非法字符"));
    }
    let stem = component
        .split('.')
        .next()
        .unwrap_or(component)
        .to_ascii_uppercase();
    if RESERVED_DEVICE_NAMES.contains(&stem.as_str()) {
        return Err(ResolveError::Rejected("Windows 保留设备名"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (crate::testutil::TempDir, Vec<RootMap>) {
        let dir = crate::testutil::TempDir::new("pathmap");
        let data = dir.dir("Data");
        let map = dir.dir("Map");
        dir.file("Data/a.wzl", b"abc");
        dir.file("Data/sub/b.wzx", b"de");
        dir.file("Map/m.map", b"m");
        let roots = vec![
            RootMap {
                prefix: "Data".to_string(),
                root: std::fs::canonicalize(&data).expect("canonicalize"),
            },
            RootMap {
                prefix: "Map".to_string(),
                root: std::fs::canonicalize(&map).expect("canonicalize"),
            },
        ];
        (dir, roots)
    }

    fn exts() -> Vec<String> {
        vec![".wzl".to_string(), ".wzx".to_string(), ".map".to_string()]
    }

    #[test]
    fn accepts_files_inside_roots() {
        let (_dir, roots) = fixture();
        let resolved = resolve(&roots, &exts(), "Data/a.wzl").expect("应通过");
        assert_eq!(resolved.size, 3);
        assert_eq!(resolved.display_path, "Data/a.wzl");
        let resolved = resolve(&roots, &exts(), "Data/sub/b.wzx").expect("子目录应通过");
        assert_eq!(resolved.display_path, "Data/sub/b.wzx");
        let resolved = resolve(&roots, &exts(), "data/A.WZL").expect("前缀/文件名大小写不敏感");
        assert_eq!(resolved.display_path, "Data/A.WZL");
        let resolved = resolve(&roots, &exts(), "map/M.MAP").expect("前缀/扩展名大小写不敏感");
        assert_eq!(resolved.size, 1);
    }

    #[test]
    fn rejects_traversal_and_absolute_forms() {
        let (_dir, roots) = fixture();
        let hostile = [
            "Data/../../GameSrv.exe",
            "Data/../Data/a.wzl",
            "Data/%2e%2e/Data/a.wzl",
            "Data/%2e%2e/%2e%2e/GameSrv.exe",
            "Data/..%2f..%2fGameSrv.exe",
            "Data\\..\\..\\GameSrv.exe",
            "/etc/passwd",
            "//host/share",
            "C:/Windows/win.ini",
            "C:\\Windows\\win.ini",
            "Data/a.wzl. ",
            "Data/a.wzl%20",
            "Data/a.wzl.",
            "NUL",
            "Data/NUL.wzl",
            "Data/CON",
            "Data/a.wzl%00.txt",
            "Data/a.wzl%0a.txt",
            "Data/a.wzl:ads",
            "Data/a.wzl*",
            "Data/./a.wzl",
            "Data//a.wzl",
            "Data/a.wzl/",
        ];
        for input in hostile {
            let outcome = resolve(&roots, &exts(), input);
            assert!(
                outcome.is_err(),
                "恶意路径必须被拒：{input:?} ⇒ {outcome:?}"
            );
            assert!(
                !matches!(outcome, Err(ResolveError::NotFound)),
                "恶意路径不能归为 404：{input:?}"
            );
        }
    }

    #[test]
    fn not_found_versus_rejected() {
        let (_dir, roots) = fixture();
        assert_eq!(
            resolve(&roots, &exts(), "Other/a.wzl"),
            Err(ResolveError::NotFound),
            "未知前缀 ⇒ 404"
        );
        assert_eq!(
            resolve(&roots, &exts(), "Data/missing.wzl"),
            Err(ResolveError::NotFound),
            "不存在 ⇒ 404"
        );
        assert!(
            matches!(
                resolve(&roots, &exts(), "Data/sub"),
                Err(ResolveError::Rejected(_))
            ),
            "无扩展名的路径由白名单策略直接拒绝（403）"
        );
        assert_eq!(
            resolve(&roots, &exts(), ""),
            Err(ResolveError::NotFound),
            "空路径 ⇒ 404"
        );
        assert!(matches!(
            resolve(&roots, &exts(), "Data/a.exe"),
            Err(ResolveError::Rejected(_))
        ));
        assert!(matches!(
            resolve(&roots, &exts(), "Data/noext"),
            Err(ResolveError::Rejected(_))
        ));
    }

    #[test]
    fn percent_encoded_slashes_are_treated_as_separators() {
        let (_dir, roots) = fixture();
        // %2f 解码成 '/' 后仍是「Data/a.wzl」⇒ 正常通过（不是绕过）。
        let resolved = resolve(&roots, &exts(), "Data%2fa.wzl").expect("编码斜杠等于分隔符");
        assert_eq!(resolved.display_path, "Data/a.wzl");
        // 但 %2f 拼出的 `..` 仍被第 5 步拒绝。
        assert!(resolve(&roots, &exts(), "Data%2f%2e%2e%2fData%2fa.wzl").is_err());
    }

    #[cfg(windows)]
    #[test]
    fn symlink_escape_is_rejected_when_creatable() {
        let dir = crate::testutil::TempDir::new("pathmap-symlink");
        let root = dir.dir("Data");
        dir.file("Data/real.wzl", b"real");
        let outside = dir.file("outside/secret.wzl", b"secret");
        let link = root.join("link.wzl");
        match std::os::windows::fs::symlink_file(&outside, &link) {
            Ok(()) => {
                let roots = vec![RootMap {
                    prefix: "Data".to_string(),
                    root: std::fs::canonicalize(&root).expect("canonicalize"),
                }];
                assert!(
                    matches!(
                        resolve(&roots, &exts(), "Data/link.wzl"),
                        Err(ResolveError::Rejected(_))
                    ),
                    "指向根外的符号链接必须 403"
                );
            }
            Err(error) => {
                eprintln!("跳过符号链用例：本机无法创建符号链接（{error}）");
            }
        }
    }
}
