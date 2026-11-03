//! 物品模板表（`stditems`）的**加载段**——oracle `GameApp.Initialize()` 直连 MySQL 读 `stditems`
//! （`MySqlDB.LoadItemsDB`，`src/GameSrv/DB/MySqlDB.cs:13`）。
//!
//! # 查表口径已收敛到 `game_svr::ability`
//!
//! 本模块**只保留 DB 载入**：`load_items` 返回 `SELECT * FROM stditems` 的**顺序表**，
//! `catalog_from` 把它变成 [`game_svr::ability::StdItemCatalog`]（= oracle `GameItemSystem`）。
//! `Index → StdItem.Shape` / `GetStdItem` 的查表与 1-based 语义**只有一份**，在 `ability.rs`
//! （`openmir2-m3-bag-equip-alignment` 的能力接缝，owner 裁决为保留方；本片按 `brief-m2svr-next.md`
//! §18 把 m2-svr 侧原来的第二份查表删除）。
//!
//! # 1-based 口径（为什么 `catalog_from` 下标从 1 起）
//!
//! `GameItemSystem.GetStdItem(ushort nItemIdx)`（`GameItemSystem.cs:20-33`）`nItemIdx -= 1` 后索引
//! `StdItemList`；而 `AddItem` 赋的 `Index = i + 1`（`GameItemSystem.cs:133`）⇒ **Index = 表内位置 + 1**。
//! 空名记录按 oracle 的 `if (string.IsNullOrEmpty(result.Name)) result = null`（`:28-31`）**不放进目录**
//! （`StdItemCatalog::get` 不做空名判定，见 `ability.rs`）⇒ 入目录前先滤掉空名，位置仍按原下标 + 1。
//!
//! 加载失败 / 表为空 ⇒ `catalog` 空 ⇒ `GetStdItem` 恒 `null`（oracle `LoadItemsDB` 失败分支，
//! `GameApp.cs:111-115`）；调用方据此降级 + WRN，**不伪造**任何 Shape。

use game_svr::ability::StdItemCatalog;
use mir2_data::loaders::DataLoaders;
use mir2_data::StdItem;
use mir2_storage::LegacyDsn;

/// oracle `GameSvrConf.ConnctionString` 的源码缺省（`GameSvrConf.cs:1921`）。
pub const DEFAULT_CONN_STRING: &str = "server=127.0.0.1;uid=root;pwd=;database=mir2;";

/// 启动期 DB 探活结果（只判 **connect 相位**，不跑任何 SQL）。
///
/// 卡 `openmir2-gamesvr-db-startup-hermeticity` review F1/F3：只有 [`Self::ConnectFailed`]
/// 才是「同一 DSN 不可达」，才允许跳过其余启动表；查询阶段的超时/报错只在**当前表**
/// 降级为空表，绝不牵连其它表（「慢但可用」的 DB 不得被误判死）。
///
/// 卡 `openmir2-gamesvr-db-probe-threshold`（tester-1 台账 `12747c557827`）：**connect 超时**
/// 是第三态 [`Self::TimedOut`]（可达性**未知**），绝不并入 [`Self::ConnectFailed`]（明确不可达）。
/// 慢连接（如 1.1s/1.5s 才完成握手）在 1s 快速探活上界内可能没有结论，但 DB 其实可用；把它判死
/// 会让四张启动表被静默降级为空表。
#[derive(Debug)]
pub enum StartupDbProbe {
    /// TCP connect 成功。MySQL 握手/认证/SQL 结果由各表加载器各自负责。
    Connected,
    /// connect **明确失败**（refused / DNS 解析失败 / 网络不可达等）
    /// ⇒ 判定同一 DSN 不可达，四张启动表都不尝试。
    ConnectFailed(String),
    /// connect 在探活窗口内**没有结论**（超时）⇒ 可达性未知：**不得**跳过四表，
    /// 继续按每表各自的上界（[`STARTUP_DB_LOAD_BOUND`](crate::STARTUP_DB_LOAD_BOUND)）尝试加载。
    TimedOut(String),
    /// 连接串解析失败 ⇒ 不置「不可达」，交由各表加载器立即报告（它们不会等超时）。
    DsnError(String),
}

impl StartupDbProbe {
    /// 是否**允许跳过**四张启动表；返回跳过理由（仅 [`Self::ConnectFailed`]）。
    ///
    /// 这是「四表是否都不尝试」的唯一判据（F1）：超时（[`Self::TimedOut`]）= 未知 ⇒ 返回
    /// `None`，由各表自己的 4s 上界决定成败。**不要**在这里把超时并进不可达。
    #[must_use]
    pub fn skip_startup_tables(&self) -> Option<&str> {
        match self {
            Self::ConnectFailed(e) => Some(e),
            Self::Connected | Self::TimedOut(_) | Self::DsnError(_) => None,
        }
    }
}

/// 启动期探活报告：最终三态 + 是否经过「快速观察无结论」的扩展观察。
///
/// `fast_unknown = true` ⇒ [`STARTUP_DB_PROBE_BOUND`](crate::STARTUP_DB_PROBE_BOUND) 内没有
/// 结论（A3 的 Unknown 过渡态）。**同时发起**的扩展观察只用来区分两类：
/// - 明确 `ConnectFailed`（例如系统把 RST 延迟到 ~2s 才上报）⇒ 四表跳过（A2）；
/// - 其余（最终 Ok 或仍超时）⇒ 一律按 **`TimedOut`（未知/继续尝试）** 处理，继续加载四表
///   （A1：慢连接（1.1s/1.5s 档）的结论仍是「未知」，不会被升级成 Ok 而掩盖慢连接事实）。
#[derive(Debug)]
pub struct StartupDbProbeReport {
    /// 最终结论（三态）。
    pub outcome: StartupDbProbe,
    /// 快速观察是否**无结论**（最终态来自扩展观察的定案）。
    pub fast_unknown: bool,
}

impl StartupDbProbeReport {
    /// 见 [`StartupDbProbe::skip_startup_tables`]（判据只取最终态）。
    #[must_use]
    pub fn skip_startup_tables(&self) -> Option<&str> {
        self.outcome.skip_startup_tables()
    }
}

/// 把 connect 相位的**原始结果**分类成三态（超时 / 明确失败 / 成功）。
///
/// `outcome = None` 表示观察窗口已到（`tokio::time::timeout` 的 `Err`）；`Some(Ok(_))` 为
/// connect 成功，`Some(Err(e))` 为 connect 明确失败。抽成纯函数是为了让「超时 ≠ 不可达」
/// 有**确定性**用例（不依赖真机延迟）。
fn classify_connect(
    outcome: Option<std::io::Result<tokio::net::TcpStream>>,
    host: &str,
    port: u16,
) -> StartupDbProbe {
    match outcome {
        Some(Ok(_sock)) => StartupDbProbe::Connected,
        Some(Err(e)) => {
            StartupDbProbe::ConnectFailed(format!("connect 失败（{host}:{port}）：{e}"))
        }
        None => {
            StartupDbProbe::TimedOut(format!("connect 超时（{host}:{port}；观察窗口内没有结论）"))
        }
    }
}

/// 主机名解析接口：启动探活的 fast/confirm 两条 connect **共用一次解析结果**
/// （卡 `openmir2-gamesvr-db-probe-dns-once`）。
///
/// 生产实现是 [`SystemResolver`]。抽成可注入接口，是为了让下面三条契约有**确定性**用例
/// （不依赖真 DNS / fake-IP）：
/// - 同一主机名在两条 connect 之间**只解析一次**；
/// - fast 与 confirm 使用**同一个** [`SocketAddr`](std::net::SocketAddr)；
/// - 解析失败仍按 [`StartupDbProbe::ConnectFailed`] 处理（语义与旧实现一致）。
///
/// 背景（reviewer-21 在卡 `openmir2-gamesvr-db-probe-threshold` 的 review `8c40e2b0`）：
/// fake-IP（FlClash）或负载均衡下，同一主机名两次解析可能落到不同后端；旧实现两条 connect
/// 各解析一次，fast 命中 down 的后端就会 `ConnectFailed` ⇒ 四表被**假跳过**。
type ResolveFuture<'a> = std::pin::Pin<
    Box<dyn std::future::Future<Output = std::io::Result<std::net::SocketAddr>> + Send + 'a>,
>;

pub trait AddrResolver {
    /// 把 `host:port` 解析成**一个**地址；多条记录取第一条。
    ///
    /// 旧实现 `TcpStream::connect((host, port))` 内部也是「任选其一」；本卡只把
    /// 「每次连接各选一次」收敛成「选一次后两条连接复用」，不改变选法。
    fn resolve<'a>(&'a self, host: &'a str, port: u16) -> ResolveFuture<'a>;
}

/// 生产解析器：走 tokio 系统解析（等价于旧 `TcpStream::connect((host, port))` 内部的解析）。
pub struct SystemResolver;

impl AddrResolver for SystemResolver {
    fn resolve<'a>(&'a self, host: &'a str, port: u16) -> ResolveFuture<'a> {
        Box::pin(async move {
            tokio::net::lookup_host((host, port))
                .await?
                .next()
                .ok_or_else(|| {
                    std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        format!("{host}:{port} 没有可用地址"),
                    )
                })
        })
    }
}

/// 启动期 DB 探活：`TcpStream::connect`，不跑 SQL、不建 sqlx 池。
///
/// 只回答「connect 相位是否可达」—— 这是决定「四表是否都不尝试」的唯一判据（F1）。
/// 探活成功但后续查询超时**不算**不可达（那只让该表降级为空表）；探活**超时**同样不算
/// 不可达（可达性未知，见 [`StartupDbProbe::TimedOut`]）。
///
/// 两条 connect **同时发起**（`select!`）：
/// - `fast_bound` 内的明确结论（Ok / refused）立即返回，扩展连接被丢弃 ⇒ 快路径不拖慢；
/// - 快速观察无结论时才等扩展观察（它已并行跑了 `fast_bound` 那么久），最多到
///   `confirm_bound`。**扩展观察只用于确认「明确 refused」**（含系统延迟到 ~2s 才上报的
///   RST，避免无 DB 路径退化成「四表各等 4s」，A2）；若它最终连上，本次探活仍判
///   `TimedOut`（未知/继续尝试，A1），不把「超过快速上界才连上」掩盖成 Ok。
pub async fn probe_reachable(
    conn: &str,
    fast_bound: std::time::Duration,
    confirm_bound: std::time::Duration,
) -> StartupDbProbeReport {
    probe_reachable_with(conn, fast_bound, confirm_bound, &SystemResolver).await
}

/// [`probe_reachable`] 的可注入版本（生产路径固定用 [`SystemResolver`]）。
///
/// **解析只做一次**（卡 `openmir2-gamesvr-db-probe-dns-once`）：先得到 `addr`，fast 与
/// confirm 两条 connect 都连这一个 `addr`，因此 fake-IP / 负载均衡下不会落到不同后端。
/// 解析失败 ⇒ [`StartupDbProbe::ConnectFailed`]（与旧实现里 connect 的 DNS 失败同语义：
/// 明确不可达 ⇒ 跳过四表）。
pub async fn probe_reachable_with<R: AddrResolver + ?Sized>(
    conn: &str,
    fast_bound: std::time::Duration,
    confirm_bound: std::time::Duration,
    resolver: &R,
) -> StartupDbProbeReport {
    let dsn = match LegacyDsn::parse(conn) {
        Ok(dsn) => dsn,
        Err(e) => {
            return StartupDbProbeReport {
                outcome: StartupDbProbe::DsnError(format!("ConnctionString 解析失败：{e}")),
                fast_unknown: false,
            };
        }
    };
    // 卡 `openmir2-gamesvr-db-probe-dns-once`：**先解析一次**成 SocketAddr，fast/confirm 复用。
    //
    // 解析本身也必须在**扩展观察上界**内给出结论：旧实现把 DNS 解析包在 `TcpStream::connect`
    // 的 1s/2.5s 上界里；若改成裸 `resolve().await`，DNS 卡死会让启动探活无界（破坏 SLA）。
    let started = tokio::time::Instant::now();
    let target =
        match tokio::time::timeout(confirm_bound, resolver.resolve(&dsn.host, dsn.port)).await {
            Ok(Ok(addr)) => addr,
            Ok(Err(e)) => {
                return StartupDbProbeReport {
                    outcome: StartupDbProbe::ConnectFailed(format!(
                        "connect 失败（{}:{}）：{e}",
                        dsn.host, dsn.port
                    )),
                    // 解析在快速观察窗口内失败 ⇒ 明确结论；窗口外才失败 ⇒ 与旧实现一样算「快速观察无结论」。
                    fast_unknown: started.elapsed() >= fast_bound,
                };
            }
            Err(_) => {
                return StartupDbProbeReport {
                    outcome: StartupDbProbe::TimedOut(format!(
                    "connect 超时（{}:{}；DNS 解析在扩展观察上界 {confirm_bound:?} 内没有结论）",
                    dsn.host, dsn.port
                )),
                    fast_unknown: true,
                };
            }
        };
    let resolution_elapsed = started.elapsed();
    if resolution_elapsed >= fast_bound {
        // 解析已用掉快速观察窗口（旧实现里 DNS 在 fast 的 1s connect 内）⇒ 快速观察无结论；
        // 扩展观察用**同一地址**定案：明确 refused ⇒ ConnectFailed，连上 ⇒ 仍按未知（A1）。
        let remaining = confirm_bound.saturating_sub(resolution_elapsed);
        let outcome = match classify_connect(
            tokio::time::timeout(remaining, tokio::net::TcpStream::connect(target))
                .await
                .ok(),
            &dsn.host,
            dsn.port,
        ) {
            StartupDbProbe::ConnectFailed(e) => StartupDbProbe::ConnectFailed(e),
            StartupDbProbe::Connected => StartupDbProbe::TimedOut(format!(
                "connect 超过快速观察上界 {fast_bound:?}（DNS 解析耗时 {resolution_elapsed:?}；扩展观察确认最终可达：慢连接按未知继续）"
            )),
            other => other,
        };
        return StartupDbProbeReport {
            outcome,
            fast_unknown: true,
        };
    }
    let fast = tokio::time::timeout(
        fast_bound.saturating_sub(resolution_elapsed),
        tokio::net::TcpStream::connect(target),
    );
    let confirm = tokio::time::timeout(
        confirm_bound.saturating_sub(resolution_elapsed),
        tokio::net::TcpStream::connect(target),
    );
    tokio::pin!(fast);
    tokio::pin!(confirm);
    tokio::select! {
        biased;
        fast_outcome = &mut fast => {
            let outcome = classify_connect(fast_outcome.ok(), &dsn.host, dsn.port);
            if matches!(outcome, StartupDbProbe::TimedOut(_)) {
                // 快速观察无结论 ⇒ 扩展观察（自 t=0 并行发起）只用来确认「明确 refused」。
                let outcome = match classify_connect(confirm.await.ok(), &dsn.host, dsn.port) {
                    // 明确不可达（即使 RST 上报得晚）⇒ 跳过四表（A2）。
                    StartupDbProbe::ConnectFailed(e) => StartupDbProbe::ConnectFailed(e),
                    // 最终可达：仍按未知/继续尝试（A1），不升级成 Ok。
                    StartupDbProbe::Connected => StartupDbProbe::TimedOut(format!(
                        "connect 超过快速观察上界 {fast_bound:?}（扩展观察确认最终可达：慢连接按未知继续）"
                    )),
                    other => other,
                };
                StartupDbProbeReport { outcome, fast_unknown: true }
            } else {
                StartupDbProbeReport { outcome, fast_unknown: false }
            }
        }
        confirm_outcome = &mut confirm => {
            // 扩展观察先到（只可能早于 fast 的明确结论）：它本身是明确结论（Ok/refused）；
            // 若它也是超时，按未知处理（理论上 confirm_bound ≥ fast_bound）。
            let outcome = classify_connect(confirm_outcome.ok(), &dsn.host, dsn.port);
            StartupDbProbeReport { outcome, fast_unknown: true }
        }
    }
}

/// 直连 MySQL 读 `stditems`（`MySqlDB.LoadItemsDB`，`src/GameSrv/DB/MySqlDB.cs:13`）。
///
/// 连接串走 [`LegacyDsn`]（与 db-srv/login-srv 同一份解析，勿再手写一份）。失败只返回错误，
/// **不 panic** —— 调用方按 oracle 的「加载失败 ⇒ 空物品系统」降级。
///
/// # Errors
/// 连接串解析失败、连库失败、`SELECT * FROM stditems` 失败时返回人话错误。
pub async fn load_items(conn: &str) -> Result<Vec<StdItem>, String> {
    let dsn = LegacyDsn::parse(conn).map_err(|e| format!("ConnctionString 解析失败：{e}"))?;
    let pool = sqlx::mysql::MySqlPoolOptions::new()
        .max_connections(2)
        .connect(&dsn.to_sqlx_url())
        .await
        .map_err(|e| format!("连接 MySQL 失败（读 stditems）：{e}"))?;
    // `monster_power_rate` 只被 `load_monster_db` 用，物品表不读它；照 tests 的口径传 10。
    DataLoaders::new(pool, 10)
        .load_items_db()
        .await
        .map_err(|e| format!("加载 stditems 失败：{e}"))
}

/// 把 `LoadItemsDB` 的**顺序表**装成 oracle `GameItemSystem` 的查表目录。
///
/// - **1-based**：表内位置 `i` ⇒ `Index = i + 1`（`GameItemSystem.cs:133` 的 `item.Index = (ushort)(i + 1)`）；
/// - **空名按缺**：`Name` 为空的记录不进目录（oracle `GetStdItem` 的空名分支，`GameItemSystem.cs:28-31`）；
/// - 超过 `u16` 表（`Index` 溢出）的尾部记录一律丢弃（oracle 侧 `Index` 也是 `ushort`）。
#[must_use]
pub fn catalog_from(items: &[StdItem]) -> StdItemCatalog {
    let entries = items.iter().enumerate().filter_map(|(i, item)| {
        if item.name.is_empty() {
            return None;
        }
        let index = u16::try_from(i + 1).ok()?;
        Some((index, item.clone()))
    });
    StdItemCatalog::from_items(entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_svr::ability::ItemCatalog;

    fn item(name: &str, shape: u8) -> StdItem {
        StdItem {
            name: name.to_string(),
            shape,
            ..StdItem::default()
        }
    }

    /// **探活牙（review F1/A4）**：不可达端口必须判 `ConnectFailed`（快速失败），
    /// 绝不退化成 `Connected`。
    ///
    /// 注意：本机（HIPS/WFP 过滤）对「端口关闭」的 RST 约 2s 才回，快速观察 500ms 会先超时，
    /// 由扩展观察定案为 `ConnectFailed`。这与「不可达」的最终判据一致。
    ///
    /// 红检：把 `probe_reachable` 改成恒 `Connected`（去掉快速失败）⇒ 本用例红。
    #[tokio::test]
    async fn probe_reports_unreachable_port() {
        let mut unreachable = None;
        // `bind(:0)` 拿号后立即 drop ⇒ 该端口无监听；若被并行进程抢走就换一个再试。
        for _ in 0..3 {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind 临时端口失败");
            let port = listener.local_addr().expect("读临时端口失败").port();
            drop(listener);
            let conn = format!("server=127.0.0.1;uid=root;pwd=;database=mir2;port={port};");
            let report = probe_reachable(
                &conn,
                std::time::Duration::from_millis(500),
                std::time::Duration::from_secs(3),
            )
            .await;
            if let StartupDbProbe::ConnectFailed(e) = report.outcome {
                unreachable = Some(e);
                break;
            }
        }
        assert!(
            unreachable.is_some(),
            "不可达端口必须判 ConnectFailed（快速失败）；3 次都没拿到不可达端口（被并行进程抢走？）"
        );
    }

    /// **A3/A5 牙（卡 `openmir2-gamesvr-db-probe-threshold`）**：connect 超时是**第三态**
    /// `TimedOut`（可达性未知），绝不并入 `ConnectFailed`；refused 仍是 `ConnectFailed`。
    ///
    /// 用 [`classify_connect`] 直接喂「上界已到」（`None`）⇒ 不依赖真机延迟，确定性。
    /// 红检（A5 变异）：把超时分支改成返回 `ConnectFailed` ⇒ 本用例红。
    #[test]
    fn timeout_is_unknown_and_refused_is_unreachable() {
        let timed_out = classify_connect(None, "127.0.0.1", 3306);
        assert!(
            matches!(timed_out, StartupDbProbe::TimedOut(_)),
            "connect 超时必须判 TimedOut（可达性未知），实得 {timed_out:?}"
        );
        assert_eq!(
            timed_out.skip_startup_tables(),
            None,
            "Timeout ⇒ 不得跳过四表（A1 的决策面）"
        );
        let report = StartupDbProbeReport {
            outcome: timed_out,
            fast_unknown: true,
        };
        assert_eq!(
            report.skip_startup_tables(),
            None,
            "报告层的唯一判据也必须拒绝「超时 ⇒ 跳过」"
        );

        let refused = classify_connect(
            Some(Err(std::io::Error::from(
                std::io::ErrorKind::ConnectionRefused,
            ))),
            "127.0.0.1",
            3306,
        );
        assert!(
            matches!(refused, StartupDbProbe::ConnectFailed(_)),
            "refused 必须判 ConnectFailed（明确不可达），实得 {refused:?}"
        );
        assert!(
            refused.skip_startup_tables().is_some(),
            "Refused ⇒ 必须跳过四表（A2 的决策面）"
        );
    }

    /// **A1 反向牙**：可达端点（TCP accept 在快速窗口内）⇒ `Connected` 且 `fast_unknown=false`，
    /// 扩展观察被丢弃、不拖慢快路径。红检：把 `Connected` 判成 `TimedOut` ⇒ 本用例红。
    #[tokio::test]
    async fn reachable_endpoint_is_connected_in_fast_window() {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("bind 临时监听失败");
        let port = listener.local_addr().expect("读临时端口失败").port();
        let conn = format!("server=127.0.0.1;uid=root;pwd=;database=mir2;port={port};");
        let started = std::time::Instant::now();
        let report = probe_reachable(
            &conn,
            std::time::Duration::from_millis(500),
            std::time::Duration::from_secs(3),
        )
        .await;
        let elapsed = started.elapsed();
        assert!(
            matches!(report.outcome, StartupDbProbe::Connected),
            "可达端点必须判 Connected，实得 {:?}",
            report.outcome
        );
        assert!(
            !report.fast_unknown,
            "快速窗口内已有结论 ⇒ fast_unknown=false"
        );
        assert!(
            elapsed < std::time::Duration::from_millis(500),
            "快路径不得等扩展观察；实测 {elapsed:?}"
        );
    }

    /// **A3 三态可区分**：同一输入域里 `Connected` / `TimedOut` / `ConnectFailed` 互不混淆，
    /// 且只有 `ConnectFailed` 允许跳过四表。
    #[tokio::test]
    async fn probe_states_are_distinct_and_only_refused_skips() {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("bind 临时监听失败");
        let addr = listener.local_addr().expect("读临时端口失败");
        let sock = tokio::net::TcpStream::connect(addr)
            .await
            .expect("connect 临时监听失败");
        let connected = classify_connect(Some(Ok(sock)), "127.0.0.1", addr.port());
        assert!(
            matches!(connected, StartupDbProbe::Connected),
            "connect 成功必须判 Connected，实得 {connected:?}"
        );
        assert_eq!(
            connected.skip_startup_tables(),
            None,
            "Connected 不跳过四表"
        );
        let dsn = StartupDbProbe::DsnError("x".to_string());
        assert_eq!(dsn.skip_startup_tables(), None, "DsnError 不触发跨表跳过");
    }

    /// **A1 牙（卡 `openmir2-gamesvr-db-probe-dns-once`）**：同一主机名的 fast/confirm
    /// 两条 connect 之间**只解析一次**，且都复用同一个 `SocketAddr`。
    ///
    /// 红检（A4 变异）：把实现改回「两条 connect 各自解析」（或各调一次 resolver）⇒
    /// 计数变 2 ⇒ 本用例红。
    #[tokio::test]
    async fn resolution_happens_once_and_both_connects_share_addr() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        struct CountingResolver {
            addr: std::net::SocketAddr,
            calls: AtomicUsize,
        }
        impl AddrResolver for CountingResolver {
            fn resolve<'a>(&'a self, _host: &'a str, _port: u16) -> ResolveFuture<'a> {
                self.calls.fetch_add(1, Ordering::SeqCst);
                let addr = self.addr;
                Box::pin(async move { Ok(addr) })
            }
        }

        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("bind 临时监听失败");
        let addr = listener.local_addr().expect("读临时端口失败");
        let resolver = CountingResolver {
            addr,
            calls: AtomicUsize::new(0),
        };
        // 用**主机名**而不是 IP：本用例的契约就是「同一个主机名只解析一次」。
        let conn = format!(
            "server=db.example.invalid;uid=root;pwd=;database=mir2;port={};",
            addr.port()
        );
        let report = probe_reachable_with(
            &conn,
            std::time::Duration::from_millis(500),
            std::time::Duration::from_secs(3),
            &resolver,
        )
        .await;
        assert!(
            matches!(report.outcome, StartupDbProbe::Connected),
            "注入解析器返回可达地址 ⇒ Connected；实得 {:?}",
            report.outcome
        );
        assert_eq!(
            resolver.calls.load(Ordering::SeqCst),
            1,
            "fast/confirm 必须复用同一解析结果（解析调用次数 = 1）；变 2 ⇒ 回到「两次解析落到不同后端」的假跳过 bug"
        );
    }

    /// **A2 牙**：解析失败 ⇒ `ConnectFailed` ⇒ 跳过四表（与旧实现里 connect 的 DNS
    /// 失败同语义：明确不可达）。
    #[tokio::test]
    async fn resolution_failure_is_connect_failed_and_skips_tables() {
        struct FailingResolver;
        impl AddrResolver for FailingResolver {
            fn resolve<'a>(&'a self, _host: &'a str, _port: u16) -> ResolveFuture<'a> {
                Box::pin(async move { Err(std::io::Error::other("dns: no such host")) })
            }
        }

        let report = probe_reachable_with(
            "server=db.example.invalid;uid=root;pwd=;database=mir2;port=3306;",
            std::time::Duration::from_millis(500),
            std::time::Duration::from_secs(3),
            &FailingResolver,
        )
        .await;
        match &report.outcome {
            StartupDbProbe::ConnectFailed(e) => {
                assert!(
                    e.contains("db.example.invalid:3306"),
                    "错误必须点名 host:port；实得 {e}"
                );
                assert!(
                    e.contains("dns: no such host"),
                    "错误必须保留解析失败原因；实得 {e}"
                );
            }
            other => panic!("解析失败必须判 ConnectFailed，实得 {other:?}"),
        }
        assert!(
            report.skip_startup_tables().is_some(),
            "解析失败 = 明确不可达 ⇒ 必须跳过四表"
        );
        assert!(
            !report.fast_unknown,
            "解析失败是明确结论，不得标记 fast_unknown"
        );
    }

    /// **解析上界回归牙**：DNS 解析必须像旧实现一样受扩展观察上界约束 —— 裸
    /// `resolve().await` 会让启动探活无界（旧实现里 DNS 包在 connect 的 1s/2.5s 上界内）。
    ///
    /// 红检（A4 同族）：去掉 `timeout(confirm_bound, resolver.resolve(...))` 的外层上界
    /// ⇒ 内部 future 永不完成 ⇒ 外层 2s 兜底超时 ⇒ 本用例红（而不是挂死）。
    #[tokio::test]
    async fn resolution_is_bounded_by_confirm_bound() {
        struct HangingResolver;
        impl AddrResolver for HangingResolver {
            fn resolve<'a>(&'a self, _host: &'a str, _port: u16) -> ResolveFuture<'a> {
                Box::pin(std::future::pending::<std::io::Result<std::net::SocketAddr>>())
            }
        }

        let confirm_bound = std::time::Duration::from_millis(150);
        let started = std::time::Instant::now();
        let report = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            probe_reachable_with(
                "server=db.example.invalid;uid=root;pwd=;database=mir2;port=3306;",
                std::time::Duration::from_millis(50),
                confirm_bound,
                &HangingResolver,
            ),
        )
        .await
        .expect("解析必须在扩展观察上界内返回（裸 resolve 无界 ⇒ 外层 2s 兜底触发）");
        assert!(
            matches!(report.outcome, StartupDbProbe::TimedOut(_)),
            "解析无结论 ⇒ TimedOut（未知，继续四表）；实得 {:?}",
            report.outcome
        );
        assert!(report.fast_unknown, "解析无结论 ⇒ fast_unknown=true");
        assert_eq!(
            report.skip_startup_tables(),
            None,
            "解析无结论 ⇒ 不得跳过四表"
        );
        // 下界（安全的墙钟断言）：证明确实等到了上界，而不是提前返回。
        assert!(
            started.elapsed() >= confirm_bound,
            "必须等满上界 {confirm_bound:?}，实测 {:?}",
            started.elapsed()
        );
    }

    /// 1-based + 空名按缺：`Index=1` ⇒ `items[0]`；空名记录处的位置仍占号。
    #[test]
    fn catalog_is_one_based_and_skips_empty_names() {
        let table = vec![item("木剑", 5), item("", 9), item("布衣", 3)];
        let catalog = catalog_from(&table);
        assert_eq!(catalog.shape(1), Some(5), "Index=1 ⇒ items[0]（木剑）");
        assert_eq!(catalog.shape(2), None, "空名记录按 oracle GetStdItem 计缺");
        assert_eq!(
            catalog.shape(3),
            Some(3),
            "空名记录仍占位置：Index=3 ⇒ items[2]"
        );
        assert_eq!(catalog.shape(0), None, "Index=0 越界 ⇒ None");
        assert_eq!(catalog.shape(4), None, "越界 ⇒ None");
        assert_eq!(catalog_from(&[]).shape(1), None, "空表 ⇒ 恒 None");
    }
}
