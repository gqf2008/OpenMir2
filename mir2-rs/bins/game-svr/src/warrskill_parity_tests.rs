//! 卡 `openmir2-warrskill-entry` 要求 5：**`ClientSpellXY` 入口的 9 份 golden 与 Rust 等价路径
//! 逐收件人逐帧一致**（不借 `DoSpell` 账本）。
//!
//! - 3/4/7 的 639 由 `handle_spell_cmd_with_magic` 既有 `default_arm && is_warr_skill` 块产生；
//! - 12/25/26/27/34/38 由 [`crate::warrskill`] 的 6 个 `case` 臂产生。
//!
//! 比对口径：
//! - 每份 `warr-<id>.json` 的每个 case、每个收件人（caster/target/obs_in/obs_out）的 `Sent` 帧序列
//!   **逐条相等**；比较宽度按 `MIR2_CLIENT_ABILITY_LAYOUT`：
//!   - **oracle 布局（`=0`）**：**全帧逐字节**（12B 头含 recog + 尾），53 也比全帧；
//!   - **客户端布局（默认）**：除 53 的 16B `TCharDesc` 尾（本仓多发的已登记 wire 偏差，
//!     责任卡 `openmir2-client-e2e-enter-gaps`）外，其余全帧比；53 头四字段 + 长度 28 断言；
//! - 短包（`SendRaw`）按 golden 的 `(ident=0, tail="+XXX")` 记；**`+GD/…` 不入比对**——它是
//!   `CM_SPELL` 调用方尾部（`PlayObject.Message.cs:1339-1344`），golden 生成器同样排除（卡面判据边界）；
//! - 三键契约（`rng_mode` / `login_order` / `reachability`）缺键即红；
//! - 墙钟字段（`delivery` / `NotDue.now`）不在 `Sent` 里，故天然不入比对（章程 L5）。
use super::*;
use mir2_protocol::frame::CommandMessage;
use serde_json::Value;

type Frames = Vec<(u16, u16, u16, u16, String, Vec<u8>)>;

fn golden(id: u32) -> Value {
    let path = format!(
        "{}/../../tests/parity/golden/magic/warr-{id}.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("读不到 {path}: {e}"));
    serde_json::from_str(&text).expect("golden json")
}

fn spawn(world: &mut mir2_world_bevy::World, name: &str, x: i32, y: i32) -> i32 {
    let mut e = mir2_world_bevy::EntitySpawn::new(0, name.to_string(), x, y);
    e.race = mir2_world_bevy::ACTOR_RACE_PLAY;
    world.add_entity(e)
}

fn who_of(socket: i32) -> &'static str {
    match socket {
        901 => "caster",
        902 => "target",
        903 => "obs_in",
        _ => "obs_out",
    }
}

/// 一个 case 的观测面：帧面 + **行为副作用**（dir / ack / 零抽 / 目标血 / MP）。
/// 拆成结构体是为了让 `warr_43_matches_golden`（帧面）与 `warr_43_behavior_side_effects`
/// （行为面）各自有落点、失败名可定位（§50）。
struct CaseObs {
    caster: i32,
    frames: std::collections::HashMap<String, Frames>,
    mp_before: u16,
    mp_after: u16,
    dir_before: u8,
    dir_after: u8,
    /// `+GD/<tick>` 短包条数（调用方尾部；`result=true` 才发，`delay_time>0` 不走该支）。
    ack_count: usize,
    target_hp_before: u16,
    target_hp_after: u16,
    /// `RandomNumber` 的 `Debug` 状态快照（`seed_array/inext/inextp`）。
    /// **失效条件**：若将来给 `RandomNumber` 写自定义 `Debug` 且省略这些字段，零抽断言会**变盲**
    /// ⇒ 届时改用显式 `draws()` 计数。
    rng_before: String,
    rng_after: String,
}

/// 跑一个 case：按 golden 的 `Fixture` 建世界/会话/记录，调宿主 `CM_SPELL` 入口，收集每个
/// 收件人的**线上帧**（`+GD/…` 不进行比对面、只计数）与行为侧观测。
fn run_case(magic_id: u16, fx: &Value) -> CaseObs {
    let mp = u16::try_from(fx["caster_mp"].as_u64().expect("caster_mp")).expect("mp u16");
    let level =
        u8::try_from(fx["user_magic_level"].as_u64().expect("user_magic_level")).expect("level u8");
    let tx = i32::try_from(fx["entry_args"]["n_target_x"].as_i64().expect("n_target_x"))
        .expect("tx i32");
    let ty = i32::try_from(fx["entry_args"]["n_target_y"].as_i64().expect("n_target_y"))
        .expect("ty i32");
    let preset = &fx["preset"];

    // 地图放大（DEV-MG-07）：真支自移 caster (100,100)->(102,100) 需要界内坐标；
    // 既有门/退化用例不移动 ⇒ 不受影响。
    let mut world = mir2_world_bevy::World::with_maps(vec![(256, 256)]);
    let caster = spawn(&mut world, "caster", 100, 100);
    let target = spawn(&mut world, "target", 101, 100);
    let obs_in = spawn(&mut world, "obs_in", 105, 100);
    let obs_out = spawn(&mut world, "obs_out", 140, 100);
    // oracle 装置 `env.AddMapObject(...)` 同构：同步 rig 需显式落格（`Command::Enter` 到 tick 才落格）。
    let bind_now = now_ms();
    for (bid, bx, by) in [
        (caster, 100, 100),
        (target, 101, 100),
        (obs_in, 105, 100),
        (obs_out, 140, 100),
    ] {
        if let Some(map) = world.maps.get_mut(0) {
            map.add_map_object(bx, by, mir2_world_bevy::CellType::Play, bid, bind_now);
        }
    }

    let mut online: OnlineRegistry = OnlineRegistry::new();
    for (i, (name, id)) in [
        ("caster", caster),
        ("target", target),
        ("obs_in", obs_in),
        ("obs_out", obs_out),
    ]
    .into_iter()
    .enumerate()
    {
        online.push(OnlineEntry {
            account: name.to_string(),
            chr_name: name.to_string(),
            session_id: i as i32 + 1,
            gate_idx: 0,
            socket: 901 + i as i32,
            actor_id: id,
        });
    }
    // 展示表：golden 的 caster 登录后 `Feature/CharStatus/Light` 全 0、Dir=4
    // （27 的 `SM_RUSHKUNG` 尾 = `Encode(CharDesc(feature,status))`，8 个零字节 ⇒ `ddhkddhkddK`）。
    let mut displays = downlink::DisplayTable::new();
    displays.set(
        caster,
        downlink::ActorDisplay::new(4, 0, 0, 0, 0, 255, b"caster".to_vec()),
    );

    let mut sessions: std::collections::HashMap<String, SessionGround> =
        std::collections::HashMap::new();
    let mut sg = crate::tests::test_session_ground(caster);
    // golden 的 caster 走登录序 `RecalcLevelAbilitys/RecalcAbilitys`：L30 满血 MaxHP=419（53 的
    // Param/Series 实测值），MP = case 的 `caster_mp`。这里直接给**同一输入态**（不重算职业公式）。
    sg.state.wabil.hp = 419;
    sg.state.wabil.max_hp = 419;
    sg.state.wabil.mp = mp;
    sg.show_hp = false;
    sg.magic.use_thrusting = preset["use_thrusting"].as_bool().unwrap_or(false);
    sg.magic.use_half_moon = preset["use_half_moon"].as_bool().unwrap_or(false);
    sg.magic.crs_hitkill = preset["crs_hitkill"].as_bool().unwrap_or(false);
    sg.magic.mbo43_kill = preset["mbo43_kill"].as_bool().unwrap_or(false);
    if preset["fire_hit_busy"].as_bool().unwrap_or(false) {
        // 26 的 `AllowFireHitSkill`：`now - LatestFireHitTick > 10000` 才放行；
        // golden 的 busy case 预设 `LatestFireHitTick = now`（10s 门内）。
        sg.magic.latest_fire_hit_tick = now_ms();
    }
    sessions.insert("caster".to_string(), sg);

    let mut rec = record::PlayerRecord::default();
    rec.data.magic = Some(vec![Some(record::MagicRcd {
        mag_idx: magic_id,
        level,
        magic_key: 0,
        tran_point: 0,
    })]);
    let mut saves: std::collections::HashMap<String, SavedHuman> = std::collections::HashMap::new();
    saves.insert(
        "caster".to_string(),
        SavedHuman {
            account: "uc".to_string(),
            record: rec,
            enter_map: "0".to_string(),
            derived: None,
        },
    );

    let (tx0, mut rx0) = mpsc::unbounded_channel::<SimToGate>();
    let mut gates: std::collections::HashMap<u8, mpsc::UnboundedSender<SimToGate>> =
        std::collections::HashMap::new();
    gates.insert(0, tx0);
    // 魔表：`Spell=20 / DefSpell=0 / Level=1` ⇒ `GetSpellPoint = 10`（golden `fixture_spec` 同值）。
    let magic_table = magics::MagicTable::new(vec![mir2_data::MagicInfo {
        magic_id,
        spell: 20,
        def_spell: 0,
        ..Default::default()
    }]);
    let catalog = game_svr::ability::StdItemCatalog::new();
    let pet_cfg = mir2_data::pet_slave::PetSlaveConfig::default();
    let table = game_svr::mongen::MonsterTable::new(Vec::new());
    // oracle 登录后 `Dir` = golden `Fixture.dir`；先置成同一输入态，调用后要求**不变**。
    let dir_start = u8::try_from(fx["dir"].as_u64().expect("dir")).expect("dir u8");
    if let Some(e) = world.entities.get_mut(caster) {
        e.ai.dir = dir_start;
    }
    let dir_before = world.entities.get(caster).map_or(0, |e| e.ai.dir);
    let target_hp_before = world.entities.get(target).map_or(0, |e| e.hp);
    // MP 直接断言：调用**前**真实会话里的 `state.wabil.mp`（不是 fixture 常量）。
    let mp_before = sessions
        .get("caster")
        .map(|s| s.state.wabil.mp)
        .unwrap_or_default();
    // 零抽断言的状态快照（见 `CaseObs.rng_before` 的失效条件）。
    let rng_before = format!("{:?}", world.rng);
    // `CM_SPELL` 入参：`tag = magic_id`、`recog = make_long(nTargetX, nTargetY)`（`magicwire::parse_spell`）。
    // `nParam3 = MakeLong(Param, Series)` 才是**目标 ActorId**（`WorldServer.cs:783-790`）：
    // 之前传 0 会让 `req.target_actor_id = 0`（目标面不可观测）；这里按真实映射传 target，
    // 使"不伤目标"类断言（43）有可咬的产品侧写点。
    handle_spell_cmd_with_magic(
        game_svr::feature::make_long(tx as u16, ty as u16),
        magic_id,
        (target & 0xFFFF) as u16,
        ((target >> 16) & 0xFFFF) as u16,
        "caster",
        901,
        &online,
        &mut world,
        &mut saves,
        &mut sessions,
        &catalog,
        &magic_table,
        &pet_cfg,
        8,
        None,
        &table,
        &mut displays,
        &gates,
        1,
        1,
        2,
        None,
        &MsgColors::default(),
        &game_svr::strings::StringConf::defaults(),
    );

    // **行为面观测**：MP 投影（客户端 MP 源）、Dir、目标血、RNG 状态。
    let mp_after = sessions
        .get("caster")
        .map(|s| s.state.wabil.mp)
        .unwrap_or_default();
    let dir_after = world.entities.get(caster).map_or(0, |e| e.ai.dir);
    let target_hp_after = world.entities.get(target).map_or(0, |e| e.hp);
    let rng_after = format!("{:?}", world.rng);
    let mut ack_count = 0usize;
    let mut out: std::collections::HashMap<String, Frames> = std::collections::HashMap::new();
    while let Ok(msg) = rx0.try_recv() {
        match msg {
            SimToGate::SendFrame { socket, body } => {
                let cm = CommandMessage::from_bytes(&body).expect("body >= 12B");
                let tail = String::from_utf8_lossy(&body[12..]).to_string();
                out.entry(who_of(socket).to_string()).or_default().push((
                    cm.ident,
                    cm.param,
                    cm.tag,
                    cm.series,
                    tail,
                    body.clone(),
                ));
            }
            SimToGate::SendRaw { socket, payload } => {
                // 调用方尾部 `+GD/<uptime>` 不在 golden（墙钟 + 不属于 ClientSpellXY），
                // 但**计数**用于 `result=true` 的行为断言。
                if payload.starts_with(b"+GD/") {
                    ack_count += 1;
                    continue;
                }
                out.entry(who_of(socket).to_string()).or_default().push((
                    0,
                    0,
                    0,
                    0,
                    String::from_utf8_lossy(&payload).to_string(),
                    payload.clone(),
                ));
            }
            _ => {}
        }
    }
    CaseObs {
        caster,
        frames: out,
        mp_before,
        mp_after,
        dir_before,
        dir_after,
        ack_count,
        target_hp_before,
        target_hp_after,
        rng_before,
        rng_after,
    }
}

fn golden_frames(r: &Value) -> Vec<(u16, u16, u16, u16, String)> {
    r["Sent"]
        .as_array()
        .expect("Receivers[].Sent")
        .iter()
        .map(|f| {
            (
                u16::try_from(f["ident"].as_u64().expect("ident")).expect("u16"),
                u16::try_from(f["param"].as_u64().expect("param")).expect("u16"),
                u16::try_from(f["tag"].as_u64().expect("tag")).expect("u16"),
                u16::try_from(f["series"].as_u64().expect("series")).expect("u16"),
                f["tail"].as_str().expect("tail").to_string(),
            )
        })
        .collect()
}

/// 单份 golden 的全部 case 逐收件人比对。每份一个 `#[test]`，便于变异实验定位
/// 「哪一支红了、别的支没被连带」（§50）。
fn check_golden(id: u32) {
    let doc = golden(id);
    for case in doc["cases"].as_array().expect("cases") {
        let label = case["Label"].as_str().unwrap_or("?");
        let fx = &case["Fixture"];
        // 三键契约（`opc-07` 2026-11-20）：缺键即红。
        assert_eq!(
            fx["rng_mode"], "cover_drain",
            "warr-{id}/{label}: 缺 rng_mode"
        );
        assert_eq!(fx["login_order"], true, "warr-{id}/{label}: 缺 login_order");
        assert_eq!(
            fx["reachability"], "reachable",
            "warr-{id}/{label}: 缺 reachability"
        );
        let obs = run_case(id as u16, fx);
        // **零抽两侧断言**（判据 4）：
        // ① golden 的 `Rng` 必须为空（oracle 记录零抽）；
        // ② Rust 的 RNG 状态快照前后必须不变（`CaseObs.rng_before/after` 的 `Debug` 快照）。
        //    **失效条件**：若将来自定义 `Debug` 省略 `seed_array/inext/inextp`，本断言会**变盲**
        //    ⇒ 届时改用 `draws()` 计数（与 43 的行为用例同口径）。
        assert_eq!(
            case["Rng"].as_array().expect("Rng").len(),
            0,
            "warr-{id}/{label}: golden 的 Rng 必须为空（oracle 零抽）"
        );
        assert_eq!(
            obs.rng_after, obs.rng_before,
            "warr-{id}/{label}: Rust 零抽（RNG 状态不得变化）"
        );
        let (caster, got) = (obs.caster, obs.frames);
        for r in case["Receivers"].as_array().expect("Receivers") {
            let who = r["Who"].as_str().expect("Who");
            let want = golden_frames(r);
            let have = got.get(who).cloned().unwrap_or_default();
            assert_eq!(
                have.len(),
                want.len(),
                "warr-{id}/{label} 收件人 {who} 的帧数不一致（got {have:?} / want {want:?}）"
            );
            // 比较宽度按布局决定（`MIR2_CLIENT_ABILITY_LAYOUT`）：
            // - oracle 布局（env=0）⇒ **全帧**（头 12B 含 recog + 尾）逐字节比，**包括 53**；
            // - 客户端布局（默认）⇒ 53 的 16B `TCharDesc` 尾是本仓多发的**已登记 wire 偏差**
            //   （责任卡 `openmir2-client-e2e-enter-gaps`），该 16B 不在比较面内；其余帧仍全帧比。
            let oracle_layout = !game_svr::smframe::client_layout_enabled();
            for (i, (h, w)) in have.iter().zip(want.iter()).enumerate() {
                assert_eq!(
                    (h.0, h.1, h.2, h.3),
                    (w.0, w.1, w.2, w.3),
                    "warr-{id}/{label} 收件人 {who} 第 {i} 帧头四字段不一致（got {have:?} / want {want:?}）"
                );
                if h.0 == 0 {
                    // 短包（`SendSocket(string)`）：整段 payload 即全帧。
                    assert_eq!(
                        h.5,
                        w.4.as_bytes(),
                        "warr-{id}/{label} 收件人 {who} 第 {i} 帧短包 payload 不一致"
                    );
                    continue;
                }
                if h.0 == 53 && !oracle_layout {
                    // 客户端布局的 53：头已比；尾部 16B 为已登记偏差，但长度必须恰为 12+16。
                    assert_eq!(
                        h.5.len(),
                        12 + 16,
                        "warr-{id}/{label} 收件人 {who} 第 {i} 帧（53，客户端布局）body 必须 12+16"
                    );
                    continue;
                }
                // **全帧**：头（`Recog` = 施法者，golden 不记 recog）+ golden 的尾。
                let mut expected =
                    game_svr::smframe::command_header(caster, h.0, h.1, h.2, h.3).to_vec();
                expected.extend_from_slice(w.4.as_bytes());
                assert_eq!(
                    h.5,
                    expected,
                    "warr-{id}/{label} 收件人 {who} 第 {i} 帧全帧不一致（got {have:?} / want {want:?}）"
                );
            }
        }
    }
}

/// 43 `SKILL_43`：`Skill43OnOff` 开/关 + `"+CID"/"+UCID"`。
///
/// 文案是**源码默认**（`MessageSettings.RedHalfMoonOn/Off`；`StringConf` 不读、部署无该键）
/// ⇒ golden 的 `fixture_spec` 记了生成时的两条文案，比较器除逐帧外还断言它与字面量一致，
/// 防默认串被改而 golden 静默漂移。
#[test]
fn warr_43_matches_golden() {
    let doc = golden(43);
    assert_eq!(
        doc["fixture_spec"]["toggle_text_on"], "开启破空剑",
        "43 开启文案（源码默认，StringConf 不读）"
    );
    assert_eq!(
        doc["fixture_spec"]["toggle_text_off"], "关闭破空剑",
        "43 关闭文案（源码默认，StringConf 不读）"
    );
    check_golden(43);
}

/// 43 的**行为面**（失败名可定位，§50）：Dir 不变、`result=true` 的 `+GD` ack、零抽、
/// 目标血不变、MP 不扣。帧面见 [`warr_43_matches_golden`]。
#[test]
fn warr_43_behavior_side_effects() {
    let doc = golden(43);
    for case in doc["cases"].as_array().expect("cases") {
        let fx = &case["Fixture"];
        let label = case["Label"].as_str().unwrap_or("?");
        // oracle 侧记录先核：golden 的 `Rng` 必须为空（只断"Rust 没抽"不看"oracle 记零抽"＝只看一半）。
        assert_eq!(
            case["Rng"].as_array().expect("Rng").len(),
            0,
            "warr-43/{label}: golden 的 Rng 必须为空（oracle 零抽）"
        );
        let want_dir = u8::try_from(fx["dir"].as_u64().expect("dir")).expect("dir u8");
        let want_hp = u16::try_from(case["TargetHp"].as_u64().expect("TargetHp")).expect("hp u16");
        let before_hp = u16::try_from(case["TargetHpBefore"].as_u64().expect("TargetHpBefore"))
            .expect("hp u16");
        let obs = run_case(43, fx);
        // ① Dir：显式 case 不执行 default 的 `Dir = GetNextDirection` ⇒ 前后不变且 == golden。
        assert_eq!(
            obs.dir_after, obs.dir_before,
            "warr-43/{label}: Dir 不得变化（显式 case 不走 default 的 GetNextDirection）"
        );
        assert_eq!(
            obs.dir_after, want_dir,
            "warr-43/{label}: Dir 必须 == golden"
        );
        // ② `result=true` 的可观测代理：恰一条 `+GD/<tick>`（`delay_time>0` 不走该支）。
        assert_eq!(
            obs.ack_count, 1,
            "warr-43/{label}: 必须恰一条 +GD ack（result=true / delay_time==0）"
        );
        // ③ 零抽：RNG 状态快照前后相等（依赖 `Debug` 含 `seed_array/inext/inextp`；
        //    若将来自定义 Debug 省略这些字段，本断言会变盲 ⇒ 改用 draws() 计数）。
        assert_eq!(
            obs.rng_after, obs.rng_before,
            "warr-43/{label}: 43 零抽（RNG 状态不得变化）"
        );
        // ④ 目标血：前后不变且等于 golden 的 TargetHpBefore/TargetHp。
        assert_eq!(
            obs.target_hp_before, before_hp,
            "warr-43/{label}: 起始目标血必须 == golden TargetHpBefore"
        );
        assert_eq!(
            obs.target_hp_after, obs.target_hp_before,
            "warr-43/{label}: 43 不得伤目标"
        );
        assert_eq!(
            obs.target_hp_after, want_hp,
            "warr-43/{label}: 目标血必须 == golden TargetHp"
        );
        // ⑤ MP：前后相等且起值 == fixture（从帧测试移来：行为面归行为测试）。
        assert_eq!(obs.mp_after, obs.mp_before, "warr-43/{label}: MP 不得变化");
        assert_eq!(
            obs.mp_before,
            u16::try_from(fx["caster_mp"].as_u64().expect("caster_mp")).expect("u16"),
            "warr-43/{label}: 起始 MP 必须等于 fixture"
        );
    }
}

#[test]
fn warr_3_matches_golden() {
    check_golden(3);
}

#[test]
fn warr_4_matches_golden() {
    check_golden(4);
}

#[test]
fn warr_7_matches_golden() {
    check_golden(7);
}

#[test]
fn warr_12_matches_golden() {
    check_golden(12);
}

#[test]
fn warr_25_matches_golden() {
    check_golden(25);
}

#[test]
fn warr_26_matches_golden() {
    check_golden(26);
}

#[test]
fn warr_27_matches_golden() {
    check_golden(27);
}

#[test]
fn warr_34_matches_golden() {
    check_golden(34);
}

#[test]
fn warr_38_matches_golden() {
    check_golden(38);
}

// ============ MG-07（2026-11-25）：27 的 3s 门（已实现未断言） + 真支登记偏差 ============

/// MG-07 夹具：与 `warr-27.json` 的 `coordinate_dir` 同一输入态
/// （caster (100,100)/dir=4/MP=100/L1，目标 (101,100)，魔表 `Spell=20` ⇒ 花费 10）。
#[allow(clippy::type_complexity)]
fn mg07_fixture() -> (
    mir2_world_bevy::World,
    OnlineRegistry,
    std::collections::HashMap<String, SavedHuman>,
    std::collections::HashMap<String, SessionGround>,
    game_svr::ability::StdItemCatalog,
    magics::MagicTable,
    mir2_data::pet_slave::PetSlaveConfig,
    game_svr::mongen::MonsterTable,
    downlink::DisplayTable,
    std::collections::HashMap<u8, mpsc::UnboundedSender<SimToGate>>,
    mpsc::UnboundedReceiver<SimToGate>,
    i32,
    i32,
) {
    // 地图放大（DEV-MG-07）：真支自移 caster (100,100)->(102,100) 需要界内坐标；
    // 既有门/退化用例不移动 ⇒ 不受影响。
    let mut world = mir2_world_bevy::World::with_maps(vec![(256, 256)]);
    let caster = spawn(&mut world, "caster", 100, 100);
    let target = spawn(&mut world, "target", 101, 100);

    let mut online: OnlineRegistry = OnlineRegistry::new();
    for (i, (name, id)) in [("caster", caster), ("target", target)]
        .into_iter()
        .enumerate()
    {
        online.push(OnlineEntry {
            account: name.to_string(),
            chr_name: name.to_string(),
            session_id: i as i32 + 1,
            gate_idx: 0,
            socket: 901 + i as i32,
            actor_id: id,
        });
    }
    let mut displays = downlink::DisplayTable::new();
    displays.set(
        caster,
        downlink::ActorDisplay::new(4, 0, 0, 0, 0, 255, b"caster".to_vec()),
    );

    let mut sessions: std::collections::HashMap<String, SessionGround> =
        std::collections::HashMap::new();
    let mut sg = crate::tests::test_session_ground(caster);
    sg.state.wabil.hp = 419;
    sg.state.wabil.max_hp = 419;
    sg.state.wabil.mp = 100;
    sg.show_hp = false;
    sessions.insert("caster".to_string(), sg);

    let mut rec = record::PlayerRecord::default();
    rec.data.magic = Some(vec![Some(record::MagicRcd {
        mag_idx: 27,
        level: 1,
        magic_key: 0,
        tran_point: 0,
    })]);
    let mut saves: std::collections::HashMap<String, SavedHuman> = std::collections::HashMap::new();
    saves.insert(
        "caster".to_string(),
        SavedHuman {
            account: "uc".to_string(),
            record: rec,
            enter_map: "0".to_string(),
            derived: None,
        },
    );

    let (tx0, rx0) = mpsc::unbounded_channel::<SimToGate>();
    let mut gates: std::collections::HashMap<u8, mpsc::UnboundedSender<SimToGate>> =
        std::collections::HashMap::new();
    gates.insert(0, tx0);
    let magic_table = magics::MagicTable::new(vec![mir2_data::MagicInfo {
        magic_id: 27,
        spell: 20,
        def_spell: 0,
        ..Default::default()
    }]);
    (
        world,
        online,
        saves,
        sessions,
        game_svr::ability::StdItemCatalog::new(),
        magic_table,
        mir2_data::pet_slave::PetSlaveConfig::default(),
        game_svr::mongen::MonsterTable::new(Vec::new()),
        displays,
        gates,
        rx0,
        caster,
        target,
    )
}

/// 一次 `CM_SPELL 27` 的观测：施法者（socket=901）的 SM 帧 + `+GD` ack 计数。
struct Mg07Obs {
    frames: Frames,
    acks: usize,
}

#[allow(clippy::too_many_arguments)]
fn mg07_cast(
    world: &mut mir2_world_bevy::World,
    online: &OnlineRegistry,
    saves: &mut std::collections::HashMap<String, SavedHuman>,
    sessions: &mut std::collections::HashMap<String, SessionGround>,
    catalog: &game_svr::ability::StdItemCatalog,
    magic_table: &magics::MagicTable,
    pet_cfg: &mir2_data::pet_slave::PetSlaveConfig,
    table: &game_svr::mongen::MonsterTable,
    displays: &mut downlink::DisplayTable,
    gates: &std::collections::HashMap<u8, mpsc::UnboundedSender<SimToGate>>,
    rx: &mut mpsc::UnboundedReceiver<SimToGate>,
    target: i32,
    spell_xy: (i32, i32),
) -> Mg07Obs {
    // oracle 装置在夹具建立时逐实体 `env.AddMapObject(...)`；本 rig 同步调 spell 前手动落格
    //（`add_entity` 的 `Command::Enter` 要到 tick 才落格 ⇒ 同步 spell rig 必须显式落格，
    //  否则 `moving_object_at`（GetMovingObject 移植）看不到施法者/目标 ⇒ 真支/退化支都走错）。
    let bind_now = now_ms();
    let caster_id = sessions.get("caster").map_or(0, |s| s.actor_id);
    for id in [caster_id, target] {
        let (bx, by) = world.entities.get(id).map_or((0, 0), |e| (e.x, e.y));
        if let Some(map) = world.maps.get_mut(0) {
            map.add_map_object(bx, by, mir2_world_bevy::CellType::Play, id, bind_now);
        }
    }
    handle_spell_cmd_with_magic(
        game_svr::feature::make_long(spell_xy.0 as u16, spell_xy.1 as u16),
        27,
        (target & 0xFFFF) as u16,
        ((target >> 16) & 0xFFFF) as u16,
        "caster",
        901,
        online,
        world,
        saves,
        sessions,
        catalog,
        magic_table,
        pet_cfg,
        8,
        None,
        table,
        displays,
        gates,
        1,
        1,
        2,
        None,
        &MsgColors::default(),
        &game_svr::strings::StringConf::defaults(),
    );
    let mut obs = Mg07Obs {
        frames: Vec::new(),
        acks: 0,
    };
    while let Ok(msg) = rx.try_recv() {
        match msg {
            SimToGate::SendFrame { socket: 901, body } => {
                let cm = CommandMessage::from_bytes(&body).expect("body >= 12B");
                let tail = String::from_utf8_lossy(&body[12..]).to_string();
                obs.frames
                    .push((cm.ident, cm.param, cm.tag, cm.series, tail, body));
            }
            SimToGate::SendRaw { payload, .. } if payload.starts_with(b"+GD/") => obs.acks += 1,
            _ => {}
        }
    }
    obs
}

/// **MG-07 补验：27 的 3s 门**（oracle `PlayObject.Attack.cs:1189`
/// `GetTickCount() - DoMotaeboTick > 3 * 1000`；Rust `warrskill.rs` `Arm::Moteebo`
/// 的 `now - do_motaebo_tick <= 3 * 1000 ⇒ return true`）。
///
/// 三段连续调用（同一会话/同一世界）：
/// A. 门内（`do_motaebo_tick = now-500`）⇒ **零 SM 帧**、MP 不变、戳不变，但 `+GD` ack 恰 1
///    （oracle `result = true` 在门**外**就置位，gate 命中只跳过效果体）；
/// B. 门外（回拨 `do_motaebo_tick = now-5000`）⇒ 退化支全帧：`SM_RUSHKUNG(7)` + `53` + 红字 `100`
///    （100 = `SM_SYSMESSAGE`），MP 100→90，戳刷新；目标血不动（退化支不伤人）；
/// C. 紧接再调 ⇒ 又进门：零帧、MP 仍 90、戳不变。
///
/// 变异：删 `warrskill.rs` 的 3s 早退 ⇒ A/C 出帧且 MP 掉，本用例红。
#[test]
fn warr_27_motaebo_three_second_gate() {
    let (
        mut world,
        online,
        mut saves,
        mut sessions,
        catalog,
        magic_table,
        pet_cfg,
        table,
        mut displays,
        gates,
        mut rx,
        _caster,
        target,
    ) = mg07_fixture();
    let mut cast = |world: &mut mir2_world_bevy::World,
                    saves: &mut std::collections::HashMap<String, SavedHuman>,
                    sessions: &mut std::collections::HashMap<String, SessionGround>,
                    displays: &mut downlink::DisplayTable| {
        mg07_cast(
            world,
            &online,
            saves,
            sessions,
            &catalog,
            &magic_table,
            &pet_cfg,
            &table,
            displays,
            &gates,
            &mut rx,
            target,
            (101, 100),
        )
    };

    // A：门内。
    let stale_tick = now_ms() - 500;
    sessions.get_mut("caster").unwrap().magic.do_motaebo_tick = stale_tick;
    let a = cast(&mut world, &mut saves, &mut sessions, &mut displays);
    assert!(
        a.frames.is_empty(),
        "3s 门内必须零 SM 帧（oracle 只 return true）：{:?}",
        a.frames.iter().map(|f| f.0).collect::<Vec<_>>()
    );
    assert_eq!(a.acks, 1, "门内 result=true ⇒ 调用方 +GD ack 恰 1");
    assert_eq!(
        sessions["caster"].state.wabil.mp, 100,
        "门内不得扣 MP（DamageSpell 在门后）"
    );
    assert_eq!(
        sessions["caster"].magic.do_motaebo_tick, stale_tick,
        "门内不得刷新 DoMotaeboTick（oracle 在门后写戳）"
    );

    // B：门外（回拨 5s）。
    let tick_before_b = now_ms() - 5_000;
    sessions.get_mut("caster").unwrap().magic.do_motaebo_tick = tick_before_b;
    let target_hp_before = world.entities.get(target).map_or(0, |e| e.hp);
    let b = cast(&mut world, &mut saves, &mut sessions, &mut displays);
    let ids: Vec<u16> = b.frames.iter().map(|f| f.0).collect();
    for want in [
        mir2_protocol::messages::SM_RUSHKUNG,
        mir2_protocol::messages::SM_HEALTHSPELLCHANGED,
        mir2_protocol::messages::SM_SYSMESSAGE,
    ] {
        assert!(ids.contains(&want), "门外退化支必发 {want}：{ids:?}");
    }
    assert_eq!(b.acks, 1, "门外同样 result=true ⇒ +GD 恰 1");
    assert_eq!(
        sessions["caster"].state.wabil.mp, 90,
        "门外扣 GetSpellPoint=10（100 → 90）"
    );
    assert!(
        sessions["caster"].magic.do_motaebo_tick >= tick_before_b + 3_000,
        "门外必须刷新 DoMotaeboTick（{} → {}）",
        tick_before_b,
        sessions["caster"].magic.do_motaebo_tick
    );
    assert_eq!(
        world.entities.get(target).map_or(0, |e| e.hp),
        target_hp_before,
        "退化支（无正前目标）不得伤目标"
    );

    // C：紧接再调 ⇒ 又进门。
    let fresh_tick = sessions["caster"].magic.do_motaebo_tick;
    let c = cast(&mut world, &mut saves, &mut sessions, &mut displays);
    assert!(
        c.frames.is_empty(),
        "3s 内紧接再调必须零帧：{:?}",
        c.frames.iter().map(|f| f.0).collect::<Vec<_>>()
    );
    assert_eq!(c.acks, 1, "门内 +GD 恰 1");
    assert_eq!(sessions["caster"].state.wabil.mp, 90, "门内不得二次扣 MP");
    assert_eq!(
        sessions["caster"].magic.do_motaebo_tick, fresh_tick,
        "门内不得刷新戳"
    );
}

/// **DEV-MG-07（真支已实装，原登记偏差钉改正向）**：正前格另有**更低等级**对象 ⇒
/// oracle 真支（`PlayObject.Base.cs:1390-1502` Path A）：推挤 `SM_BACKSTEP(9)`×2、
/// 自移 `SM_RUSH(6)`×2、目标伤害 `SM_STRUCK(31)`、`bo35=false`（无 7/无红字 100）、
/// 施法者进占其旧格 (102,100)、怪被推到 (103,100) 且掉血、MP 100→90。
/// 训练尾 640（延迟 1000ms）与逐字节帧面在 golden 驱动面（`warr-27-true.json` 批④）。
#[test]
fn warr_27_true_branch_pushes_struck_trains() {
    let (
        mut world,
        online,
        mut saves,
        mut sessions,
        catalog,
        magic_table,
        pet_cfg,
        table,
        mut displays,
        gates,
        mut rx,
        caster,
        target,
    ) = mg07_fixture();
    // 玩家 target 挪出推挤走廊（golden 同款 (110,100)），正前格放低等级怪。
    {
        let t = world.entities.transform_mut(target).expect("target");
        t.x = 110;
        t.y = 100;
    }
    let mut blocker_e = mir2_world_bevy::EntitySpawn::new(0, "blocker".to_string(), 101, 100);
    blocker_e.race = mir2_world_bevy::ACTOR_RACE_ANIMAL;
    blocker_e.level = 10;
    blocker_e.hp = 100;
    blocker_e.max_hp = 100;
    let blocker = world.add_entity(blocker_e);
    if let Some(map) = world.maps.get_mut(0) {
        map.add_map_object(
            101,
            100,
            mir2_world_bevy::CellType::Monster,
            blocker,
            now_ms(),
        );
    }
    // 施法者 L30（CanMotaebo 等级门 `我等级 > 目标等级`）；怪 L10。
    sessions.get_mut("caster").unwrap().state.wabil.level = 30;
    // 31/9 帧尾需要受击者/被推者 CharDesc（display）。
    displays.set(
        blocker,
        downlink::ActorDisplay::new(2, 0, 0, 0, 0, 255, b"blocker".to_vec()),
    );
    let blocker_hp_before = world.entities.get(blocker).map_or(0, |e| e.hp);
    sessions.get_mut("caster").unwrap().magic.do_motaebo_tick = now_ms() - 5_000;
    let obs = mg07_cast(
        &mut world,
        &online,
        &mut saves,
        &mut sessions,
        &catalog,
        &magic_table,
        &pet_cfg,
        &table,
        &mut displays,
        &gates,
        &mut rx,
        target,
        (2, 100),
    );
    let ids: Vec<u16> = obs.frames.iter().map(|f| f.0).collect();
    // 全序列（含 per-收件人四叉堆序）：53 直发最前（优先级 68 最低），
    // buf 帧堆序 = 31(82) < 9(156) < 6(168)。
    assert_eq!(
        ids,
        vec![
            mir2_protocol::messages::SM_HEALTHSPELLCHANGED,
            mir2_protocol::messages::SM_STRUCK,
            mir2_protocol::messages::SM_BACKSTEP,
            mir2_protocol::messages::SM_BACKSTEP,
            mir2_protocol::messages::SM_RUSH,
            mir2_protocol::messages::SM_RUSH,
        ],
        "真支帧面（53→31→9×2→6×2）：{ids:?}"
    );
    let (x, y) = world.entities.get(caster).map_or((0, 0), |e| (e.x, e.y));
    assert_eq!((x, y), (102, 100), "真支 ⇒ 施法者进占两格");
    let (bx, by) = world.entities.get(blocker).map_or((0, 0), |e| (e.x, e.y));
    assert_eq!((bx, by), (103, 100), "怪被推两格");
    let blocker_hp_after = world.entities.get(blocker).map_or(0, |e| e.hp);
    assert!(
        blocker_hp_after < blocker_hp_before,
        "目标伤害段必须掉血（{blocker_hp_before} → {blocker_hp_after}）"
    );
    assert_eq!(obs.acks, 1, "result=true ⇒ +GD 恰 1");
    assert_eq!(
        sessions["caster"].state.wabil.mp, 90,
        "门外扣 GetSpellPoint=10"
    );
    // `Dir = (byte)nTargetX = 2`
    assert_eq!(
        world.entities.get(caster).map_or(0, |e| e.ai.dir),
        2,
        "DoMotaebo 入口写施法者 Dir"
    );
}

/// **BUG-REPLICA 显式钉（三重钉之二）**：`n24` 是 **byte**（`Base.cs:1398`）——L0 两轮推挤后
/// `1 → 0 → 255` 回绕 ⇒ 尾段伤害 `Random(2560) + 2560` ∈ **[2560, 5119]**（oracle 实测 3755）。
/// 实现若"顺手"改 i32（负值钳 0）⇒ 伤害 ∈ [10, 19] ⇒ 本测红。
/// **副本随原件；修复需走 oracle 变更流程**（三重钉之三 = manifest BUG-REPLICA 登记）。
#[test]
fn warr_27_l0_byte_wrap_damage_bound_is_bug_replica() {
    let (
        mut world,
        online,
        mut saves,
        mut sessions,
        catalog,
        magic_table,
        pet_cfg,
        table,
        mut displays,
        gates,
        mut rx,
        caster,
        target,
    ) = mg07_fixture();
    // UserMagicLevel=0：SpellPoint = Round(20/4*(0+1)) = 5 ⇒ MP 100→95。
    saves.get_mut("caster").unwrap().record.data.magic = Some(vec![Some(record::MagicRcd {
        mag_idx: 27,
        level: 0,
        magic_key: 0,
        tran_point: 0,
    })]);
    {
        let t = world.entities.transform_mut(target).expect("target");
        t.x = 110;
        t.y = 100;
    }
    let mut blocker_e = mir2_world_bevy::EntitySpawn::new(0, "blocker".to_string(), 101, 100);
    blocker_e.race = mir2_world_bevy::ACTOR_RACE_ANIMAL;
    blocker_e.level = 10;
    // HP=4000：活过 byte-wrap 一击（死亡面不属本测；golden `levelup_l0` 同口径）。
    blocker_e.hp = 4000;
    blocker_e.max_hp = 4000;
    let blocker = world.add_entity(blocker_e);
    if let Some(map) = world.maps.get_mut(0) {
        map.add_map_object(
            101,
            100,
            mir2_world_bevy::CellType::Monster,
            blocker,
            now_ms(),
        );
    }
    sessions.get_mut("caster").unwrap().state.wabil.level = 30;
    displays.set(
        blocker,
        downlink::ActorDisplay::new(2, 0, 0, 0, 0, 255, b"blocker".to_vec()),
    );
    sessions.get_mut("caster").unwrap().magic.do_motaebo_tick = now_ms() - 5_000;
    let obs = mg07_cast(
        &mut world,
        &online,
        &mut saves,
        &mut sessions,
        &catalog,
        &magic_table,
        &pet_cfg,
        &table,
        &mut displays,
        &gates,
        &mut rx,
        target,
        (2, 100),
    );
    let (x, y) = world.entities.get(caster).map_or((0, 0), |e| (e.x, e.y));
    assert_eq!((x, y), (102, 100), "L0 两轮推挤 ⇒ 施法者进占两格");
    let blocker_hp_after = world.entities.get(blocker).map_or(0, |e| e.hp);
    assert!(blocker_hp_after > 0, "HP=4000 必须活过回绕一击");
    let loss = 4000 - blocker_hp_after;
    assert!(
        (2560..=5119).contains(&loss),
        "byte 回绕伤害界 [2560,5119]（oracle 实测 3755），got {loss} \
         ——若落进 [10,19] 说明 n24 被改成 i32（BUG-REPLICA 勿修）"
    );
    assert_eq!(
        sessions["caster"].state.wabil.mp, 95,
        "L0 SpellPoint=Round(20/4*1)=5"
    );
    let ids: Vec<u16> = obs.frames.iter().map(|f| f.0).collect();
    assert!(
        ids.contains(&mir2_protocol::messages::SM_STRUCK),
        "回绕巨伤必须落成 SM_STRUCK：{ids:?}"
    );
}

/// **Leader 条件（2026-11-27）**：非法方向 ⇒ 前方格==自己——`coordinate_dir` 红字支的
/// **显式机理断言**（不只留在注释）：`GetFrontPosition` 对非法方向不改坐标 ⇒
/// `GetPoseCreate` 取到施法者本人 ⇒ `DoMotaeboCanMotaebo` 的等级门自败 ⇒ `bo35` 红字支。
#[test]
fn warr_27_invalid_dir_front_position_is_own_cell() {
    assert_eq!(
        mir2_world_bevy::magic::front_position(100, 100, 101, 256, 256),
        (100, 100),
        "非法方向 101 ⇒ 前格 = 脚下"
    );
    // 合法方向对照：Right(2) ⇒ (101,100)。
    assert_eq!(
        mir2_world_bevy::magic::front_position(100, 100, 2, 256, 256),
        (101, 100)
    );
}
