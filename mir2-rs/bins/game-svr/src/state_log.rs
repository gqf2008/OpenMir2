//! M3②③ 夹具：Rust 侧**服务端权威逐击状态 dump**（`SERVER_STATE <seq> <json 数组>`）。
//!
//! **为什么需要它**（owner 2026-10-12 23:50 第 ③ 条 / §5.254；夹具 `run_attack_pair.ps1:1164`
//! 明写「-Side rust 无服务端状态 dump（归 m2-svr）」）：C# 侧「目标格 `(18,20)` 从 `k=4` 起为空」
//! **不是格子空了**——C# 影子 hook 的服务端 dump（`StartupHook.cs::DumpActors`）证明受害者一直站在
//! `(18,20)`，只是它的 `Ghost` 标志被某条 oracle 路径静默置 1；`Envir.GetMovingObject(x,y,true)` 对
//! `Ghost` 直接返回 null ⇒ `GetPoseCreate()` 为 null ⇒ `_Attack`/`AttackSuccess` 提前返回。
//! **客户端视图看不到这个 ghost** ⇒ 夹具必须用**服务端权威状态**判，否则会拿「假的有目标窗口」去比。
//! Rust 侧缺这一段 ⇒ `server_state_gate.py` 这道 fail-closed 门**目前只在 C# 侧强制**（只保护一半）。
//!
//! **口径（与 `StartupHook.cs::DumpActors` 同形）**：
//! - **触发点** = 每个**玩家攻击单元**（C# 是玩方 `GetAttackPower@PlayObject.cs:2929` 的首抽；Rust 是
//!   [`crate::handle_hit_cmd`] 判定「本击进入挥砍/抽攻击力」的那一点），**与玩家单元同序**；
//! - **内容** = 权威世界 `world.entities` 里全部**玩家族**（`race == 0` = `ActorRace.Play`）对象的
//!   `id/name/x/y/dir/ghost/death/hp/maxhp/map`；
//! - **开关** = 环境变量 `MIR2_RNG_STATE_LOG`（与 C# `start-csharp-hook.ps1 -StateLog` 同名）。
//!   **未设/空 ⇒ 完全不写**（不建文件、不取数、不打印），生产路径与改动前逐字节相同。
//!
//! **登记（与 C# 的微差，均被 `server_state_gate.py` 的 `line.strip()` 归一，不影响判据）**：
//! - 行结束：C# 的 `StreamWriter.WriteLine` 在 Windows 上写 `\r\n`，这里写 `\n`；
//! - `name`/`map`：C# 直接拼接（不转义），这里做最小 JSON 转义（`\"`/`\\`/控制字符）——真实角色名无
//!   这些字符 ⇒ 逐字节同形，且避免名字带引号时把整行打成非法 JSON（gate 会 PREREQ）；
//! - `dir`：C# dump 发生在 `Dir = nDir`（`BaseObject.Attack.cs:12`）**之前**（旧朝向），Rust 在本击
//!   处理返回后 dump（`Entity.ai.dir` 已是本帧 `dir`）；gate 不读 `dir`，仅登记。

use std::io::Write;

use mir2_envir::MapRegistry;
use mir2_world::World;

use crate::downlink::DisplayTable;

/// C# 影子 hook 同名环境变量（`StartupHook.cs`：`MIR2_RNG_STATE_LOG`）。
pub const STATE_LOG_ENV: &str = "MIR2_RNG_STATE_LOG";

/// 服务端权威逐击状态 dump。`out == None` ⇒ 关闭（生产默认）。
pub struct StateLog {
    out: Option<std::io::BufWriter<std::fs::File>>,
    seq: u64,
}

impl StateLog {
    /// 关闭态：不写任何东西（单测/未设环境变量的生产路径用它）。
    #[must_use]
    pub fn disabled() -> Self {
        StateLog { out: None, seq: 0 }
    }

    /// 从环境变量 [`STATE_LOG_ENV`] 构造：非空 ⇒ `Create`（截断，镜像 C# `FileMode.Create`）打开；
    /// 空/未设/打开失败 ⇒ 关闭态（**不** fail-closed：这是调试 dump，缺它不影响世界模拟；未设时
    /// 行为必须与改动前逐字节相同）。
    #[must_use]
    pub fn from_env() -> Self {
        Self::from_path(std::env::var(STATE_LOG_ENV).ok().as_deref())
    }

    /// 供构造性用例注入路径（不读环境变量）。
    #[must_use]
    pub fn from_path(path: Option<&str>) -> Self {
        let Some(p) = path.filter(|p| !p.is_empty()) else {
            return StateLog::disabled();
        };
        match std::fs::File::create(p) {
            Ok(f) => StateLog {
                out: Some(std::io::BufWriter::new(f)),
                seq: 0,
            },
            Err(e) => {
                crate::glog!(
                    "WRN",
                    "服务端逐击状态 dump 无法建文件 {p}：{e} ⇒ 关闭（不影响世界模拟）"
                );
                StateLog::disabled()
            }
        }
    }

    #[must_use]
    pub fn enabled(&self) -> bool {
        self.out.is_some()
    }

    /// 落一行 `SERVER_STATE <seq> <json 数组>`（关闭态为 no-op）。
    ///
    /// `dir` 取**实体** `Entity.ai.dir`（oracle `BaseObject.Dir`；展示表只是投影，走/跑被挡时
    /// 可能落后）；`registry` 提供 `map_id → MapName`。缺注册表时对应字段退化为空串（不 panic）。
    pub fn record(
        &mut self,
        world: &World,
        _displays: &DisplayTable,
        registry: Option<&MapRegistry>,
    ) {
        let Some(out) = self.out.as_mut() else {
            return;
        };
        let mut line = String::from("[");
        let mut first = true;
        for e in world.entities.iter() {
            // 只看玩家族（C# `Race==0` == `ActorRace.Play`），与 `DumpActors` 一致。
            if e.race != mir2_world::ACTOR_RACE_PLAY {
                continue;
            }
            // `Entity.ai.dir` 是 `BaseObject.Dir` 的唯一权威（展示表只是投影）。
            let dir = e.dir();
            let map = registry
                .and_then(|r| r.name_of(e.map_id))
                .unwrap_or_default();
            if !first {
                line.push(',');
            }
            first = false;
            line.push_str(&format!(
                "{{\"id\":{},\"name\":\"{}\",\"x\":{},\"y\":{},\"dir\":{},\"ghost\":{},\"death\":{},\"hp\":{},\"maxhp\":{},\"map\":\"{}\"}}",
                e.id,
                json_escape(&e.name),
                e.x,
                e.y,
                dir,
                u8::from(e.ghost),
                u8::from(e.death),
                e.hp,
                e.max_hp,
                json_escape(map),
            ));
        }
        line.push(']');
        self.seq += 1;
        // C# `AutoFlush = true`：每行落盘（崩溃/被杀也留得住）。
        let _ = writeln!(out, "SERVER_STATE {} {}", self.seq, line);
        let _ = out.flush();
    }
}

/// 最小 JSON 字符串转义（C# `DumpActors` 不转义；真实角色名/地图名无这些字符 ⇒ 逐字节同形）。
fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    /// 构造一个 attacker `(17,20)`、victim `(18,20)` 的两玩家世界（`0137` 的 pin 摆位）。
    fn two_player_world() -> (World, i32, i32) {
        let mut world = mir2_world::World::with_maps(vec![(64, 64)]);
        let atk = {
            let mut e = mir2_world::Entity::new(0, "m3sd0".to_string(), 17, 20);
            e.race = mir2_world::ACTOR_RACE_PLAY;
            e.hp = 19;
            e.max_hp = 19;
            world.add_entity(e)
        };
        let vic = {
            let mut e = mir2_world::Entity::new(0, "m3sd1".to_string(), 18, 20);
            e.race = mir2_world::ACTOR_RACE_PLAY;
            e.hp = 19;
            e.max_hp = 19;
            world.add_entity(e)
        };
        // 夹一个怪物，验证 dump **只**落玩家族（`Race==0`）。
        let mut mon = mir2_world::Entity::new(0, "mon".to_string(), 19, 20);
        mon.race = mir2_world::ACTOR_RACE_MONSTER;
        world.add_entity(mon);
        // 注册表用 0 号图的真名（若上游无注册表则字段退化为空串，gate 不读 map）。
        (world, atk, vic)
    }

    fn read_to_string(path: &std::path::Path) -> String {
        let mut s = String::new();
        std::fs::File::open(path)
            .unwrap()
            .read_to_string(&mut s)
            .unwrap();
        s
    }

    /// 构造性判据 ①：一行一个玩家攻击单元，内容 = 玩家族状态（与 `DumpActors` 同形）。
    /// 红检：把 `e.race != ACTOR_RACE_PLAY` 的过滤删掉（怪物也落）⇒ 本用例必红。
    #[test]
    fn record_emits_player_actors_only_and_matches_gate_semantics() {
        let dir = std::env::temp_dir().join(format!(
            "gs-state-log-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("state.tsv");
        let mut log = StateLog::from_path(Some(path.to_str().unwrap()));
        assert!(log.enabled());
        let (world, _atk, vic) = two_player_world();
        let mut displays = DisplayTable::new();
        displays.set(
            1,
            crate::downlink::ActorDisplay::new(2, 0, 0, 0, 0, 255, b"".to_vec()),
        );
        // 两次「玩家攻击单元」⇒ 两行。
        log.record(&world, &displays, None);
        log.record(&world, &displays, None);

        let text = read_to_string(&path);
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 2, "两个攻击单元必须落两行：{text:?}");
        assert!(
            lines[0].starts_with("SERVER_STATE 1 ["),
            "行首/序号：{}",
            lines[0]
        );
        assert!(
            lines[1].starts_with("SERVER_STATE 2 ["),
            "序号单调：{}",
            lines[1]
        );

        // 与 `server_state_gate.py` 的 `victim_available` 同口径：非 attacker 的玩家族活体在 (18,20)。
        let actors = parse_actors(lines[1]);
        assert_eq!(
            actors.len(),
            2,
            "必须只含 2 个玩家（怪物被过滤）：{actors:?}"
        );
        assert!(
            actors.iter().all(|a| a.name != "mon"),
            "怪物 `mon` 不得进 dump：{actors:?}"
        );
        let found = actors
            .iter()
            .find(|a| a.id != 1 && a.x == 18 && a.y == 20 && a.ghost == 0 && a.death == 0);
        assert!(
            found.is_some(),
            "gate 必须能在 (18,20) 找到 !ghost && !death 的受害者：{actors:?}"
        );
        assert_eq!(found.unwrap().id, vic);
    }

    /// 构造性判据 ②：**未设环境变量 ⇒ 关闭、不建文件、行为与改动前相同**。
    /// 红检：把 `from_path(None)` 改成默认打开某固定文件 ⇒ 本用例必红。
    #[test]
    fn disabled_when_env_unset_never_writes() {
        let log = StateLog::from_path(None);
        assert!(!log.enabled(), "未设路径必须是关闭态");
        let log2 = StateLog::from_path(Some(""));
        assert!(!log2.enabled(), "空路径必须是关闭态");
    }

    /// 构造性判据 ③：设置环境变量（`from_env`）⇒ 建文件并落行；与 `from_path` 同路径。
    #[test]
    fn from_env_writes_when_set() {
        let dir = std::env::temp_dir().join(format!(
            "gs-state-log-env-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("env-state.tsv");
        std::env::set_var(STATE_LOG_ENV, path.to_str().unwrap());
        let mut log = StateLog::from_env();
        std::env::remove_var(STATE_LOG_ENV);
        assert!(log.enabled(), "设了 {STATE_LOG_ENV} 必须打开");
        let (world, _a, _v) = two_player_world();
        log.record(&world, &DisplayTable::new(), None);
        let text = read_to_string(&path);
        assert!(text.starts_with("SERVER_STATE 1 ["), "{text:?}");
    }

    #[derive(Debug)]
    struct Actor {
        id: i32,
        name: String,
        x: i32,
        y: i32,
        ghost: u8,
        death: u8,
    }

    /// 解析 `SERVER_STATE <seq> <json>`（镜像 gate 的 `parse_dump` + `json` 结构），**不依赖 python**。
    fn parse_actors(line: &str) -> Vec<Actor> {
        let rest = line.strip_prefix("SERVER_STATE ").expect("prefix");
        let sp = rest.find(' ').expect("seq 后空格");
        let payload = &rest[sp + 1..];
        parse_json_array(payload)
    }

    /// 极简 JSON 数组解析器（只吃本 dump 的定形对象；避免为单测引入 serde 依赖）。
    fn parse_json_array(payload: &str) -> Vec<Actor> {
        let body = payload
            .strip_prefix('[')
            .and_then(|s| s.strip_suffix(']'))
            .expect("JSON 数组");
        if body.is_empty() {
            return Vec::new();
        }
        let mut out = Vec::new();
        let mut depth = 0_i32;
        let mut start = 0_usize;
        let bytes = body.as_bytes();
        let mut i = 0_usize;
        while i < bytes.len() {
            match bytes[i] {
                b'{' => {
                    if depth == 0 {
                        start = i;
                    }
                    depth += 1;
                }
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        out.push(parse_obj(&body[start..=i]));
                    }
                }
                b'"' => {
                    // 跳过字符串（本 dump 的名字已转义且无裸引号）。
                    i += 1;
                    while i < bytes.len() && bytes[i] != b'"' {
                        if bytes[i] == b'\\' {
                            i += 1;
                        }
                        i += 1;
                    }
                }
                _ => {}
            }
            i += 1;
        }
        out
    }

    fn parse_obj(obj: &str) -> Actor {
        let get_i = |k: &str| -> i32 {
            let pat = format!("\"{k}\":");
            let pos = obj
                .find(&pat)
                .unwrap_or_else(|| panic!("缺字段 {k}: {obj}"));
            let rest = &obj[pos + pat.len()..];
            let end = rest.find([',', '}']).unwrap_or(rest.len());
            rest[..end].trim().parse().unwrap()
        };
        let get_s = |k: &str| -> String {
            let pat = format!("\"{k}\":\"");
            let pos = obj
                .find(&pat)
                .unwrap_or_else(|| panic!("缺字段 {k}: {obj}"));
            let rest = &obj[pos + pat.len()..];
            let end = rest.find('"').unwrap();
            rest[..end].to_string()
        };
        Actor {
            id: get_i("id"),
            name: get_s("name"),
            x: get_i("x"),
            y: get_i("y"),
            ghost: get_i("ghost") as u8,
            death: get_i("death") as u8,
        }
    }

    /// 证据用：把 Rust **真实产出**写到 `M2SVR_STATE_LOG_OUT`（仅在该变量非空时执行），供
    /// `server_state_gate.py` 真跑（证明 Rust 行能被夹具的 fail-closed 门直接消费，而不是手写样例）。
    /// 未设该变量 ⇒ 直接返回（普通门禁无副作用）。
    #[test]
    fn emit_evidence_file_when_requested() {
        let Ok(out) = std::env::var("M2SVR_STATE_LOG_OUT") else {
            return;
        };
        if out.is_empty() {
            return;
        }
        let mut log = StateLog::from_path(Some(&out));
        let (world, _a, _v) = two_player_world();
        let mut displays = DisplayTable::new();
        displays.set(
            1,
            crate::downlink::ActorDisplay::new(2, 0, 0, 0, 0, 255, b"".to_vec()),
        );
        // 5 个玩家攻击单元（与 `--attacks 5` 对齐）。
        for _ in 0..5 {
            log.record(&world, &displays, None);
        }
        println!("M2SVR_STATE_LOG_OUT={out}");
    }

    /// 只为让并行用例拿到互不相撞的临时目录（同 binary 同 pid 也会并行）。
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
}
