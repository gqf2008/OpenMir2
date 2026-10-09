//! 门禁 ①：各表加载条数与 C# 启动日志一致。
//!
//! C# 侧数字（`E:\MirServer\logs\GameSvr.out.log`，2026-10-10 00:10:50 启动）：
//!
//! ```text
//! 00:10:50.806 [INF] 物品数据库加载成功...[1000]
//! 00:10:51.786 [INF] 加载怪物数据库成功...[705]
//! 00:10:51.789 [INF] 加载技能数据库成功...[108]
//! 00:10:51.808 [INF] 读取物品寄售列表成功...[0]
//! ```
//!
//! 本测试需要能连到 `mir2_data`（只读 SELECT，不写）。默认 `#[ignore]`，
//! 显式运行：
//!
//! ```text
//! cargo test -p mir2-parity-tests --test table_counts -- --ignored --nocapture
//! ```
//!
//! 连接串取自环境变量 `MIR2_DATA_URL`，缺省 `mysql://root@127.0.0.1:3306/mir2_data`
//! （与现网 `Server.conf` 的 `ConnctionString` 一致）。

use mir2_data::loaders::DataLoaders;

/// C# 启动日志数字（见模块注释的日志摘录）。
const EXPECTED: [(&str, usize); 4] = [
    ("stditems", 1000),
    ("monsters", 705),
    ("magics", 108),
    ("goldsales", 0),
];

#[tokio::test]
#[ignore = "需要本机 MySQL mir2_data；显式 --ignored 运行"]
async fn table_counts_match_csharp_startup_log() {
    let url = std::env::var("MIR2_DATA_URL")
        .unwrap_or_else(|_| "mysql://root@127.0.0.1:3306/mir2_data".to_string());
    let pool = sqlx::mysql::MySqlPoolOptions::new()
        .max_connections(4)
        .connect(&url)
        .await
        .expect("连接 mir2_data 失败");
    // MonsterPowerRate=10（现网 Server.conf:377）
    let loaders = DataLoaders::new(pool, 10);

    let items = loaders.load_items_db().await.expect("stditems 加载失败");
    let monsters = loaders.load_monster_db().await.expect("monsters 加载失败");
    let magics = loaders.load_magic_db().await.expect("magics 加载失败");
    let goldsales = loaders
        .load_sell_off_item_list()
        .await
        .expect("goldsales 加载失败");

    let actual = [
        ("stditems", items.len()),
        ("monsters", monsters.len()),
        ("magics", magics.len()),
        ("goldsales", goldsales.len()),
    ];
    println!("表加载条数对拍（rust vs C# 启动日志）:");
    for ((name, exp), (_, act)) in EXPECTED.iter().zip(actual.iter()) {
        println!("  {name}: rust={act}  csharp={exp}");
        assert_eq!(exp, act, "{name} 条数与 C# 启动日志不一致");
    }
}
