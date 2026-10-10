//! 账号存储的**实库**集成测试（默认 `#[ignore]`，需要本机 MySQL）。
//!
//! 跑法（本机现网 DSN，见 `src/LoginSrv/bin/Release/logsrv.conf`）：
//!
//! ```powershell
//! $env:MIR2_ACCOUNT_DSN = "server=127.0.0.1;uid=root;pwd=;database=mir2_account;"
//! cargo test -p mir2-storage --test account_live -- --ignored --nocapture
//! ```
//!
//! 全部为**只读**检查（不改库）——写路径（建号/改密/软删）留给 M1 ②③ 的编排脚本，
//! 避免在共享的现网库上留下测试数据。

use mir2_storage::{AccountStore, LegacyDsn};

fn dsn() -> Option<LegacyDsn> {
    let raw = std::env::var("MIR2_ACCOUNT_DSN").ok()?;
    Some(LegacyDsn::parse(&raw).expect("MIR2_ACCOUNT_DSN 解析失败"))
}

#[tokio::test]
#[ignore = "需要本机 MySQL 与 MIR2_ACCOUNT_DSN"]
async fn loads_real_account_table_and_finds_test_account() {
    let Some(dsn) = dsn() else {
        panic!("未设置 MIR2_ACCOUNT_DSN");
    };
    let store = AccountStore::connect(&dsn)
        .await
        .expect("连接 mir2_account 失败");
    println!("内存账号表条目数：{}", store.count());
    assert!(
        store.count() > 0,
        "账号表为空（行为等价下 C# 也会全登录失败）"
    );

    // 设计文档 §2 的测试账号必须能被内存表找到（否则 M1 ② 无从谈起）
    let id = store.index("mir2test").expect("账号 mir2test 不在内存表");
    println!("mir2test → Id={id}");
    let rec = store
        .get(id)
        .await
        .expect("查询失败")
        .expect("mir2test 取不到资料（account_protection 行缺失？）");
    assert_eq!(rec.account, "mir2test");
    assert_eq!(
        rec.password, "mir2pass",
        "口令应明文可比（行为等价优先，勿哈希）"
    );
    println!(
        "mir2test: PassWord={:?} UserName={:?} PassFailCount={} Seconds={} PayMode={}",
        rec.password, rec.user_name, rec.error_count, rec.play_time, rec.pay_model
    );

    // 大小写不敏感（C# StringComparer.OrdinalIgnoreCase）
    assert_eq!(store.index("MIR2TEST"), Some(id), "账号查找应大小写不敏感");
}

/// INNER JOIN 语义：`account` 有行但没有 `account_protection` 行时，`Get` 必须取不到
/// （对应"资料读取失败"，设计文档 §5.1 点名过的坑）。
#[tokio::test]
#[ignore = "需要本机 MySQL 与 MIR2_ACCOUNT_DSN"]
async fn account_without_protection_row_is_invisible() {
    let Some(dsn) = dsn() else {
        panic!("未设置 MIR2_ACCOUNT_DSN");
    };
    let store = AccountStore::connect(&dsn).await.expect("连接失败");
    // 直接查库找一个「有 account 行、无 protection 行」的 Id
    let pool = sqlx::MySqlPool::connect(&dsn.to_sqlx_url())
        .await
        .expect("连池失败");
    let row = sqlx::query(
        "SELECT a.Id FROM account a LEFT JOIN account_protection b ON a.Id=b.AccountId \
         WHERE b.AccountId IS NULL AND a.State=0 ORDER BY a.Id DESC LIMIT 1",
    )
    .fetch_optional(&pool)
    .await
    .expect("查询失败");
    match row {
        None => {
            println!("现网库当前没有「缺 protection 行」的账号，跳过该断言（语义由 C# 源钉住）");
        }
        Some(r) => {
            let id: i32 = sqlx::Row::try_get(&r, "Id").unwrap_or_default();
            let got = store.get(id).await.expect("查询失败");
            println!("缺 protection 行的账号 Id={id} → get() = {got:?}");
            assert!(
                got.is_none(),
                "INNER JOIN 语义丢了：缺 protection 行仍取到了资料"
            );
        }
    }
}
