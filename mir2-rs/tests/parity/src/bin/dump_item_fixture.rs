//! 夹具导出工具（一次性，只读）：
//! 从真实 `mir2_data.stditems` 导出指定 MonItems 文件引用到的物品子集为 JSON。
//!
//! 用法：
//!   cargo run -p mir2-parity-tests --bin dump_item_fixture -- \
//!     <database_url> <monitems.txt> <out.json>
//!
//! 导出名匹配规则与 C# `CopyToUserItemFromName` 一致（OrdinalIgnoreCase 首个匹配），
//! 保留原表相对顺序，保证「首个匹配」语义在夹具子集上不变。

use mir2_data::loaders::DataLoaders;
use mir2_data::monitems;
use serde::Serialize;

#[derive(Serialize)]
struct ItemFixture {
    name: String,
    std_mode: u8,
    shape: u8,
    dura_max: u16,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 4 {
        eprintln!("usage: dump_item_fixture <database_url> <monitems.txt> <out.json>");
        std::process::exit(2);
    }
    let drops = monitems::load_monitems(std::path::Path::new(&args[2]))?;
    let pool = sqlx::mysql::MySqlPoolOptions::new()
        .max_connections(2)
        .connect(&args[1])
        .await?;
    let loaders = DataLoaders::new(pool, 10);
    let items = loaders.load_items_db().await?;
    let mut fixture: Vec<ItemFixture> = Vec::new();
    for item in &items {
        if drops.iter().any(|d| {
            d.item_name
                .chars()
                .flat_map(char::to_uppercase)
                .eq(item.name.chars().flat_map(char::to_uppercase))
        }) {
            fixture.push(ItemFixture {
                name: item.name.clone(),
                std_mode: item.std_mode,
                shape: item.shape,
                dura_max: item.dura_max,
            });
        }
    }
    let json = serde_json::to_string_pretty(&fixture)?;
    std::fs::write(&args[3], &json)?;
    println!(
        "stditems 全表 {} 行，掉落表 {} 行，导出夹具 {} 行 -> {}",
        items.len(),
        drops.len(),
        fixture.len(),
        args[3]
    );
    Ok(())
}
