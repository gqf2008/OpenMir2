//! 单测共用的临时目录夹具（不做任何真实资产依赖）。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// 进程内唯一、Drop 自清理的临时目录。
pub struct TempDir {
    path: PathBuf,
}

impl TempDir {
    /// 新建空临时目录（同名残留先删）。
    pub fn new(tag: &str) -> Self {
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "res-svr-unit-{tag}-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("创建临时目录");
        Self { path }
    }

    /// 根路径。
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// 建目录（含父目录）。
    pub fn dir(&self, relative: &str) -> PathBuf {
        let path = self.path.join(relative);
        std::fs::create_dir_all(&path).expect("创建夹具目录");
        path
    }

    /// 写文件（含父目录），返回绝对路径。
    pub fn file(&self, relative: &str, bytes: &[u8]) -> PathBuf {
        let path = self.path.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("创建父目录");
        }
        std::fs::write(&path, bytes).expect("写夹具文件");
        path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}
