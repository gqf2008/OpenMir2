//! 攻城文件解析的离线 dump（M3⑧-6 第①片证据件）。
//!
//! 用法（在 `mir2-rs/` 下）：
//! ```sh
//! cargo run -p mir2-envir --example dump_castle_files -- <SabukW.txt> [AttackSabukWall.txt]
//! ```
//! 只读；把 [`mir2_envir::castle::SabukWConfig`] 的逐字段读数打到 stdout，供证据包 `03-rust-vectors.txt`。
//! 传入的 `SabukW.txt` 按 GBK/BOM 口径解码（与 oracle `ConfigFile.Load` 同）。

use mir2_envir::castle::{parse_attack_sabuk_wall_line, parse_sabuk_w_bytes};

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(sabuk) = args.next() else {
        eprintln!("usage: dump_castle_files <SabukW.txt> [AttackSabukWall.txt]");
        std::process::exit(2);
    };
    let bytes = std::fs::read(&sabuk).expect("读 SabukW.txt");
    let cfg = parse_sabuk_w_bytes(&bytes);
    println!("sabuk_w={sabuk}");
    println!("castle_name={}", cfg.castle_name);
    println!("own_guild={}", cfg.own_guild);
    println!("change_date={:?}", cfg.change_date);
    println!("war_date={:?}", cfg.war_date);
    println!("income_today={:?}", cfg.income_today);
    println!("total_gold={}", cfg.total_gold);
    println!("today_income={}", cfg.today_income);
    println!("envir_list={:?}", cfg.envir_list);
    println!("map_name={}", cfg.map_name);
    println!("home_map={}", cfg.home_map);
    println!("home=({},{})", cfg.home_x, cfg.home_y);
    println!("war_range=({},{})", cfg.war_range_x, cfg.war_range_y);
    println!("palace_map={}", cfg.palace_map);
    println!("secret_map={}", cfg.secret_map);
    println!("palace_door=({},{})", cfg.palace_door_x, cfg.palace_door_y);
    println!("main_door_open={}", cfg.main_door_open);
    for (kind, unit) in cfg.defense_units() {
        println!(
            "{kind}\t{}\tx={}\ty={}\thp={}",
            unit.name, unit.x, unit.y, unit.hp
        );
    }

    if let Some(attack) = args.next() {
        let bytes = std::fs::read(&attack).expect("读 AttackSabukWall.txt");
        let text = mir2_envir::text::decode_text(&bytes);
        println!("attack_sabuk_wall={attack}");
        for line in text.lines() {
            if let Some(e) = parse_attack_sabuk_wall_line(line) {
                println!(
                    "ATTACK\t{}\t{}\tparsed={}",
                    e.guild_name, e.attack_date, e.attack_date_parsed
                );
            }
        }
    }
}
