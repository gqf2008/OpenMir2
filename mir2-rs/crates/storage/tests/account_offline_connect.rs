//! 卡 `openmir2-login-srv-dsn-source` 续作（reply-chain ⑤）：**离线连接复现**。
//!
//! 生产事实（2026-10-29 重切窗 + 本机实证）：
//! - 本机 mysqld（`127.0.0.1:3306`，MariaDB 10.11.19）对 `mysql -u root`（**不发送口令**）
//!   长期可连；账号表 `account` 仅在 `mir2_account`（`mir2_db` 无此表）；
//! - 两个 conf 的 `ConnctionString` 凭据均为陈旧配置：`config.conf` 指向不可达的
//!   `10.10.0.199`（旧库机）且 pwd 为 6 字符旧口令；`dbsvr.conf` pwd 为空但 **SQL 客户端
//!   上发空串口令 ⇒ 1045 `Access denied … (using password: YES)`**（`mysql.exe` 无 -p
//!   则不上发 ⇒ 可连）。
//!
//! 本测试用**组装后的精确连接选项**（user=`root`、**无 password 字段**、host
//! `127.0.0.1:3306`、database=`mir2_account`）连本机 mysqld 并查询 `account` 表：
//! CLI 可连而 rust 客户端失败 ⇒ 语义差异点在客户端组装，本测试钉住直到修复。
//! 本机无 3306 监听 ⇒ 离线环境跳过。

use sqlx::mysql::MySqlConnectOptions;

fn mysql_3306_listening() -> bool {
    std::net::TcpStream::connect("127.0.0.1:3306").is_ok()
}

fn assembled_authoritative_options() -> MySqlConnectOptions {
    // 与 bins/login-srv/src/lib.rs `authoritative_account_dsn` + storage
    // `init_or_empty` 的组装**逐字段同形**：user=root、无 password 字段、
    // 127.0.0.1:3306、database=mir2_account。
    MySqlConnectOptions::new()
        .host("127.0.0.1")
        .port(3306)
        .username("root")
        .database("mir2_account")
}

/// 组装后的精确连接选项 ⇒ 必须能连上本机 mysqld 并查到账号行。
/// （CLI `mysql -u root` 同参可连 ⇒ 若 rust 失败即为客户端组装语义差异。）
#[tokio::test]
async fn assembled_root_no_password_options_connect_local_mysqld() {
    if !mysql_3306_listening() {
        eprintln!("skip: 本机 3306 无监听（离线环境）");
        return;
    }
    let pool = sqlx::mysql::MySqlPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(std::time::Duration::from_secs(5))
        .connect_lazy_with(assembled_authoritative_options());
    let (id, account): (i64, String) =
        sqlx::query_as("SELECT Id, Account FROM account ORDER BY Id DESC LIMIT 1")
            .fetch_one(&pool)
            .await
            .expect("组装选项（root/无 password/127.0.0.1:3306/mir2_account）连接+查询必须成功");
    assert!(id > 0);
    assert!(!account.is_empty());
}

/// 同选项经 `AccountStore::init_or_empty` ⇒ 账号表必须非空装载（fail-soft 路径不吞差异：
/// 装载失败 ⇒ 返回 Some(err)，本测试据此红）。
#[tokio::test]
async fn init_or_empty_loads_accounts_with_assembled_options() {
    if !mysql_3306_listening() {
        eprintln!("skip: 本机 3306 无监听（离线环境）");
        return;
    }
    let dsn =
        mir2_storage::LegacyDsn::parse("server=127.0.0.1;uid=root;database=mir2_test_account;")
            .expect("DSN 解析");
    let (store, init_err) = mir2_storage::AccountStore::init_or_empty(&dsn).await;
    assert!(
        init_err.is_none(),
        "组装选项（root/无 password/mir2_test_account）装载必须成功：{init_err:?}"
    );
    assert!(store.count() > 0, "账号缓存必须非空");
}

/// MUT-C（卡 `openmir2-login-srv-dsn-source`，第四次重切决定性实验 C）：**陈旧 config
/// 口令陷阱钉死**——组装选项带 `pwd=123456`（config.conf 陈旧值）⇒ 本机 mysqld 必须
/// 1045 拒绝（`using password: YES`）。该测试红 = 有人把陈旧口令又当有效凭据用了。
#[tokio::test]
async fn assembled_options_with_stale_config_password_denied() {
    if !mysql_3306_listening() {
        eprintln!("skip: 本机 3306 无监听（离线环境）");
        return;
    }
    let opts = MySqlConnectOptions::new()
        .host("127.0.0.1")
        .port(3306)
        .username("root")
        .password("123456") // config.conf 陈旧口令（决定性实验 C：-p123456 ⇒ 1045 YES）
        .database("mir2_account");
    let pool = sqlx::mysql::MySqlPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(std::time::Duration::from_secs(5))
        .connect_lazy_with(opts);
    let err = sqlx::query("SELECT Id, Account FROM account LIMIT 1")
        .fetch_one(&pool)
        .await
        .expect_err("陈旧口令必须被拒（1045）");
    let text = err.to_string();
    assert!(
        text.contains("1045") || text.contains("Access denied"),
        "必须是 1045/Access denied 签名，实得：{text}"
    );
}

/// Index 探针：插入临时行 ⇒ `init_or_empty` ⇒ `index` 必须命中（Map 键/大小写/装载时序）。
#[tokio::test]
async fn index_finds_inserted_row() {
    if !mysql_3306_listening() {
        eprintln!("skip: 本机 3306 无监听（离线环境）");
        return;
    }
    let cleanup = sqlx::mysql::MySqlPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(std::time::Duration::from_secs(5))
        .connect_lazy_with(
            MySqlConnectOptions::new()
                .host("127.0.0.1")
                .port(3306)
                .username("root")
                .database("mir2_test_account"),
        );
    sqlx::query("DELETE FROM account_protection WHERE AccountId IN (SELECT Id FROM account WHERE Account='tooth_idx')")
        .execute(&cleanup)
        .await
        .unwrap();
    sqlx::query("DELETE FROM account WHERE Account='tooth_idx'")
        .execute(&cleanup)
        .await
        .unwrap();
    sqlx::query("INSERT INTO account (Account,PassWord,PassFailCount,PassFailTime,Seconds,LastLoginTime,PayMode,State) VALUES ('tooth_idx','idxpw',0,0,0,0,0,0)")
        .execute(&cleanup)
        .await
        .unwrap();

    let dsn =
        mir2_storage::LegacyDsn::parse("server=127.0.0.1;uid=root;database=mir2_test_account;")
            .expect("DSN 解析");
    let (store, init_err) = mir2_storage::AccountStore::init_or_empty(&dsn).await;
    assert!(init_err.is_none(), "装载必须成功：{init_err:?}");
    let hit = store.index("tooth_idx");
    assert!(
        hit.is_some(),
        "index 必须命中临时行（map_len={}）",
        store.count()
    );
    assert!(hit.unwrap() > 0);
    sqlx::query("DELETE FROM account_protection WHERE AccountId IN (SELECT Id FROM account WHERE Account='tooth_idx')")
        .execute(&cleanup)
        .await
        .unwrap();
    sqlx::query("DELETE FROM account WHERE Account='tooth_idx'")
        .execute(&cleanup)
        .await
        .unwrap();
}
