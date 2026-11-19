//! **NPC 多观察者 AOI 金标准**（卡 `openmir2-npc-multiobserve-golden`；`#[cfg(test)]` 专用）。
//!
//! 产物面：`mir2-rs/tests/parity/golden/npc/aoi/*.json`（生成器
//! `mir2-rs/tests/parity/csharp/NpcAoiGolden`，真 `Envir`/`ActorMgr` + 替身 `INetChannel`）。
//!
//! 本模块只做**读入 + 校验 + 逐接收者比较**（纯函数、无世界依赖）：
//!
//! * [`load`]：读文件 + 解析；缺文件/坏 JSON/零用例/零观察者 ⇒ `Err`（PREREQ，**不得**判绿）；
//! * [`validate`]：结构面 —— 5 类观察者（`self`/`near`/`edge`/`out`/`other_map`）+ 隐藏 NPC 行；
//!   `drain_hit_cap` 任一为真 ⇒ `Err`（**命中上限必须红**）；隐藏 NPC 出现在任何观察者的
//!   `hidden_visible`/`visible_actors` 里 ⇒ `Err`（AOI 负例）；
//! * [`compare`]：金标准 vs 实测逐接收者比较（`role` 集合对称差 + 逐帧
//!   `(ident,param,tag,series,tail)` + 帧数）；**全零拒绝**（金标准与实测都为 0 帧 ⇒ `Err`，
//!   防空转）。
//!
//! 墙钟排除：`delivery` 只进金标准 RM 账本、**不进比较面**（[`FrameGolden`] 不含它）。
//!
//! 解析用手写 `serde_json::Value`（本 crate 只依赖 `serde_json`，不新增 `serde` 依赖）。

// golden 的账本面（`npc`/坐标/`rm`/`generator` 等）由 step 2 的对拍驱动器逐条消费；
// 本模块当前只把**结构校验与帧面**接通 ⇒ 允许这些字段暂未被读（不是死代码，是分期消费面）。
#![allow(dead_code)]

use serde_json::Value;

/// 金标准的期望观察者类别（卡 §2；`hidden` 是 NPC 行不是观察者，单列）。
pub const REQUIRED_ROLES: [&str; 5] = ["self", "near", "edge", "out", "other_map"];

/// 一帧的线缆面（`SM` 帧头 + EDCode 尾；`delivery`/tick 类墙钟字段**不在**此结构里）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FrameGolden {
    pub ident: u16,
    pub param: u16,
    pub tag: u16,
    pub series: u16,
    pub tail: String,
}

/// 金标准账本里的一条 RM（只入账；比较面目前只覆盖帧面，RM 留作 step 2 的扩展点）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RmGolden {
    pub ident: i64,
    pub w_param: i64,
    pub p1: i64,
    pub p2: i64,
    pub p3: i64,
    pub msg: String,
    pub late: bool,
    pub delivery: i64,
    pub priority: i64,
}

/// 一个观察者的金标准面。
#[derive(Clone, Debug)]
pub struct ObserverGolden {
    pub role: String,
    pub actor_id: i64,
    pub x: i64,
    pub y: i64,
    pub map: String,
    pub gate: i32,
    pub visible_actors: Vec<i64>,
    pub hidden_visible: bool,
    pub drain_passes: i64,
    pub drain_hit_cap: bool,
    pub rm: Vec<RmGolden>,
    pub frames: Vec<FrameGolden>,
}

/// 隐藏 NPC 行（部署事实：`IsHide=true`、不进任何格 `obj_list`）。
#[derive(Clone, Debug)]
pub struct HiddenNpcGolden {
    pub actor_id: i64,
    pub chr_name: String,
    pub is_hide: bool,
    pub on_map: bool,
}

/// `npc` 行（施动者）。
#[derive(Clone, Debug)]
pub struct NpcGolden {
    pub actor_id: i64,
    pub chr_name: String,
    pub x: i64,
    pub y: i64,
    pub is_hide: bool,
}

/// 一例。
#[derive(Clone, Debug)]
pub struct CaseGolden {
    pub name: String,
    pub npc: NpcGolden,
    pub hidden_npc: HiddenNpcGolden,
    pub send_ref_msg_range: i64,
    pub observers: Vec<ObserverGolden>,
}

/// 一份 golden 文件。
#[derive(Clone, Debug, Default)]
pub struct AoiGolden {
    pub generator: String,
    pub oracle_source: Vec<String>,
    pub max_drain_passes: i64,
    pub excluded_wallclock_fields: Vec<String>,
    pub cases: Vec<CaseGolden>,
}

/// 实测面（Rust 侧 rig 产出；与 [`ObserverGolden`] 同形的子集）。
#[derive(Clone, Debug, Default)]
pub struct ObserverActual {
    pub role: String,
    pub actor_id: i64,
    pub frames: Vec<FrameGolden>,
    pub hidden_visible: bool,
    pub drain_hit_cap: bool,
}

fn as_i64(v: &Value, key: &str, ctx: &str) -> Result<i64, String> {
    v.get(key)
        .and_then(Value::as_i64)
        .ok_or_else(|| format!("PREREQ：{ctx} 缺整数字段 `{key}`"))
}

fn as_str(v: &Value, key: &str, ctx: &str) -> Result<String, String> {
    v.get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| format!("PREREQ：{ctx} 缺字符串字段 `{key}`"))
}

fn as_u16(v: &Value, key: &str, ctx: &str) -> Result<u16, String> {
    let n = as_i64(v, key, ctx)?;
    u16::try_from(n).map_err(|_| format!("PREREQ：{ctx} `{key}` 越界（{n}）"))
}

fn parse_frame(v: &Value, ctx: &str) -> Result<FrameGolden, String> {
    Ok(FrameGolden {
        ident: as_u16(v, "ident", ctx)?,
        param: as_u16(v, "param", ctx)?,
        tag: as_u16(v, "tag", ctx)?,
        series: as_u16(v, "series", ctx)?,
        tail: v
            .get("tail")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
    })
}

fn parse_rm(v: &Value, ctx: &str) -> Result<RmGolden, String> {
    Ok(RmGolden {
        ident: as_i64(v, "ident", ctx)?,
        w_param: v.get("w_param").and_then(Value::as_i64).unwrap_or(0),
        p1: v.get("p1").and_then(Value::as_i64).unwrap_or(0),
        p2: v.get("p2").and_then(Value::as_i64).unwrap_or(0),
        p3: v.get("p3").and_then(Value::as_i64).unwrap_or(0),
        msg: v
            .get("msg")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        late: v.get("late").and_then(Value::as_bool).unwrap_or(false),
        delivery: v.get("delivery").and_then(Value::as_i64).unwrap_or(0),
        priority: v.get("priority").and_then(Value::as_i64).unwrap_or(0),
    })
}

fn parse_observer(v: &Value, ctx: &str) -> Result<ObserverGolden, String> {
    let gate = as_i64(v, "gate", ctx)?;
    let frames = v
        .get("frames")
        .and_then(Value::as_array)
        .ok_or_else(|| format!("PREREQ：{ctx} 缺 `frames` 数组"))?
        .iter()
        .map(|f| parse_frame(f, ctx))
        .collect::<Result<Vec<_>, _>>()?;
    let rm = v
        .get("rm")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .map(|r| parse_rm(r, ctx))
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()?
        .unwrap_or_default();
    let visible_actors = v
        .get("visible_actors")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_i64).collect())
        .unwrap_or_default();
    Ok(ObserverGolden {
        role: as_str(v, "role", ctx)?,
        actor_id: as_i64(v, "actor_id", ctx)?,
        x: v.get("x").and_then(Value::as_i64).unwrap_or(0),
        y: v.get("y").and_then(Value::as_i64).unwrap_or(0),
        map: as_str(v, "map", ctx)?,
        gate: i32::try_from(gate).map_err(|_| format!("PREREQ：{ctx} gate 越界"))?,
        visible_actors,
        hidden_visible: v
            .get("hidden_visible")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        drain_passes: v.get("drain_passes").and_then(Value::as_i64).unwrap_or(0),
        drain_hit_cap: v
            .get("drain_hit_cap")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        rm,
        frames,
    })
}

fn parse_case(v: &Value) -> Result<CaseGolden, String> {
    let name = as_str(v, "name", "case")?;
    let ctx = format!("case {name}");
    let npc_v = v
        .get("npc")
        .ok_or_else(|| format!("PREREQ：{ctx} 缺 `npc`"))?;
    let npc = NpcGolden {
        actor_id: as_i64(npc_v, "actor_id", &ctx)?,
        chr_name: as_str(npc_v, "chr_name", &ctx)?,
        x: npc_v.get("x").and_then(Value::as_i64).unwrap_or(0),
        y: npc_v.get("y").and_then(Value::as_i64).unwrap_or(0),
        is_hide: npc_v
            .get("is_hide")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    };
    let hidden_v = v
        .get("hidden_npc")
        .ok_or_else(|| format!("PREREQ：{ctx} 缺 `hidden_npc`"))?;
    let hidden_npc = HiddenNpcGolden {
        actor_id: as_i64(hidden_v, "actor_id", &ctx)?,
        chr_name: as_str(hidden_v, "chr_name", &ctx)?,
        is_hide: hidden_v
            .get("is_hide")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        on_map: hidden_v
            .get("on_map")
            .and_then(Value::as_bool)
            .unwrap_or(true),
    };
    let observers = v
        .get("observers")
        .and_then(Value::as_array)
        .ok_or_else(|| format!("PREREQ：{ctx} 缺 `observers` 数组"))?
        .iter()
        .map(|o| parse_observer(o, &ctx))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(CaseGolden {
        name,
        npc,
        hidden_npc,
        send_ref_msg_range: v
            .get("send_ref_msg_range")
            .and_then(Value::as_i64)
            .unwrap_or(0),
        observers,
    })
}

/// 读入 golden（缺文件/坏 JSON ⇒ `Err`，PREREQ —— **不得**当作"无差异"）。
///
/// # Errors
/// 文件读失败、JSON 解析失败或字段缺失/类型不符。
pub fn load(path: &std::path::Path) -> Result<AoiGolden, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("PREREQ：读不到 golden {}（{e}）", path.display()))?;
    parse(&text)
}

/// 解析 + 结构校验（见 [`validate`]）。
///
/// # Errors
/// JSON 解析失败、字段缺失/类型不符、或结构校验失败。
pub fn parse(text: &str) -> Result<AoiGolden, String> {
    let v: Value =
        serde_json::from_str(text).map_err(|e| format!("PREREQ：golden JSON 解析失败（{e}）"))?;
    let doc = AoiGolden {
        generator: v
            .get("generator")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        oracle_source: v
            .get("oracle_source")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default(),
        max_drain_passes: v
            .get("max_drain_passes")
            .and_then(Value::as_i64)
            .unwrap_or(0),
        excluded_wallclock_fields: v
            .get("excluded_wallclock_fields")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default(),
        cases: v
            .get("cases")
            .and_then(Value::as_array)
            .map(|a| a.iter().map(parse_case).collect::<Result<Vec<_>, _>>())
            .transpose()?
            .unwrap_or_default(),
    };
    validate(&doc)?;
    Ok(doc)
}

/// 结构面校验（零用例/零观察者/缺角色/命中上限/隐藏 NPC 可见 ⇒ `Err`）。
///
/// # Errors
/// 结构不合卡面（见函数体逐条）。
pub fn validate(doc: &AoiGolden) -> Result<(), String> {
    if doc.cases.is_empty() {
        return Err("golden 零用例 ⇒ 红（防空转）".to_string());
    }
    for case in &doc.cases {
        if case.observers.is_empty() {
            return Err(format!("{}：零观察者 ⇒ 红（防空转）", case.name));
        }
        for role in REQUIRED_ROLES {
            if !case.observers.iter().any(|o| o.role == role) {
                return Err(format!(
                    "{}：缺观察者类别 {role}（卡 §2 要求 5 类）",
                    case.name
                ));
            }
        }
        if case.send_ref_msg_range <= 0 {
            return Err(format!("{}：send_ref_msg_range 必须 > 0", case.name));
        }
        if case.npc.actor_id == case.hidden_npc.actor_id {
            return Err(format!(
                "{}：npc 与 hidden_npc 的 actor_id 不得相同",
                case.name
            ));
        }
        if !case.hidden_npc.is_hide {
            return Err(format!("{}：hidden_npc.is_hide 必须为 true", case.name));
        }
        if case.hidden_npc.on_map {
            return Err(format!(
                "{}：hidden_npc.on_map=true 与部署事实矛盾（0.map (0,0) HighWall ⇒ 不落格）",
                case.name
            ));
        }
        for o in &case.observers {
            if o.drain_hit_cap {
                return Err(format!(
                    "{}：{} 的 DrainPassesHitCap=true ⇒ 红（不得当作完整账本）",
                    case.name, o.role
                ));
            }
            if o.hidden_visible || o.visible_actors.contains(&case.hidden_npc.actor_id) {
                return Err(format!(
                    "{}：{} 看到隐藏 NPC（actor_id={}）⇒ AOI 负例失败",
                    case.name, o.role, case.hidden_npc.actor_id
                ));
            }
        }
    }
    Ok(())
}

/// 逐接收者比较。返回 `Ok(比较过的帧数)`；任何差异/防空转 ⇒ `Err(逐条)`。
///
/// 比较面 = `(role 集合对称差为 0)` + 每个 role 的帧序列逐位
/// `(ident,param,tag,series,tail)` + 帧数 + `hidden_visible` + `drain_hit_cap`。
///
/// # Errors
/// 金标准或实测**总帧数为 0**（全零不得互证）、role 集合不一致、逐帧差异、命中上限。
pub fn compare(
    expected: &[ObserverGolden],
    actual: &[ObserverActual],
) -> Result<usize, Vec<String>> {
    let mut errs = Vec::new();
    let expected_total: usize = expected.iter().map(|o| o.frames.len()).sum();
    let actual_total: usize = actual.iter().map(|o| o.frames.len()).sum();
    if expected_total == 0 || actual_total == 0 {
        errs.push(format!(
            "全零账本不得互证：金标准总帧数={expected_total}，实测总帧数={actual_total} ⇒ 红（防空转）"
        ));
    }
    // role 对称差（只查一侧会把纯新增误判成无变化）。
    let mut e_roles: Vec<&str> = expected.iter().map(|o| o.role.as_str()).collect();
    let mut a_roles: Vec<&str> = actual.iter().map(|o| o.role.as_str()).collect();
    e_roles.sort_unstable();
    a_roles.sort_unstable();
    if e_roles != a_roles {
        errs.push(format!(
            "role 集合不一致：金标准={e_roles:?} 实测={a_roles:?}"
        ));
    }
    for e in expected {
        let Some(a) = actual.iter().find(|a| a.role == e.role) else {
            continue; // 已在对称差里报过
        };
        if e.drain_hit_cap != a.drain_hit_cap {
            errs.push(format!(
                "{}：drain_hit_cap 金标准={} 实测={}",
                e.role, e.drain_hit_cap, a.drain_hit_cap
            ));
        }
        if e.hidden_visible != a.hidden_visible {
            errs.push(format!(
                "{}：hidden_visible 金标准={} 实测={}",
                e.role, e.hidden_visible, a.hidden_visible
            ));
        }
        if e.frames.len() != a.frames.len() {
            errs.push(format!(
                "{}：帧数 金标准={} 实测={}",
                e.role,
                e.frames.len(),
                a.frames.len()
            ));
        }
        for (i, (ef, af)) in e.frames.iter().zip(a.frames.iter()).enumerate() {
            if ef != af {
                errs.push(format!("{}[{i}]：金标准={ef:?} 实测={af:?}", e.role));
            }
        }
    }
    if errs.is_empty() {
        Ok(actual_total)
    } else {
        Err(errs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(ident: u16) -> FrameGolden {
        FrameGolden {
            ident,
            param: 1,
            tag: 2,
            series: 3,
            tail: String::new(),
        }
    }

    fn golden_observer(role: &str, frames: Vec<FrameGolden>) -> ObserverGolden {
        ObserverGolden {
            role: role.to_string(),
            actor_id: 5000,
            x: 0,
            y: 0,
            map: "0".to_string(),
            gate: 5000,
            visible_actors: Vec::new(),
            hidden_visible: false,
            drain_passes: 1,
            drain_hit_cap: false,
            rm: Vec::new(),
            frames,
        }
    }

    fn case_with(observers: Vec<ObserverGolden>) -> CaseGolden {
        CaseGolden {
            name: "selftest".to_string(),
            npc: NpcGolden {
                actor_id: 6002,
                chr_name: "QManage".to_string(),
                x: 0,
                y: 0,
                is_hide: false,
            },
            hidden_npc: HiddenNpcGolden {
                actor_id: 6001,
                chr_name: "RobotManage".to_string(),
                is_hide: true,
                on_map: false,
            },
            send_ref_msg_range: 12,
            observers,
        }
    }

    fn doc_with(cases: Vec<CaseGolden>) -> AoiGolden {
        AoiGolden {
            generator: "selftest".to_string(),
            oracle_source: Vec::new(),
            max_drain_passes: 8,
            excluded_wallclock_fields: vec!["delivery".to_string()],
            cases,
        }
    }

    fn five_roles(frames: Vec<FrameGolden>) -> Vec<ObserverGolden> {
        REQUIRED_ROLES
            .iter()
            .map(|r| golden_observer(r, frames.clone()))
            .collect()
    }

    /// **负例 1**：零用例 ⇒ `validate` 红（防空转）。
    #[test]
    fn aoi_golden_rejects_zero_cases() {
        let err = validate(&doc_with(Vec::new())).expect_err("零用例必须红");
        assert!(err.contains("零用例"), "原文：{err}");
        println!("NEG1 零用例：{err}");
    }

    /// **负例 2**：缺文件 ⇒ `load` 红且带 PREREQ（不得当作无差异）。
    #[test]
    fn aoi_golden_rejects_missing_file() {
        let err =
            load(std::path::Path::new("E:/__no_such_aoi_golden__.json")).expect_err("缺文件必须红");
        assert!(err.starts_with("PREREQ"), "原文：{err}");
        println!("NEG2 缺文件：{err}");
    }

    /// **负例 2b**：交互式坏 JSON / 零用例文件 ⇒ `parse` 红。
    #[test]
    fn aoi_golden_rejects_bad_json_and_empty_parsed_doc() {
        let err = parse("not json").expect_err("坏 JSON 必须红");
        assert!(err.contains("PREREQ"), "原文：{err}");
        println!("NEG2b 坏 JSON：{err}");

        let err = parse(r#"{"cases":[]}"#).expect_err("零用例文件必须红");
        assert!(err.contains("零用例"), "原文：{err}");
        println!("NEG2c 零用例文件：{err}");
    }

    /// **负例 3**：全零账本对全零金标准 ⇒ `compare` 红（防空转）。
    #[test]
    fn aoi_golden_rejects_all_zero_ledger() {
        let expected = five_roles(Vec::new());
        let actual: Vec<ObserverActual> = REQUIRED_ROLES
            .iter()
            .map(|r| ObserverActual {
                role: (*r).to_string(),
                ..Default::default()
            })
            .collect();
        let errs = compare(&expected, &actual).expect_err("全零必须红");
        assert!(errs.iter().any(|e| e.contains("全零")), "原文：{errs:?}");
        println!("NEG3 全零：{errs:?}");
    }

    /// **负例 4**：结构面 —— 命中上限 / 隐藏 NPC 可见 / 缺角色 各红一次。
    #[test]
    fn aoi_golden_rejects_structural_violations() {
        let mut obs = five_roles(vec![frame(10)]);
        obs[1].drain_hit_cap = true;
        let err = validate(&doc_with(vec![case_with(obs)])).expect_err("命中上限必须红");
        assert!(err.contains("DrainPassesHitCap"), "原文：{err}");
        println!("NEG4a 命中上限：{err}");

        let mut obs = five_roles(vec![frame(10)]);
        obs[0].hidden_visible = true;
        let err = validate(&doc_with(vec![case_with(obs)])).expect_err("隐藏可见必须红");
        assert!(err.contains("隐藏 NPC"), "原文：{err}");
        println!("NEG4b 隐藏可见：{err}");

        let mut obs = five_roles(vec![frame(10)]);
        obs.retain(|o| o.role != "edge");
        let err = validate(&doc_with(vec![case_with(obs)])).expect_err("缺角色必须红");
        assert!(err.contains("edge"), "原文：{err}");
        println!("NEG4c 缺角色：{err}");

        let mut obs = five_roles(vec![frame(10)]);
        obs[0].visible_actors.push(6001);
        let err =
            validate(&doc_with(vec![case_with(obs)])).expect_err("visible_actors 含隐藏必须红");
        assert!(err.contains("隐藏 NPC"), "原文：{err}");
        println!("NEG4d visible_actors 含隐藏：{err}");
    }

    /// **正向对照**（证明比较器不是恒 `Err`）：5 类同帧 ⇒ `Ok(5)`；
    /// 删一帧/改一字节 ⇒ `Err` 且点名。
    #[test]
    fn aoi_compare_positive_control_and_frame_byte_diff() {
        let expected = five_roles(vec![frame(10), frame(41)]);
        let actual: Vec<ObserverActual> = REQUIRED_ROLES
            .iter()
            .map(|r| ObserverActual {
                role: (*r).to_string(),
                frames: vec![frame(10), frame(41)],
                ..Default::default()
            })
            .collect();
        assert_eq!(compare(&expected, &actual).expect("同形必须绿"), 10);

        let mut bad = actual.clone();
        bad[2].frames.pop();
        let errs = compare(&expected, &bad).expect_err("少一帧必须红");
        assert!(
            errs.iter()
                .any(|e| e.contains("edge") && e.contains("帧数")),
            "{errs:?}"
        );

        let mut bad = actual;
        bad[4].frames[1].series = 9;
        let errs = compare(&expected, &bad).expect_err("改一字节必须红");
        assert!(errs.iter().any(|e| e.contains("other_map[1]")), "{errs:?}");
    }
}
