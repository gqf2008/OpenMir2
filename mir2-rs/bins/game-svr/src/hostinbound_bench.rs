//! **离线微基准（不是判据）**：`HostInbound` 热路径上各类线性查找的单价。
//!
//! 卡 `openmir2-p1-hostinbound-index`（Phase A 测量证据）。只在 `cfg(test)` 下编译，
//! 默认 `#[ignore]`，并需 `MIR2_HOSTINBOUND_BENCH=1` 二道门。跑法（release）：
//!
//! ```text
//! CARGO_TARGET_DIR=E:/UserTemp/w93-rel cargo test --release -p game-svr --lib \
//!     -- --ignored --nocapture hostinbound_bench
//! ```
//!
//! P1-2 起第 5 节内建**同轮旧基线**（`chat::hear_targets_at_legacy` vs `hear_targets_at`），
//! 单次运行即得 old_ns/new_ns/ratio，不需要回旧树。
//!
//! 用真实类型（`OnlineRegistry`/`DisplayTable`/`World::session`/`chat::hear_targets_at`），
//! 不改生产代码；每个 primitive 在典型规模（online=500/1000/5000、display=500/1000/5000、
//! 实体=1000/5000/8874）下测单次耗时。dense 场景按 500 档的密度 17.7（500 玩家 / 8874 实体）
//! 铺格，`hear_targets` 的收件人数量与真栈同量级。
use super::*;
use std::collections::HashMap;
use std::hint::black_box;
use std::time::Instant;

const MAP: i32 = 200;
const SEND_RANGE: i32 = 12;

fn make_online(n: usize) -> Vec<OnlineEntry> {
    (0..n)
        .map(|i| OnlineEntry {
            account: format!("acct{i:06}"),
            chr_name: format!("Char{i:06}"),
            session_id: i as i32,
            gate_idx: (i % 4) as i32,
            socket: 9_000 + i as i32,
            actor_id: 1_000_000 + i as i32,
        })
        .collect()
}

/// ns/op，先预热再计时；`f` 用旋转目标避免常量折叠/分支预测偏置。
fn time_ns<F: FnMut() -> usize>(iters: usize, mut f: F) -> f64 {
    let mut acc = 0usize;
    for _ in 0..(iters / 8).max(1) {
        acc ^= f();
    }
    let t = Instant::now();
    for _ in 0..iters {
        acc ^= f();
    }
    let ns = t.elapsed().as_nanos() as f64 / iters as f64;
    black_box(acc);
    ns
}

fn line(label: &str, ns: f64) {
    println!("{label:<58} {:>10.3} ns/op", ns);
}

/// 网格铺法（与 `host_aoi_bench` 同形）：x 隔一格，y 逐行。
fn grid_xy(i: usize) -> (i32, i32) {
    (1 + 2 * (i as i32 % 99), 1 + (i as i32 / 99))
}

fn actor_find(online: &[OnlineEntry], id: i32) -> usize {
    online
        .iter()
        .find(|e| e.actor_id == id)
        .map_or(0usize, |e| e.actor_id as usize)
}

fn chr_find(online: &[OnlineEntry], name: &str) -> usize {
    online
        .iter()
        .find(|e| e.chr_name == name)
        .map_or(0usize, |e| e.actor_id as usize)
}

#[test]
#[ignore = "测量 harness：需 MIR2_HOSTINBOUND_BENCH=1 + --ignored --nocapture；见卡 `openmir2-p1-hostinbound-index` 的 patch body"]
fn hostinbound_bench_primitives() {
    if std::env::var("MIR2_HOSTINBOUND_BENCH").ok().as_deref() != Some("1") {
        eprintln!("MIR2_HOSTINBOUND_BENCH != 1 不跑测量（这是 harness，不是判据）");
        return;
    }

    println!("== 1) OnlineRegistry: Vec<OnlineEntry> 线性查找（n = 在线数） ==");
    for n in [500usize, 1_000, 5_000] {
        let online = make_online(n);
        // 目标位置轮转 0%,25%,50%,100%，取平均（命中位置越靠后越贵；取「平均/最坏」两列）。
        let mut avg_chr = 0.0;
        let mut avg_act = 0.0;
        let mut worst_chr = 0.0;
        let mut worst_act = 0.0;
        for frac in [0usize, 1, 2, 4] {
            let pos = (n * frac / 4).min(n - 1);
            let name = online[pos].chr_name.clone();
            let id = online[pos].actor_id;
            let c = time_ns(20_000, || chr_find(&online, &name));
            let a = time_ns(20_000, || actor_find(&online, id));
            avg_chr += c / 4.0;
            avg_act += a / 4.0;
            if frac == 4 {
                worst_chr = c;
                worst_act = a;
            }
        }
        line(&format!("n={n:<5} online find by chr_name (avg)"), avg_chr);
        line(
            &format!("n={n:<5} online find by chr_name (worst/miss)"),
            worst_chr,
        );
        line(&format!("n={n:<5} online find by actor_id (avg)"), avg_act);
        line(
            &format!("n={n:<5} online find by actor_id (worst/miss)"),
            worst_act,
        );
        // online_take = position + remove
        let take = time_ns(5_000, || {
            let mut v = crate::OnlineRegistry::from(online.clone());
            let pos = n / 2;
            let nm = v[pos].chr_name.clone();
            online_take(&mut v, &nm).map_or(0, |e| e.actor_id as usize)
        });
        line(
            &format!("n={n:<5} online_take (position+remove, incl. clone)"),
            take,
        );
        // HashMap baseline
        let by_chr: HashMap<String, usize> = online
            .iter()
            .enumerate()
            .map(|(i, e)| (e.chr_name.clone(), i))
            .collect();
        let by_act: HashMap<i32, usize> = online
            .iter()
            .enumerate()
            .map(|(i, e)| (e.actor_id, i))
            .collect();
        let pos = n - 1;
        let name = online[pos].chr_name.clone();
        let id = online[pos].actor_id;
        let hc = time_ns(200_000, || by_chr.get(&name).copied().unwrap_or(0));
        let ha = time_ns(200_000, || by_act.get(&id).copied().unwrap_or(0));
        line(&format!("n={n:<5} HashMap<String,_> get"), hc);
        line(&format!("n={n:<5} HashMap<i32,_> get"), ha);
        println!();
    }

    println!("== 2) 扇出：每个接收者一次 online find（k=收件人数，n=在线数） ==");
    {
        let n = 500usize;
        let online = make_online(n);
        let by_act: HashMap<i32, usize> = online
            .iter()
            .enumerate()
            .map(|(i, e)| (e.actor_id, i))
            .collect();
        for k in [1usize, 5, 17, 50] {
            let targets: Vec<i32> = (0..k).map(|i| online[(i * 719) % n].actor_id).collect();
            let vec_fan = time_ns(20_000, || {
                let mut acc = 0usize;
                for &t in &targets {
                    acc ^= actor_find(&online, t);
                }
                acc
            });
            let map_fan = time_ns(20_000, || {
                let mut acc = 0usize;
                for &t in &targets {
                    acc ^= by_act.get(&t).copied().unwrap_or(0);
                }
                acc
            });
            line(&format!("n={n} k={k:<3} fanout via Vec find"), vec_fan);
            line(&format!("n={n} k={k:<3} fanout via HashMap"), map_fan);
        }
        println!();
    }

    println!("== 3) DisplayTable (actor_id -> display)，m = 展示表规模 ==");
    for m in [500usize, 1_000, 5_000, 8_874] {
        let mut dt = downlink::DisplayTable::new();
        for i in 0..m {
            dt.set(
                1_000_000 + i as i32,
                downlink::ActorDisplay::new(0, 0, 0, 0, 0, 0, format!("d{i}").into_bytes()),
            );
        }
        let last = 1_000_000 + (m - 1) as i32;
        let g = time_ns(20_000, || dt.get(last).map_or(0, |d| d.dir as usize));
        let s = time_ns(20_000, || {
            dt.set(
                last,
                downlink::ActorDisplay::new(1, 2, 3, 4, 5, 6, b"x".to_vec()),
            );
            1
        });
        line(&format!("m={m:<5} DisplayTable::get"), g);
        line(&format!("m={m:<5} DisplayTable::set(覆盖)"), s);
        let rm = time_ns(200, || {
            let mut c = dt.clone();
            c.remove(last);
            c.len()
        });
        line(&format!("m={m:<5} DisplayTable::remove (incl. clone)"), rm);
        println!();
    }

    println!("== 4) SessionRegistry::get (chr_name)，s = 会话数 ==");
    for s in [500usize, 1_000, 5_000] {
        let mut world = mir2_world_bevy::World::new(MAP, MAP);
        let mut names = Vec::with_capacity(s);
        for i in 0..s {
            let name = format!("Sess{i:06}");
            world.open_session(&name);
            names.push(name);
        }
        let last = names.last().unwrap().clone();
        let g = time_ns(20_000, || {
            world.session(&last).map_or(0, |x| x.chr_name.len())
        });
        line(&format!("s={s:<5} World::session (Vec find)"), g);
        println!();
    }

    println!("== 5) hear_targets_at（O(全实体) 扫描），E=实体数，players=玩家数 ==");
    for (e_count, players) in [(1_000usize, 500usize), (5_000, 500), (8_874, 500)] {
        let mut world = mir2_world_bevy::World::new(MAP, MAP);
        let mut ids = Vec::with_capacity(e_count);
        for i in 0..e_count {
            let (x, y) = grid_xy(i);
            let mut e = mir2_world_bevy::EntitySpawn::new((i + 1) as i32, format!("e{i}"), x, y);
            e.race = if i < players { 0 } else { 80 };
            e.view_range = 5;
            ids.push(world.add_entity(e));
        }
        let speaker = ids[0];
        let (sx, sy) = grid_xy(0);
        // **P1-2 同轮基线**：旧路径（`iter_ref`，24 组件 + 完整 EntityRef）与新路径
        // （4 组件轻量视图）在**同一次运行、同一个世界**上测；先断言输出逐元素相等，
        // 再报 old_ns / new_ns / ratio。不要像 P1-1 那样把 before 留在另一次运行里。
        let old_targets =
            game_svr::chat::hear_targets_at_legacy(&world, speaker, 0, sx, sy, SEND_RANGE);
        let new_targets = game_svr::chat::hear_targets_at(&world, speaker, 0, sx, sy, SEND_RANGE);
        assert_eq!(
            new_targets, old_targets,
            "E={e_count}: 新旧收件人必须逐元素相等（集合与顺序）"
        );
        // 覆盖牙：轻量扫描必须逐实体覆盖全部 E —— 否则「new 很快」可能只是静默少扫了一部分。
        assert_eq!(
            world.entities.iter_send_ref_targets().count(),
            e_count,
            "E={e_count}: 轻量收件人扫描必须覆盖全部实体"
        );
        let old_ns = time_ns(300, || {
            game_svr::chat::hear_targets_at_legacy(&world, speaker, 0, sx, sy, SEND_RANGE).len()
        });
        let new_ns = time_ns(300, || {
            game_svr::chat::hear_targets_at(&world, speaker, 0, sx, sy, SEND_RANGE).len()
        });
        let hits = new_targets.len();
        line(
            &format!(
                "E={e_count:<5} players={players:<4} hear_targets_at old/iter_ref ({} 收件人)",
                hits
            ),
            old_ns,
        );
        line(
            &format!("E={e_count:<5} players={players:<4} hear_targets_at new/4-comp"),
            new_ns,
        );
        println!(
            "[hear_targets_at] E={e_count:<5} old_ns={old_ns:>10.3} new_ns={new_ns:>10.3} ratio={:.3}",
            new_ns / old_ns
        );
        // 端到端扇出：hear_targets + k 次 online find（online 有 players 条）
        let online = make_online(players);
        let by_act: HashMap<i32, usize> = online
            .iter()
            .enumerate()
            .map(|(i, e)| (e.actor_id, i))
            .collect();
        // 注意：world 实体 id 与 online actor_id 不是同一套（本 harness 的 online 是合成表），
        // 这里只测「k 次查找」的增量，不追求 id 对应。
        let k = hits;
        let fan_vec = time_ns(300, || {
            let targets = game_svr::chat::hear_targets_at(&world, speaker, 0, sx, sy, SEND_RANGE);
            let mut acc = 0usize;
            for &t in &targets {
                acc ^= actor_find(&online, t);
            }
            acc
        });
        let fan_map = time_ns(300, || {
            let targets = game_svr::chat::hear_targets_at(&world, speaker, 0, sx, sy, SEND_RANGE);
            let mut acc = 0usize;
            for &t in &targets {
                acc ^= by_act.get(&t).copied().unwrap_or(0);
            }
            acc
        });
        line(
            &format!("E={e_count:<5} 端到端 fanout: hear_targets + Vec find ×k"),
            fan_vec,
        );
        line(
            &format!("E={e_count:<5} 端到端 fanout: hear_targets + HashMap ×k"),
            fan_map,
        );
        if k > 0 {
            line(
                &format!("E={e_count:<5} 仅 Vec 查找增量 ((fan_vec-fan_map)/k)"),
                (fan_vec - fan_map) / k as f64,
            );
        }
        println!();
    }
}
