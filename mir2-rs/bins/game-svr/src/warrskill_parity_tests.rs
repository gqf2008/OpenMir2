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

    let mut world = mir2_world_bevy::World::with_maps(vec![(32, 32)]);
    let caster = spawn(&mut world, "caster", 100, 100);
    let target = spawn(&mut world, "target", 101, 100);
    let obs_in = spawn(&mut world, "obs_in", 105, 100);
    let obs_out = spawn(&mut world, "obs_out", 140, 100);

    let mut online: OnlineRegistry = Vec::new();
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
