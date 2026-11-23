//! **L2-5 follow-up**：`MIR2_ZONE_MIRROR_DIAG=<path>` 门控诊断——每区镜像 id 的**直接状态证据**。
//!
//! 目的：把「离开后镜像是否滞留」从**行为推断**（看帧/看 WRN）变成**状态证据**：每个切片区在
//! `before_phase2` 的镜像刷新（`apply_mirrors`）之后，把该区世界里的镜像 id 集合（`world.mirror_ids()`，
//! 槽位序 ⇒ 确定性）写一行；集合**变化时**才写。
//!
//! # 生产默认零参与（实现方式）
//!
//! - 环境变量未设 ⇒ 启动后第一次调用时只查一次 `std::env::var_os`，之后 `enabled()` 只读一个
//!   `AtomicBool`；**不取 `mirror_ids()`、不分配、不加锁、不写文件**。
//! - **可核凭据（实测，不是代码阅读）**：`world.mirror_ids()` 的**唯一**调用入口是
//!   [`maybe_write_world`]；测试 `mirror_ids_not_called_when_disabled` 用 `#[cfg(test)]` 计数实测
//!   「env 未设 ⇒ 调用次数 **0**；env 设了 ⇒ 恰好 **1**」。
//! - 环境变量设了 ⇒ 打不开文件只打一条 WRN 并保持禁用（fail-open：诊断不得影响游戏路径）。
//! - 本模块**不新开 cargo feature**：与现有旁路日志（`frame-src`）不同，这里只有 env 门，
//!   默认构建里的唯一开销是一次 `Relaxed` 原子读（每个切片区每 tick 一次）。
//!
//! # 文件格式
//!
//! 首行 `zone,tick,mirror_ids`；其后每行 `<zone>,<tick>,<id;id;...>`（空集合 = 空串）。
//! `tick` 是该区 B 段的 `tick` 参数（= `self.ticks`），**不另取墙钟**。

use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use crate::zone::ZoneId;

/// 环境门：设了路径才写（生产默认不设 ⇒ 零参与）。
const ENV_ZONE_MIRROR_DIAG: &str = "MIR2_ZONE_MIRROR_DIAG";

/// 是否已尝试过初始化（只读一次环境变量）。
static INIT: AtomicBool = AtomicBool::new(false);
/// 诊断是否真的启用（文件已成功打开）。
static ENABLED: AtomicBool = AtomicBool::new(false);
/// 诊断文件（追加写；进程级共享）。
static FILE: Mutex<Option<std::fs::File>> = Mutex::new(None);

fn ensure_init() {
    // `swap` 保证并发首个调用只初始化一次（失败也保持 INIT=true，不再反复尝试）。
    if INIT.swap(true, Ordering::Relaxed) {
        return;
    }
    let Some(path) = std::env::var_os(ENV_ZONE_MIRROR_DIAG) else {
        return;
    };
    match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        Ok(mut f) => {
            if let Err(e) = f.write_all(b"zone,tick,mirror_ids\n") {
                crate::glog!("WRN", "镜像诊断写表头失败（{path:?}）：{e}");
                return;
            }
            *FILE
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(f);
            ENABLED.store(true, Ordering::Relaxed);
        }
        Err(e) => crate::glog!("WRN", "镜像诊断打不开（{path:?}）：{e}"),
    }
}

/// 诊断是否启用。**返回 `false` 时调用方不得调用 `world.mirror_ids()`**（避免无谓分配）。
#[inline]
pub(crate) fn enabled() -> bool {
    if !INIT.load(Ordering::Relaxed) {
        ensure_init();
    }
    ENABLED.load(Ordering::Relaxed)
}

/// **测试打点**：`world.mirror_ids()` 的实际调用次数（仅 `cfg(test)` 编译）。
///
/// 「env 未设 ⇒ 默认零开销」不靠代码阅读，而靠这个计数实测（见
/// `mirror_ids_not_called_when_disabled`）。
#[cfg(test)]
static MIRROR_IDS_CALLS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// 读 `world.mirror_ids()` 调用次数（测试用）。
#[cfg(test)]
pub(crate) fn mirror_ids_calls() -> u64 {
    MIRROR_IDS_CALLS.load(Ordering::Relaxed)
}

/// 复位调用次数（测试用）。
#[cfg(test)]
pub(crate) fn reset_call_counter_for_tests() {
    MIRROR_IDS_CALLS.store(0, Ordering::Relaxed);
}

/// **唯一的 `world.mirror_ids()` 调用入口**：未启用 ⇒ 立即返回（**调用次数 0**）；
/// 启用 ⇒ 取一次集合，再按「变化时写」落盘。
pub(crate) fn maybe_write_world(
    zone: ZoneId,
    tick: u64,
    world: &mir2_world_bevy::World,
    last: &mut Option<Vec<i32>>,
) {
    if !enabled() {
        return;
    }
    #[cfg(test)]
    MIRROR_IDS_CALLS.fetch_add(1, Ordering::Relaxed);
    let ids = world.mirror_ids();
    maybe_write(zone, tick, &ids, last);
}

/// 集合变化时写一行；`last` 是**每区**的缓存（调用方持有，跨 tick 复用）。
pub(crate) fn maybe_write(zone: ZoneId, tick: u64, ids: &[i32], last: &mut Option<Vec<i32>>) {
    if !enabled() {
        return;
    }
    if last.as_deref() == Some(ids) {
        return;
    }
    *last = Some(ids.to_vec());
    let mut joined = String::new();
    for (i, id) in ids.iter().enumerate() {
        if i > 0 {
            joined.push(';');
        }
        joined.push_str(&id.to_string());
    }
    let line = format!("{},{tick},{joined}\n", zone.get());
    let mut guard = FILE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(f) = guard.as_mut() {
        if let Err(e) = f.write_all(line.as_bytes()) {
            crate::glog!("WRN", "镜像诊断写失败：{e}");
        }
    }
}

/// 测试用：复位初始化状态（env 是进程级全局，用例必须持 `perf_counters::TEST_LOCK`）。
#[cfg(test)]
pub(crate) fn reset_for_tests() {
    INIT.store(false, Ordering::Relaxed);
    ENABLED.store(false, Ordering::Relaxed);
    *FILE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::perf_counters::TEST_LOCK;

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        // 每次调用取独立序号：多个用例并行跑同一 test binary 时不共享目录（C19 夹具键加固）。
        static DIR_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let seq = DIR_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "mir2_zone_mirror_diag_{tag}_{}_{seq}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    /// 默认零参与：env 未设 ⇒ `enabled()==false`、`maybe_write` 不写、`last` 不被改。
    ///
    /// 变异：把 `enabled()` 改成恒真 ⇒ 本条红（会尝试写并可能建文件）。
    #[test]
    fn disabled_by_default_is_noop() {
        let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::remove_var(ENV_ZONE_MIRROR_DIAG);
        reset_for_tests();
        let dir = temp_dir("off");
        let path = dir.join("diag.csv");
        assert!(!enabled(), "env 未设必须禁用");
        let mut last = None;
        maybe_write(ZoneId::new(1), 1, &[7], &mut last);
        assert!(last.is_none(), "禁用时不得缓存/写");
        assert!(!path.exists(), "禁用时不得建文件");
        reset_for_tests();
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 变化时写：同集合不重复写；集合变化才追加；文件含表头 + 两行，第二行是空集合。
    ///
    /// 变异：删「变化比较」⇒ 同集合写两次 ⇒ 行数断言红。
    #[test]
    fn writes_header_and_only_on_change() {
        let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = temp_dir("on");
        let path = dir.join("diag.csv");
        std::env::set_var(ENV_ZONE_MIRROR_DIAG, &path);
        reset_for_tests();
        assert!(enabled(), "env 设了必须启用");
        let mut last = None;
        maybe_write(ZoneId::new(2), 10, &[7, 9], &mut last);
        maybe_write(ZoneId::new(2), 11, &[7, 9], &mut last); // 同集合 ⇒ 跳过
        maybe_write(ZoneId::new(2), 12, &[], &mut last); // 变化 ⇒ 写空集合
        std::env::remove_var(ENV_ZONE_MIRROR_DIAG);
        reset_for_tests();
        let text = std::fs::read_to_string(&path).expect("诊断文件必须存在");
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 3, "表头 + 2 行（同集合不写）：{text:?}");
        assert_eq!(lines[0], "zone,tick,mirror_ids");
        assert_eq!(lines[1], "2,10,7;9");
        assert_eq!(lines[2], "2,12,");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **实测（不是代码阅读）**：env 未设 ⇒ `world.mirror_ids()` 调用次数 **0**；
    /// env 设了 ⇒ 恰好 **1** 次。这是「默认零参与/零开销」的可核凭据。
    ///
    /// 变异：把 `maybe_write_world` 的 `enabled()` 判断删掉（或改成恒真）⇒ 禁用态计数变 1 ⇒ 红。
    #[test]
    fn mirror_ids_not_called_when_disabled() {
        let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::remove_var(ENV_ZONE_MIRROR_DIAG);
        reset_for_tests();
        reset_call_counter_for_tests();
        let world = mir2_world_bevy::World::new(200, 200);
        let mut last = None;
        maybe_write_world(ZoneId::new(1), 1, &world, &mut last);
        println!(
            "MIRROR_DIAG disabled: calls={} last_is_none={}",
            mirror_ids_calls(),
            last.is_none()
        );
        assert_eq!(mirror_ids_calls(), 0, "禁用时不得调用 mirror_ids()");
        assert!(last.is_none(), "禁用时不得缓存集合");

        let dir = temp_dir("calls");
        let path = dir.join("diag.csv");
        std::env::set_var(ENV_ZONE_MIRROR_DIAG, &path);
        reset_for_tests();
        reset_call_counter_for_tests();
        maybe_write_world(ZoneId::new(1), 2, &world, &mut last);
        println!("MIRROR_DIAG enabled: calls={}", mirror_ids_calls());
        assert_eq!(mirror_ids_calls(), 1, "启用时恰好调用 1 次 mirror_ids()");
        std::env::remove_var(ENV_ZONE_MIRROR_DIAG);
        reset_for_tests();
        let _ = std::fs::remove_dir_all(&dir);
    }
}
