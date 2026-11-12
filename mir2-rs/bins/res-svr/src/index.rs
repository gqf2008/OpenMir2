//! 资产清单（`/res/index.json`）与 `sha256` 缓存（卡 A3）。
//!
//! * 遍历每个映射根，按扩展白名单收集文件；输出 `{prefix}/{相对路径}`（`/` 分隔），
//!   按路径排序（同输入同输出，便于对拍与缓存诊断）。
//! * 哈希缓存键是**文件自身路径**，值是 `(size, mtime) → sha256`：任一变化即重算
//!   （内容改大小、内容同大小但 mtime 变，都会失效）。
//! * 遍历是元数据级操作（不读内容），哈希只在缓存失效时发生 ⇒ 首次全量、其后按需。
//! * 符号链接：只有**解析后仍在映射根内且是普通文件**的才纳入（与下载面的包含检查同口径）；
//!   目录符号链接不跟随（避免环路），目录深度上限 [`MAX_DEPTH`]，超限/不可读条目逐条记 stderr。
//! * 不假设任何具体资产存在（例如 `objects256` 两侧都不存在）：一切以遍历实况为准。

use std::collections::HashMap;
use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use sha2::{Digest, Sha256};

use crate::config::RootMap;

/// 目录递归深度上限（防 junction/挂载点成环；超限记 stderr 并截断）。
pub const MAX_DEPTH: usize = 32;
/// 哈希读缓冲（≤1 MiB 上限之内的 64 KiB 块）。
const HASH_CHUNK: usize = 64 * 1024;

/// 清单条目。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// 对外路径（`Data/objects.wzl` 形式，`/` 分隔）。
    pub path: String,
    /// 字节数。
    pub size: u64,
    /// 小写十六进制 `sha256`。
    pub sha256: String,
}

/// `(size, mtime)` 指纹。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stamp {
    /// 字节数。
    pub size: u64,
    /// 修改时间（平台给不出时为 `None`，此时只按 size 判失效）。
    pub mtime: Option<SystemTime>,
}

#[derive(Debug, Clone)]
struct CacheEntry {
    stamp: Stamp,
    sha256: String,
}

/// 线程安全的哈希缓存。
#[derive(Debug, Clone, Default)]
pub struct HashCache {
    entries: Arc<Mutex<HashMap<PathBuf, CacheEntry>>>,
}

impl HashCache {
    /// 取文件的 `sha256`：`(size, mtime)` 未变则命中缓存，否则重算并写回。
    ///
    /// # Errors
    /// 文件不可读/哈希扫描 IO 失败。
    pub fn sha256_of(&self, path: &Path, stamp: Stamp) -> io::Result<String> {
        let cached = {
            let entries = self
                .entries
                .lock()
                .expect("缓存锁不会 poison：临界区内无 panic 路径");
            entries
                .get(path)
                .and_then(|entry| (entry.stamp == stamp).then(|| entry.sha256.clone()))
        };
        if let Some(sha256) = cached {
            return Ok(sha256);
        }
        let sha256 = sha256_file(path)?;
        self.entries
            .lock()
            .expect("缓存锁不会 poison：临界区内无 panic 路径")
            .insert(
                path.to_path_buf(),
                CacheEntry {
                    stamp,
                    sha256: sha256.clone(),
                },
            );
        Ok(sha256)
    }
}

/// 流式计算文件 `sha256`（小写十六进制）。
///
/// # Errors
/// 打开/读取失败。
pub fn sha256_file(path: &Path) -> io::Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; HASH_CHUNK];
    loop {
        let read = io::Read::read(&mut file, &mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex::encode(hasher.finalize()))
}

/// 遍历全部映射根，生成排好序的清单。不可读/不可哈希的条目**逐条记 stderr 后跳过**
/// （可观测性红线：丢弃必须留痕）。
#[must_use]
pub fn build_index(roots: &[RootMap], allowed_exts: &[String], cache: &HashCache) -> Vec<Entry> {
    let mut entries = Vec::new();
    for map in roots {
        walk(
            &map.root,
            &map.root,
            &map.prefix,
            allowed_exts,
            cache,
            &mut entries,
            0,
        );
    }
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    entries
}

#[allow(clippy::too_many_arguments)]
fn walk(
    dir: &Path,
    root: &Path,
    prefix: &str,
    allowed_exts: &[String],
    cache: &HashCache,
    out: &mut Vec<Entry>,
    depth: usize,
) {
    if depth > MAX_DEPTH {
        eprintln!(
            "res-svr: 目录深度超过上限 {MAX_DEPTH}，跳过：{}",
            dir.display()
        );
        return;
    }
    let read_dir = match std::fs::read_dir(dir) {
        Ok(read_dir) => read_dir,
        Err(error) => {
            eprintln!("res-svr: 跳过不可读目录 {}：{error}", dir.display());
            return;
        }
    };
    let mut items = Vec::new();
    for item in read_dir {
        match item {
            Ok(item) => items.push(item),
            Err(error) => eprintln!(
                "res-svr: 跳过不可读条目（目录 {:?}）：{error}",
                dir.display()
            ),
        }
    }
    items.sort_by_key(std::fs::DirEntry::file_name);
    for item in items {
        let path = item.path();
        let Ok(file_type) = item.file_type() else {
            continue;
        };
        if file_type.is_dir() {
            walk(&path, root, prefix, allowed_exts, cache, out, depth + 1);
        } else if file_type.is_file() {
            push_file(&path, &path, root, prefix, allowed_exts, cache, out);
        } else if file_type.is_symlink() {
            let Ok(canonical) = std::fs::canonicalize(&path) else {
                eprintln!("res-svr: 跳过无法解析的符号链接：{}", path.display());
                continue;
            };
            if !canonical.starts_with(root) {
                eprintln!(
                    "res-svr: 跳过越出映射根的符号链接：{} → {}",
                    path.display(),
                    canonical.display()
                );
                continue;
            }
            match std::fs::metadata(&canonical) {
                Ok(metadata) if metadata.is_file() => {
                    push_file(&path, &canonical, root, prefix, allowed_exts, cache, out);
                }
                _ => {}
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn push_file(
    entry_path: &Path,
    real_path: &Path,
    root: &Path,
    prefix: &str,
    allowed_exts: &[String],
    cache: &HashCache,
    out: &mut Vec<Entry>,
) {
    let Some(extension) = entry_path.extension().and_then(std::ffi::OsStr::to_str) else {
        return;
    };
    let extension = format!(".{}", extension.to_ascii_lowercase());
    if !allowed_exts.iter().any(|allowed| allowed == &extension) {
        return;
    }
    let Ok(metadata) = std::fs::metadata(real_path) else {
        eprintln!("res-svr: 跳过无法取元数据的资产：{}", real_path.display());
        return;
    };
    let stamp = Stamp {
        size: metadata.len(),
        mtime: metadata.modified().ok(),
    };
    match cache.sha256_of(real_path, stamp) {
        Ok(sha256) => {
            let relative = entry_path
                .strip_prefix(root)
                .unwrap_or(entry_path)
                .to_string_lossy()
                .replace('\\', "/");
            out.push(Entry {
                path: format!("{prefix}/{relative}"),
                size: metadata.len(),
                sha256,
            });
        }
        Err(error) => eprintln!(
            "res-svr: 跳过无法哈希的资产 {}：{error}",
            real_path.display()
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (crate::testutil::TempDir, Vec<RootMap>, Vec<String>) {
        let dir = crate::testutil::TempDir::new("index");
        let data = dir.dir("Data");
        let map = dir.dir("Map");
        dir.file("Data/a.wzl", b"abc");
        dir.file("Data/b.wzx", b"de");
        dir.file("Data/notes.txt", b"ignored");
        dir.file("Data/sub/c.wzl", b"c");
        dir.file("Map/m.map", b"map!");
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
        let exts = vec![".wzl".to_string(), ".wzx".to_string(), ".map".to_string()];
        (dir, roots, exts)
    }

    #[test]
    fn lists_whitelisted_files_sorted_with_separators() {
        let (_dir, roots, exts) = fixture();
        let cache = HashCache::default();
        let entries = build_index(&roots, &exts, &cache);
        let paths: Vec<&str> = entries.iter().map(|entry| entry.path.as_str()).collect();
        assert_eq!(
            paths,
            vec!["Data/a.wzl", "Data/b.wzx", "Data/sub/c.wzl", "Map/m.map"]
        );
        let first = &entries[0];
        assert_eq!(first.size, 3);
        assert_eq!(
            first.sha256, "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
            "abc 的 sha256"
        );
    }

    /// 缓存失效：内容变了（size 变）⇒ 第二次清单必须给出新 sha 与新 size。
    #[test]
    fn cache_invalidates_when_file_changes() {
        let (dir, roots, exts) = fixture();
        let cache = HashCache::default();
        let before = build_index(&roots, &exts, &cache);
        let before_entry = before
            .iter()
            .find(|entry| entry.path == "Data/a.wzl")
            .expect("条目存在");
        assert_eq!(before_entry.size, 3);

        std::fs::write(dir.path().join("Data/a.wzl"), b"abcDEF").expect("改夹具");
        let after = build_index(&roots, &exts, &cache);
        let after_entry = after
            .iter()
            .find(|entry| entry.path == "Data/a.wzl")
            .expect("条目存在");
        assert_eq!(after_entry.size, 6);
        assert_ne!(after_entry.sha256, before_entry.sha256, "缓存必须失效重算");
        assert_eq!(after_entry.sha256, expected_sha256(b"abcDEF"));
        assert_eq!(before_entry.sha256, expected_sha256(b"abc"));
    }

    fn expected_sha256(bytes: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        hex::encode(hasher.finalize())
    }

    #[test]
    fn non_whitelisted_and_unreadable_are_skipped() {
        let (_dir, roots, exts) = fixture();
        let cache = HashCache::default();
        let entries = build_index(&roots, &exts, &cache);
        assert!(
            !entries
                .iter()
                .any(|entry| entry.path.ends_with("notes.txt")),
            "非白名单扩展不应出现"
        );
    }

    #[cfg(windows)]
    #[test]
    fn symlink_outside_root_is_not_listed() {
        let dir = crate::testutil::TempDir::new("index-symlink");
        let root = dir.dir("Data");
        dir.file("Data/real.wzl", b"real");
        let outside = dir.file("outside/secret.wzl", b"secret");
        let link = root.join("link.wzl");
        if std::os::windows::fs::symlink_file(&outside, &link).is_err() {
            eprintln!("跳过符号链清单用例：本机无法创建符号链接");
            return;
        }
        let roots = vec![RootMap {
            prefix: "Data".to_string(),
            root: std::fs::canonicalize(&root).expect("canonicalize"),
        }];
        let entries = build_index(&roots, &[".wzl".to_string()], &HashCache::default());
        let paths: Vec<&str> = entries.iter().map(|entry| entry.path.as_str()).collect();
        assert_eq!(paths, vec!["Data/real.wzl"], "根外符号链接不得出现在清单里");
    }
}
