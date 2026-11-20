//! 卡 `openmir2-tick-frame-priority-queue` 片①：**改造前/后逐帧字节**快照装置（临时，不入交付）。
//!
//! 同一份装置在改造前树与改造后树各跑一次：`TICKFRAME_SNAPSHOT_OUT=<file>` 时把
//! 四个接线点的下行帧按 `gate/socket/ident/body-hex` 逐条写成 NDJSON；未设环境变量时什么都不做。
//!
//! 只使用改造前就存在的 API（`handle_eat_cmd` / `handle_bag_cmd` / `drain_kill_exp_chain` /
//! `castle::fan_out_sysmessage_to_all` / `castle::refresh_show_name_in_area`），
//! 保证两棵树都能编译。
use super::*;
use std::collections::HashMap;

struct Snap {
    lines: Vec<String>,
}

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

impl Snap {
    fn note(&mut self, scenario: &str, extra: &str) {
        self.lines.push(format!(r#"{{"s":"{scenario}",{extra}}}"#));
    }

    fn add(&mut self, scenario: &str, gate: u8, frames: &[(i32, Vec<u8>)]) {
        for (socket, body) in frames {
            let ident = if body.len() >= 6 {
                u16::from_le_bytes([body[4], body[5]])
            } else {
                0
            };
            self.lines.push(format!(
                r#"{{"s":"{scenario}","g":{gate},"sock":{socket},"id":{ident},"hex":"{}"}}"#,
                hex(body)
            ));
        }
    }
}

fn drain(rx: &mut mpsc::UnboundedReceiver<SimToGate>) -> Vec<(i32, Vec<u8>)> {
    let mut v = Vec::new();
    while let Ok(SimToGate::SendFrame { socket, body }) = rx.try_recv() {
        v.push((socket, body));
    }
    v
}

fn spawn_player(world: &mut mir2_world_bevy::World, name: &str, x: i32, y: i32) -> i32 {
    let mut e = mir2_world_bevy::EntitySpawn::new(0, name.to_string(), x, y);
    e.race = mir2_world_bevy::ACTOR_RACE_PLAY;
    world.add_entity(e)
}

fn ent(name: &str, sid: i32, gate_idx: i32, socket: i32, actor_id: i32) -> OnlineEntry {
    OnlineEntry {
        account: name.to_string(),
        chr_name: name.to_string(),
        session_id: sid,
        gate_idx,
        socket,
        actor_id,
    }
}

#[test]
fn tickframe_slice1_snapshot() {
    let Some(path) = std::env::var_os("TICKFRAME_SNAPSHOT_OUT") else {
        return;
    };
    let mut snap = Snap { lines: Vec::new() };

    // ---- S1 castle::fan_out_sysmessage_to_all（含跨网关线号同 socket 号）----
    {
        let mut world = mir2_world_bevy::World::with_maps(vec![(16, 16)]);
        let mut online: OnlineRegistry = Vec::new();
        for (i, (name, gate, socket)) in [
            ("a", 0i32, 901i32),
            ("b", 0, 902),
            ("c", 1, 901),
            ("d", 1, 903),
        ]
        .into_iter()
        .enumerate()
        {
            let id = spawn_player(&mut world, name, 5, 5);
            online.push(ent(name, i as i32 + 1, gate, socket, id));
        }
        let (tx0, mut rx0) = mpsc::unbounded_channel::<SimToGate>();
        let (tx1, mut rx1) = mpsc::unbounded_channel::<SimToGate>();
        let mut gates: HashMap<u8, mpsc::UnboundedSender<SimToGate>> = HashMap::new();
        gates.insert(0, tx0);
        gates.insert(1, tx1);
        let texts = vec!["甲".to_string(), "乙".to_string(), "丙".to_string()];
        let sent =
            game_svr::castle::fan_out_sysmessage_to_all(&online, &world, &gates, &texts, 255, 56);
        snap.note("castle_fanout", &format!(r#""sent":{sent}"#));
        snap.add("castle_fanout", 0, &drain(&mut rx0));
        snap.add("castle_fanout", 1, &drain(&mut rx1));
    }

    // ---- S2 castle::refresh_show_name_in_area ----
    {
        let mut world = mir2_world_bevy::World::with_maps(vec![(32, 32)]);
        let displays = downlink::DisplayTable::new();
        let sessions: HashMap<String, SessionGround> = HashMap::new();
        let war_states: HashMap<i32, mir2_world_bevy::castle::CastleWarState> = HashMap::new();
        let mut online: OnlineRegistry = Vec::new();
        for (i, (name, x, y)) in [
            ("t1", 5i32, 5i32),
            ("t2", 6, 6),
            ("v1", 5, 6),
            ("v2", 16, 5),
            ("v3", 17, 17),
        ]
        .into_iter()
        .enumerate()
        {
            let id = spawn_player(&mut world, name, x, y);
            online.push(ent(name, i as i32 + 1, 0, 901 + i as i32, id));
        }
        let (tx0, mut rx0) = mpsc::unbounded_channel::<SimToGate>();
        let mut gates: HashMap<u8, mpsc::UnboundedSender<SimToGate>> = HashMap::new();
        gates.insert(0, tx0);
        let sent = game_svr::castle::refresh_show_name_in_area(
            &online,
            &world,
            &displays,
            &gates,
            &sessions,
            &war_states,
            Some(0),
            5,
            5,
            10,
            12,
        );
        snap.note("castle_refshow", &format!(r#""sent":{sent}"#));
        snap.add("castle_refshow", 0, &drain(&mut rx0));
    }

    // ---- S3 T-290 升级链：3 人同 tick 升级 + 子集观察者（X 全见 / Y 只见 B,C）----
    {
        let mut world = mir2_world_bevy::World::with_maps(vec![(64, 64)]);
        let a = spawn_player(&mut world, "a", 10, 10);
        let b = spawn_player(&mut world, "b", 10, 32);
        let c = spawn_player(&mut world, "c", 10, 33);
        let x = spawn_player(&mut world, "x", 10, 22);
        let y = spawn_player(&mut world, "y", 10, 44);
        let mut online: OnlineRegistry = Vec::new();
        for (i, (name, socket, id)) in [
            ("a", 901, a),
            ("b", 902, b),
            ("c", 903, c),
            ("x", 904, x),
            ("y", 905, y),
        ]
        .into_iter()
        .enumerate()
        {
            online.push(ent(name, i as i32 + 1, 0, socket, id));
        }
        let mut sessions: HashMap<String, SessionGround> = HashMap::new();
        for (name, id) in [("a", a), ("b", b), ("c", c)] {
            let mut sg = crate::tests::test_session_ground(id);
            sg.equip.abil.level = 11;
            sg.state.wabil.level = 11;
            sg.state.wabil.hp = 10;
            sg.state.wabil.mp = 5;
            sessions.insert(name.to_string(), sg);
            let e = world.entities.get_mut(id).expect("升级者实体");
            e.progression.level = 11;
            e.progression.max_exp = 1_000;
            e.progression.exp = 1_000;
            e.vitals.hp = 10;
            e.vitals.max_hp = 20;
        }
        let (tx0, mut rx0) = mpsc::unbounded_channel::<SimToGate>();
        let mut gates: HashMap<u8, mpsc::UnboundedSender<SimToGate>> = HashMap::new();
        gates.insert(0, tx0);
        let mut rec = mir2_world_bevy::TickRecord::default();
        rec.exp_awards.push((a, 100, true, 11));
        rec.exp_awards.push((b, 100, true, 11));
        rec.exp_awards.push((c, 100, true, 11));
        let displays = downlink::DisplayTable::new();
        drain_kill_exp_chain(
            &rec,
            &mut world,
            &displays,
            &online,
            &mut sessions,
            &gates,
            &game_svr::ability::StdItemCatalog::new(),
            12,
            true,
        );
        snap.add("t290_chain", 0, &drain(&mut rx0));
    }

    // ---- S4 吃路径（StdMode 1：53 + 657 自通知/观察者）----
    {
        let mut catalog = game_svr::ability::StdItemCatalog::new();
        catalog.insert(
            3000,
            mir2_data::StdItem {
                name: "food_recalc".into(),
                std_mode: 1,
                shape: 0,
                dura_max: 12000,
                ..Default::default()
            },
        );
        let mut world = mir2_world_bevy::World::with_maps(vec![(64, 64)]);
        let actor = spawn_player(&mut world, "hero", 10, 10);
        let mut online: OnlineRegistry = Vec::new();
        online.push(ent("hero", 1, 0, 901, actor));
        for (i, (dx, socket)) in [(5, 902), (11, 903), (12, 904), (13, 905)]
            .into_iter()
            .enumerate()
        {
            let id = spawn_player(&mut world, &format!("o{i}"), 10 + dx, 10);
            online.push(ent(&format!("o{i}"), 2 + i as i32, 0, socket, id));
        }
        let mut sessions: HashMap<String, SessionGround> = HashMap::new();
        let mut sg = crate::tests::test_session_ground(actor);
        sg.state.item_list = vec![game_svr::bag::BagItem {
            make_index: 1,
            index: 3000,
            dura: 0,
            dura_max: 0,
            desc: [0; 14],
        }];
        sessions.insert("hero".to_string(), sg);
        let (tx0, mut rx0) = mpsc::unbounded_channel::<SimToGate>();
        let mut gates: HashMap<u8, mpsc::UnboundedSender<SimToGate>> = HashMap::new();
        gates.insert(0, tx0);
        let colors = MsgColors {
            red_f: 255,
            red_b: 56,
            hear_f: 0,
            hear_b: 255,
            whisper_f: 0,
            whisper_b: 0,
            green_f: 0xDB,
            green_b: 0xFF,
            blue_f: 0xFF,
            blue_b: 0xFC,
        };
        let unbind = game_svr::unbindlist::UnbindList::default();
        let mut item_number = 0;
        let guilds = mir2_world_bevy::GuildManager::new();
        let gmcfg = gmcmd::GmConfig::default();
        let pwd: HashMap<String, game_svr::inbound::PwdSession> = HashMap::new();
        let env = ScriptGmEnv {
            guilds: &guilds,
            maps: None,
            cfg: &gmcfg,
            pwd_sessions: &pwd,
            host: ScriptGmHost::default(),
        };
        let lottery = crate::winlottery::WinLotteryShared::new(
            crate::config::WinLotteryCfg::default().init_state(),
        );
        handle_eat_cmd(
            1,
            0,
            0,
            None,
            "hero",
            901,
            &online,
            &mut sessions,
            &gates,
            &catalog,
            &mut world,
            &colors,
            90,
            &game_svr::strings::StringConf::defaults(),
            None,
            &unbind,
            &mut item_number,
            None,
            &game_svr::npc::NpcVars::default(),
            Some(&env),
            &lottery,
            crate::config::WinLotteryCfg::default(),
            10_000_000,
            crate::config::WeaponMakeLuckCfg::default(),
            &HashMap::new(),
        );
        snap.add("eat_stdmode1", 0, &drain(&mut rx0));
    }

    // ---- S5 换装（TAKEOFFITEM：200 → 615/620 → 752 → 52 → 657）----
    {
        let mut catalog = game_svr::ability::StdItemCatalog::new();
        catalog.insert(
            1,
            mir2_data::StdItem {
                name: "木剑".into(),
                std_mode: 5,
                ..Default::default()
            },
        );
        let mut sessions: HashMap<String, SessionGround> = HashMap::new();
        let mut sg = crate::tests::test_session_ground(42);
        sg.state.gold = 123;
        sg.use_items[0] = Some(game_svr::bag::BagItem {
            make_index: 60001,
            index: 1,
            dura: 10,
            dura_max: 10,
            desc: [0u8; 14],
        });
        sg.sub = game_svr::ability::SubAbility {
            anti_magic: 1,
            hit_point: 8,
            speed_point: 9,
            anti_poison: 0,
            poison_recover: 7,
            health_recover: 0,
            spell_recover: 0,
        };
        sessions.insert("hero".to_string(), sg);
        let mut online: OnlineRegistry = Vec::new();
        online.push(ent("hero", 1, 0, 901, 42));
        let (tx0, mut rx0) = mpsc::unbounded_channel::<SimToGate>();
        let mut gates: HashMap<u8, mpsc::UnboundedSender<SimToGate>> = HashMap::new();
        gates.insert(0, tx0);
        let mut world = mir2_world_bevy::World::new(8, 8);
        let mut displays = downlink::DisplayTable::new();
        handle_bag_cmd(
            game_svr::bag::CM_TAKEOFFITEM,
            60001,
            0,
            0,
            Some("木剑"),
            "hero",
            901,
            &online,
            &mut sessions,
            &gates,
            &catalog,
            &mut world,
            &mut displays,
            12,
            false,
            true,
        );
        snap.add("bag_takeoff", 0, &drain(&mut rx0));
    }

    std::fs::write(&path, snap.lines.join("\n") + "\n").expect("写快照失败");
}
