//! `MIR2_CLOCK_MS` 可注入时钟的**进程级**用例（M3 横切前置 `openmir2-m3-clock-injection`，Rust 半）。
//!
//! 判据（口径 = `src/clock.rs` 模块头）：
//! 1. 设 `MIR2_CLOCK_MS=<n>` ⇒ 进程日志出现 `CLOCK_PROBE origin=<n>`（启动处一条 + 世界建好后
//!    `world_clock_ms=<n>` 一条 ⇒ 证明 `now_ms()` 与 `world.clock_ms` 接的是**同一**注入）；
//! 2. **不设** ⇒ 日志**没有** `CLOCK_PROBE`（生产路径一字不改，`world.clock_ms` 仍从 0 起）；
//! 3. 设非法值 ⇒ 进程**≤N 秒内非零退出**且错误信息点名 `MIR2_CLOCK_MS`（fail-closed，不许静默降级）。
//!
//! 抗 flaky / 防挂死（照 `tests/seed_inject.rs` 的 T-30 口径）：
//! - 临时目录 `pid + 原子计数` 用例级唯一；子进程 stdout 落**文件**（不用管道）⇒ 不存在等不到 EOF；
//! - 等 probe / 等退出一律**有界轮询**；到界/失败一律**因子进程树 kill**（Windows `taskkill /T /F /PID`）；
//! - 每个用例结束断言**不留本用例的 `game-svr`**（按本用例唯一临时目录名过滤）。

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use game_svr::{STARTUP_DB_LOAD_BOUND, STARTUP_DB_PROBE_BOUND, STARTUP_DB_TABLE_COUNT};
use tokio::process::Command;

#[allow(dead_code)] // 共用模块：另一个 target 的条目在本 target 未用
mod support;

/// 注入起点（毫秒；约 2023-11-14，避开 `SystemTime` 的 0 值便于区分「注入 vs 未注入」）。
const CLOCK_ORIGIN: i64 = 1_700_000_000_000;

/// 非法值的**有界退出上限 N**：`main` 在起监听之前 `?` 传播 ⇒ 立即非零退出；取 10s = 实测 ~10× 余量。
const INVALID_CLOCK_EXIT_BOUND: Duration = Duration::from_secs(10);

/// 就绪/探针等待上限（正常 <1s 出现）。
const READY_BOUND: Duration = Duration::from_secs(20);

/// 无 DB 路径 SLA（review F3）：探活上界 1s + 启动开销 1s = 2s。
/// connect 被 refused 时探活立即返回；被防火墙丢包/挂起时最多等满探活上界。
const NO_DB_SLA: Duration = Duration::from_secs(2);
/// 无 DB 路径里「探活上界」之外的启动开销余量。
const NO_DB_SLACK: Duration = Duration::from_secs(1);

/// 有 DB 路径余量：四表上界之外的文件装载 + sim 线程 + 绑定监听（实测 <1s，留 4s）。
const CONNECTED_SLACK: Duration = Duration::from_secs(4);

/// 有 DB 路径 SLA：四表各自 4s 上界 + 4s 余量 = READY_BOUND(20s)（review F3：最坏链路也不超窗口）。
const CONNECTED_SLA: Duration = READY_BOUND;

/// **F3 两条路径的 SLA 静态断言**（卡 `openmir2-gamesvr-db-startup-hermeticity`）：
/// - 无 DB：探活失败就快速返回，总上界 = 探活上界 + 余量 ≤ 2s；
/// - 有 DB：逐表独立上界，最坏 = 上界 × 表数 + 余量 ≤ READY_BOUND(20s)。
///
/// 改大任一个上界而不同步 ⇒ 本用例红。
#[test]
fn startup_db_sla_covers_both_paths() {
    let table_total = STARTUP_DB_LOAD_BOUND
        * u32::try_from(STARTUP_DB_TABLE_COUNT).expect("启动期 DB 表数必落 u32");
    assert!(
        STARTUP_DB_PROBE_BOUND + NO_DB_SLACK <= NO_DB_SLA,
        "无 DB 路径上界（探活 {STARTUP_DB_PROBE_BOUND:?} + 余量 {NO_DB_SLACK:?}）必须 \
         ≤ NO_DB_SLA {NO_DB_SLA:?}"
    );
    assert!(
        table_total + CONNECTED_SLACK <= CONNECTED_SLA,
        "有 DB 路径上界（上界×表数 {table_total:?} + 余量 {CONNECTED_SLACK:?}）必须 \
         ≤ CONNECTED_SLA {CONNECTED_SLA:?}"
    );
    assert_eq!(
        STARTUP_DB_TABLE_COUNT, 4,
        "启动期直连 MySQL 的表是 stditems/magics/monsters/goldsales 四张；增减表必须同步常量"
    );
}

static TEMP_DIR_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn unique_temp_dir(tag: &str) -> PathBuf {
    let seq = TEMP_DIR_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    std::env::temp_dir().join(format!("gs-clock-{tag}-{}-{seq}", std::process::id()))
}

/// 与 `tests/relay.rs` / `tests/seed_inject.rs` 同形的最小合法 `Server.conf`。
fn write_conf(dir: &Path) -> PathBuf {
    std::fs::create_dir_all(dir).unwrap();
    let conf = dir.join("Server.conf");
    std::fs::write(
        &conf,
        format!(
            "[Server]\nGateAddr=127.0.0.1\nGatePort={}\nCheckBlock=12288\n\
             IDSAddr=127.0.0.1\nIDSPort={}\nDBAddr=127.0.0.1\nDBPort={}\nWorldMaps=64x64\n",
            support::next_port(),
            support::next_port(),
            support::next_port()
        ),
    )
    .unwrap();
    conf
}

/// **因子进程树 kill**：Windows 用 `taskkill /T /F /PID`，非 Windows 退化到 `kill`。
fn kill_process_tree(pid: u32) {
    if pid == 0 {
        return;
    }
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("taskkill")
            .args(["/T", "/F", "/PID", &pid.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    #[cfg(not(windows))]
    {
        let _ = std::process::Command::new("kill")
            .arg(pid.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

/// 用例级兜底：**断言 panic（展开）路径**也要因子进程树 kill + 清临时目录。
#[must_use]
struct ProcGuard {
    pid: u32,
    dir: PathBuf,
}

impl Drop for ProcGuard {
    fn drop(&mut self) {
        kill_process_tree(self.pid);
        std::fs::remove_dir_all(&self.dir).ok();
    }
}

/// 是否还有带本用例 `tag` 的 `game-svr` 进程（Windows 用 `wmic` 读命令行；无 `wmic` 时保守返回
/// `false`，不制造假红）。
#[cfg(windows)]
fn game_svr_leftover(tag: &str) -> bool {
    let Ok(out) = std::process::Command::new("wmic")
        .args([
            "process",
            "where",
            "name='game-svr.exe'",
            "get",
            "CommandLine",
            "/format:list",
        ])
        .output()
    else {
        return false;
    };
    String::from_utf8_lossy(&out.stdout).contains(tag)
}

#[cfg(not(windows))]
fn game_svr_leftover(_tag: &str) -> bool {
    false
}

/// **「用例结束不留本用例的 `game-svr`」断言**（给 tree-kill 一点收敛时间，最多 ~5s）。
async fn assert_no_leftover_game_svr(dir: &Path) {
    let tag = dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    if tag.is_empty() {
        return;
    }
    for _ in 0..25 {
        if !game_svr_leftover(&tag) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    panic!("用例结束仍残留本用例的 game-svr（tag={tag}）——tree-kill 未收干净");
}

/// 有超时地轮询日志直到出现 `needle`，返回**最后一次**读到的全文（可见性延迟 ⇒ 不做一次性读取）。
async fn wait_log_contains(log: &Path, needle: &str, timeout: Duration) -> String {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let text = std::fs::read_to_string(log).unwrap_or_default();
        if text.contains(needle) || tokio::time::Instant::now() >= deadline {
            return text;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// **有界收尸**：因子进程树 kill 后再 `wait()` 收直接子进程（best-effort，最多 `reap` 时间）。
async fn kill_tree_and_reap(child: &mut tokio::process::Child, pid: u32, reap: Duration) {
    kill_process_tree(pid);
    let _ = tokio::time::timeout(reap, child.wait()).await;
    child.kill().await.ok();
}

/// 判据 1：设起点 ⇒ 启动处与 `sim_loop` 各打一条 `CLOCK_PROBE origin=<n>`（`world_clock_ms` 见第 2 条）。
#[tokio::test]
async fn clock_env_injection_logs_probe() {
    let dir = unique_temp_dir("set");
    let conf = write_conf(&dir);
    let log = dir.join("stdout.txt");
    let out = std::fs::File::create(&log).unwrap();
    let err = out.try_clone().unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_game-svr"))
        .arg(&conf)
        .env("MIR2_CLOCK_MS", CLOCK_ORIGIN.to_string())
        .stdout(out)
        .stderr(err)
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let pid = child.id().unwrap_or(0);
    let _guard = ProcGuard {
        pid,
        dir: dir.clone(),
    };
    let text = wait_log_contains(&log, "world_clock_ms=", READY_BOUND).await;
    kill_tree_and_reap(&mut child, pid, Duration::from_secs(5)).await;
    assert_no_leftover_game_svr(&dir).await;
    assert!(
        text.contains(&format!("CLOCK_PROBE origin={CLOCK_ORIGIN}")),
        "应打出 CLOCK_PROBE origin={CLOCK_ORIGIN}；实得日志：\n{text}"
    );
    assert!(
        text.contains(&format!("world_clock_ms={CLOCK_ORIGIN}")),
        "world.clock_ms 必须以同一 origin 起（未接注入 ⇒ 本断言必红）；实得日志：\n{text}"
    );
}

/// 判据 2：不设 ⇒ 生产路径一字不改（无 `CLOCK_PROBE`；`world.clock_ms` 不从注入起点起）。
#[tokio::test]
async fn unset_clock_env_is_inert() {
    let dir = unique_temp_dir("unset");
    let conf = write_conf(&dir);
    let log = dir.join("stdout.txt");
    let out = std::fs::File::create(&log).unwrap();
    let err = out.try_clone().unwrap();
    let spawned_at = std::time::Instant::now();
    let mut child = Command::new(env!("CARGO_BIN_EXE_game-svr"))
        .arg(&conf)
        .env_remove("MIR2_CLOCK_MS")
        .stdout(out)
        .stderr(err)
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let pid = child.id().unwrap_or(0);
    let _guard = ProcGuard {
        pid,
        dir: dir.clone(),
    };
    // 先等到监听就绪行（sim 线程在此之前已起），再留一个短 settle 读全文。
    let ready = wait_log_contains(&log, "已启动", READY_BOUND).await;
    let ready_elapsed = spawned_at.elapsed();
    tokio::time::sleep(Duration::from_millis(500)).await;
    let text = std::fs::read_to_string(&log).unwrap_or_default();
    kill_tree_and_reap(&mut child, pid, Duration::from_secs(5)).await;
    assert_no_leftover_game_svr(&dir).await;
    assert!(
        ready.contains("已启动"),
        "进程应正常起来；实得日志：\n{ready}"
    );
    assert!(
        !text.contains("CLOCK_PROBE") && !text.contains("时钟注入"),
        "未设变数时不得出现注入行（生产路径一字不改）；实得日志：\n{text}"
    );
    // **F5 fail-closed + F3 双路径 SLA**：整段审计是一个**无条件调用**的纯函数；
    // 没有「闸门字符串不满足就整段跳过」的分支（review F5 的负例正是打在这里）。
    let path = audit_startup_db_registration(&text, ready_elapsed).unwrap_or_else(|violations| {
        panic!(
            "启动期 DB 登记审计失败（fail-closed）：\n{}\n实得日志：\n{text}",
            violations.join("\n")
        )
    });
    let table_total = STARTUP_DB_LOAD_BOUND
        * u32::try_from(STARTUP_DB_TABLE_COUNT).expect("启动期 DB 表数必落 u32");
    println!(
        "[db-startup 诊断] 路径={path} 已启动耗时={ready_elapsed:?}；探活上界={STARTUP_DB_PROBE_BOUND:?}；\
         单表上界={STARTUP_DB_LOAD_BOUND:?}；表数={STARTUP_DB_TABLE_COUNT}；上界×表数={table_total:?}"
    );
}

/// 判据 3：非法值 ⇒ **≤N 秒内**非零退出 + 点名变数（fail-closed），且**不留 `game-svr`**。
#[tokio::test]
async fn invalid_clock_env_exits_nonzero() {
    let dir = unique_temp_dir("bad");
    let conf = write_conf(&dir);
    let log = dir.join("stdout.txt");
    let out = std::fs::File::create(&log).unwrap();
    let err = out.try_clone().unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_game-svr"))
        .arg(&conf)
        .env("MIR2_CLOCK_MS", "not-an-int")
        .stdout(out)
        .stderr(err)
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let pid = child.id().unwrap_or(0);
    let _guard = ProcGuard {
        pid,
        dir: dir.clone(),
    };
    let started = tokio::time::Instant::now();
    let status = match tokio::time::timeout(INVALID_CLOCK_EXIT_BOUND, child.wait()).await {
        Ok(Ok(status)) => Some(status),
        Ok(Err(e)) => panic!("等待 game-svr 退出失败：{e}"),
        Err(_) => None,
    };
    let elapsed = started.elapsed();
    let text = std::fs::read_to_string(&log).unwrap_or_default();
    if status.is_none() {
        kill_tree_and_reap(&mut child, pid, Duration::from_secs(5)).await;
        assert_no_leftover_game_svr(&dir).await;
        panic!(
            "非法 MIR2_CLOCK_MS 必须 ≤{INVALID_CLOCK_EXIT_BOUND:?} 退出；到界 {elapsed:?} 仍未退出 \
             ⇒ 已因子进程树 kill（防挂死/防泄漏）。子进程原始输出：\n{text}"
        );
    }
    let code = status.unwrap().code();
    assert_no_leftover_game_svr(&dir).await;
    println!(
        "[clock-injection 诊断] invalid MIR2_CLOCK_MS ⇒ exit={code:?} elapsed={elapsed:?}\n{text}"
    );
    assert!(
        code.is_some_and(|c| c != 0),
        "非法 MIR2_CLOCK_MS 必须非零退出；实得 exit={code:?}，耗时 {elapsed:?}；子进程原始输出：\n{text}"
    );
    assert!(
        text.contains("MIR2_CLOCK_MS"),
        "错误信息应点名 MIR2_CLOCK_MS；实得：\n{text}"
    );
}
/// 四张启动表的「失败 WRN」与「成功标记」（F5 审计用）。
const STARTUP_DB_TABLE_MARKERS: [(&str, &str, &str); 4] = [
    ("stditems", "读 stditems 失败", "物品模板表 stditems："),
    ("magics", "读 magics 失败", "魔法模板表 magics："),
    ("monsters", "读 monsters 失败", "怪物模板表 monsters："),
    ("goldsales", "读取物品寄售列表失败", "读取物品寄售列表成功"),
];

/// 探活失败标记（决定走「无 DB 路径」审计分支）。
const DB_PROBE_FAILED_MARKER: &str = "启动期 DB 探活失败";

/// 探活结论三态标记（必须至少出现一条，F5 fail-closed）。
const DB_PROBE_CONCLUDED_MARKERS: [&str; 3] = [
    "启动期 DB 探活成功",
    "启动期 DB 探活失败",
    "启动期 DB 探活跳过",
];

/// **启动期 DB 登记 + 双路径 SLA 审计（fail-closed，review F3/F5）**。
///
/// 返回路径名（`"无DB"` / `"有DB"`）；违规/越界 ⇒ `Err(违规列表)`。**没有可跳过的闸门**：
/// 探活结论缺失、任一表登记缺失、对应路径超 SLA —— 任一条都直接违规。
///
/// - 探活失败（无 DB 路径）：四表必须**逐条**出现「读取失败 ⇒ 空表」WRN；总时长 ≤ [`NO_DB_SLA`]；
/// - 探活成功/跳过（有 DB 路径）：每表必须出现「加载成功」或「失败/超时 WRN」之一；总时长 ≤
///   [`CONNECTED_SLA`]。
fn audit_startup_db_registration(
    text: &str,
    ready_elapsed: Duration,
) -> Result<&'static str, Vec<String>> {
    let mut violations = Vec::new();
    if !DB_PROBE_CONCLUDED_MARKERS.iter().any(|m| text.contains(m)) {
        violations.push("缺启动期 DB 探活结论（成功/失败/跳过三态至少一条）".to_string());
    }
    let db_unreachable = text.contains(DB_PROBE_FAILED_MARKER);
    for (table, fail, ok) in STARTUP_DB_TABLE_MARKERS {
        if db_unreachable {
            if !text.contains(fail) {
                violations.push(format!("无 DB 路径缺 {table} 的登记 WRN：`{fail}`"));
            }
        } else if !text.contains(fail) && !text.contains(ok) {
            violations.push(format!(
                "有 DB 路径 {table} 既无成功标记 `{ok}` 也无失败登记 `{fail}`（静默？）"
            ));
        }
    }
    if db_unreachable {
        if ready_elapsed > NO_DB_SLA {
            violations.push(format!(
                "无 DB 路径超 SLA：实测 {ready_elapsed:?} > {NO_DB_SLA:?}"
            ));
        }
    } else if ready_elapsed > CONNECTED_SLA {
        violations.push(format!(
            "有 DB 路径超 SLA：实测 {ready_elapsed:?} > {CONNECTED_SLA:?}"
        ));
    }
    if violations.is_empty() {
        Ok(if db_unreachable { "无DB" } else { "有DB" })
    } else {
        Err(violations)
    }
}

#[cfg(test)]
mod registration_audit_teeth {
    use super::*;

    const ALL_UNREACHABLE: &str = "启动期 DB 探活失败\n读 stditems 失败\n读 magics 失败\n读 monsters 失败\n读取物品寄售列表失败";

    /// 无 DB 路径：四条 WRN 全在 + 时长在 SLA 内 ⇒ 通过。
    #[test]
    fn unreachable_path_accepts_all_four_wrns() {
        assert_eq!(
            audit_startup_db_registration(ALL_UNREACHABLE, Duration::from_millis(50)),
            Ok("无DB")
        );
    }

    /// 无 DB 路径：删掉任一表 WRN ⇒ 必须违规（review F5 负例：删 magics 跳过 WRN）。
    #[test]
    fn unreachable_path_rejects_missing_table_wrn() {
        let text = "启动期 DB 探活失败\n读 stditems 失败\n读 monsters 失败\n读取物品寄售列表失败";
        let v = audit_startup_db_registration(text, Duration::from_millis(50))
            .expect_err("缺 magics WRN 必须违规");
        assert_eq!(v.len(), 1, "必须精确指出 magics 缺登记：{v:?}");
        assert!(v[0].contains("magics"));
    }

    /// 无 DB 路径：改写 stditems WRN 文案 ⇒ 必须违规（review 负例：`读取 stditems 失败`）。
    #[test]
    fn unreachable_path_rejects_rewritten_wrn() {
        let text = "启动期 DB 探活失败\n读取 stditems 失败\n读 magics 失败\n读 monsters 失败\n读取物品寄售列表失败";
        let v = audit_startup_db_registration(text, Duration::from_millis(50))
            .expect_err("改写后的 stditems 文案不得兜住");
        assert_eq!(v.len(), 1, "必须精确指出 stditems：{v:?}");
        assert!(v[0].contains("stditems"));
    }

    /// 缺探活结论 ⇒ 违规（不许靠缺结论把审计降级）。
    #[test]
    fn rejects_missing_probe_conclusion() {
        let v = audit_startup_db_registration(
            "读 stditems 失败\n读 magics 失败\n读 monsters 失败\n读取物品寄售列表失败",
            Duration::from_millis(50),
        )
        .expect_err("缺探活结论必须违规");
        assert!(
            v.iter().any(|m| m.contains("缺启动期 DB 探活结论")),
            "{v:?}"
        );
    }

    /// 无 DB 路径超 SLA ⇒ 违规（review F3）。
    #[test]
    fn rejects_no_db_sla_overrun() {
        let v =
            audit_startup_db_registration(ALL_UNREACHABLE, NO_DB_SLA + Duration::from_millis(1))
                .expect_err("无 DB 超 SLA 必须违规");
        assert!(v.iter().any(|m| m.contains("无 DB 路径超 SLA")), "{v:?}");
    }

    /// 有 DB 路径：每表成功或失败标记齐全 ⇒ 通过。
    #[test]
    fn connected_path_accepts_success_or_failure_per_table() {
        let text = "启动期 DB 探活成功\n物品模板表 stditems：1 条\n读 magics 失败（x）\n\
                    怪物模板表 monsters：0 条\n读取物品寄售列表成功...[0]";
        assert_eq!(
            audit_startup_db_registration(text, Duration::from_millis(50)),
            Ok("有DB")
        );
    }

    /// 有 DB 路径：某表既无成功也无失败 ⇒ 违规（静默不允许）。
    #[test]
    fn connected_path_rejects_silent_table() {
        let text = "启动期 DB 探活成功\n物品模板表 stditems：1 条\n魔法模板表 magics：0 条\n读取物品寄售列表成功...[0]";
        let v = audit_startup_db_registration(text, Duration::from_millis(50))
            .expect_err("monsters 静默必须违规");
        assert_eq!(v.len(), 1, "{v:?}");
        assert!(v[0].contains("monsters"));
    }

    /// 有 DB 路径超 SLA ⇒ 违规（review F3）。
    #[test]
    fn rejects_connected_sla_overrun() {
        let text = "启动期 DB 探活成功\n物品模板表 stditems：1 条\n魔法模板表 magics：0 条\n怪物模板表 monsters：0 条\n读取物品寄售列表成功...[0]";
        let v = audit_startup_db_registration(text, CONNECTED_SLA + Duration::from_millis(1))
            .expect_err("有 DB 超 SLA 必须违规");
        assert!(v.iter().any(|m| m.contains("有 DB 路径超 SLA")), "{v:?}");
    }
}

/// **F1 牙（review）**：DB 探活连得上（TCP accept 成功）但查询永不返回 ⇒ 四张表必须
/// **各自尝试并各自超时**；不许因首表超时跳过后续表（旧链式跳过 = 慢但可用的 DB 被误判死）。
///
/// 做法：假服务只 accept、不说话（不发 MySQL 握手）。探活 → Connected；四张表的 sqlx
/// connect 各自等握手 ⇒ 各自在 `STARTUP_DB_LOAD_BOUND` 处超时。
/// 红检：恢复链式跳过（首表超时后置位并跳过其余三表）⇒ 日志只有 1 条超时 + 3 条
/// 「启动期 DB 探活失败…」⇒ 本用例红。
#[tokio::test]
async fn query_phase_timeouts_do_not_skip_other_tables() {
    let dir = unique_temp_dir("mute-db");
    std::fs::create_dir_all(&dir).unwrap();
    let db_port = support::next_port();
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", db_port))
        .await
        .expect("假 DB 服务 bind 失败");
    let fake = tokio::spawn(async move {
        // 只收不发：客户端等 MySQL 握手 ⇒ 各表各自等满自己的上界。
        let mut held = Vec::new();
        while let Ok((sock, _peer)) = listener.accept().await {
            held.push(sock);
        }
    });
    let conf = dir.join("Server.conf");
    std::fs::write(
        &conf,
        format!(
            "[DataBase]\nDBType=MySQL\nConnctionString=server=127.0.0.1;uid=root;pwd=;database=mir2;port={db_port};\n\
             [Server]\nGateAddr=127.0.0.1\nGatePort={}\nCheckBlock=12288\n\
             IDSAddr=127.0.0.1\nIDSPort={}\nDBAddr=127.0.0.1\nDBPort={}\nWorldMaps=64x64\n",
            support::next_port(),
            support::next_port(),
            support::next_port()
        ),
    )
    .unwrap();
    let log = dir.join("stdout.txt");
    let out = std::fs::File::create(&log).unwrap();
    let err = out.try_clone().unwrap();
    let spawned_at = std::time::Instant::now();
    let mut child = Command::new(env!("CARGO_BIN_EXE_game-svr"))
        .arg(&conf)
        .env_remove("MIR2_CLOCK_MS")
        .stdout(out)
        .stderr(err)
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let pid = child.id().unwrap_or(0);
    let _guard = ProcGuard {
        pid,
        dir: dir.clone(),
    };
    let ready = wait_log_contains(&log, "已启动", READY_BOUND).await;
    let elapsed = spawned_at.elapsed();
    kill_tree_and_reap(&mut child, pid, Duration::from_secs(5)).await;
    assert_no_leftover_game_svr(&dir).await;
    fake.abort();
    assert!(
        ready.contains("已启动"),
        "假 DB 只 accept 不说话时，四表各自超时后仍必须启动；实得日志：\n{ready}"
    );
    assert!(
        ready.contains("启动期 DB 探活成功"),
        "探活必须连上假服务（否则测的是无 DB 路径）；实得日志：\n{ready}"
    );
    // 只数**直接超时**（`读 X 失败（启动期加载超时（上界…`）；链式跳过的表会打
    // 「读 X 失败（启动期 DB 探活失败：启动期加载超时…」，不能把它算成一次尝试。
    let direct_timeouts = ready.matches("失败（启动期加载超时（上界").count();
    assert_eq!(
        direct_timeouts, 4,
        "四张表必须各自尝试并各自超时（链式跳过会少于此数）；实得日志：\n{ready}"
    );
    // 链式跳过会给后续表再打「启动期 DB 探活失败」；假服务 connect 成功 ⇒ 探活成功
    // ⇒ 这条串必须一条都没有。
    assert_eq!(
        ready.matches("启动期 DB 探活失败").count(),
        0,
        "探活已成功，任何「DB 探活失败」都只能是链式跳过的误判（F1）；实得日志：\n{ready}"
    );
    println!(
        "[db-startup 诊断] 有DB-假库路径 已启动耗时={elapsed:?}；四表直接超时 WRN={direct_timeouts}；CONNECTED_SLA={CONNECTED_SLA:?}"
    );
    assert!(
        elapsed <= CONNECTED_SLA,
        "有 DB 路径最坏链路必须 ≤ {CONNECTED_SLA:?}；实测 {elapsed:?}"
    );
}
