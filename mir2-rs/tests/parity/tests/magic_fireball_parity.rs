//! 卡 `openmir2-magic-parity` §19：**判定面**（Rust 比较器）——火球 `1/5` 语义层对拍。
//!
//! 契约：
//! * 金标准 = `mir2-rs/tests/parity/golden/magic/fireball.json`（记录面，C# 产出，QA-1 已独立复现）；
//! * Rust 侧台账由驱动测试产出**同构 JSON**（每接收者的 RM/SM、RNG 逐次 + site、HP 前后）；
//! * 本模块只做**判定**，不含任何"自己跟自己比"的等价逻辑。
//!
//! 断言（缺一条即不可标 `verified`）：
//! 1. **防空转**：金标准缺文件 / 解析失败 / `cases` 为空 ⇒ 红；
//! 2. **两颗负向牙**：每个 case 里 caster 的 `SM_SPELL(17)` 计数 == 0；`obs_out` 的 RM 与 SM 均为空；
//! 3. **RM 多重集**：RM(`ident/wParam/p1/p2/p3`) **排序后按多重集**相等；`priority/late`
//!    **不进判定**（`DEV-RM-PRIORITY-UNOBSERVED`：产品 `PendingMagStruck` 不产出这两个维度，
//!    台账里是驱动按 oracle 语义重建的常量 ⇒ 与 golden 比 = 循环自证；仅作信息列打印）。
//!    （口径 B，opc-07 2026-11-19：RM 顺序是 C# `MsgQueue` 两趟排空 + `VisibleHumanList` 的产物，
//!    不是可复现的行为语义）；**SM 严格**：`ident/param/tag/series/tail` 按序逐项相等
//!    （玩家可见的真实发送顺序）；
//! 4. **HP 一级断言**：`TargetHpBefore/TargetHp` 两侧相等（miss 100→100、edge/hit 100→85 由金标准决定）；
//! 5. **RNG**：行数相等、逐次取值相等、`seq` 连续无缺号、**每一行 `site` ∈ 允许清单**（§16b）。

use mir2_formula::level_ability::{recalc_level_abilitys, LevelValueConfig, PlayerJob};
use serde_json::Value;

/// `site` 允许清单（§16b 表格的代码形态）：`(前缀, 理由)`。
/// **扩充必须在此逐条写理由并进评审 diff**——不允许"能过就加"。
pub const SITE_ALLOWLIST: &[(&str, &str)] = &[
    (
        "crates/world-bevy/src/magic.rs",
        "原版 MagicManager 的威力/骰门/推挤纯函数取数点",
    ),
    (
        "crates/world-bevy/src/magic_effect.rs",
        "魔法效果计算（AntiMagic 骰门、GetPower*）",
    ),
    (
        "crates/world-bevy/src/world.rs",
        "cast_* 施法效果体，对应 DoSpell 的 case 体",
    ),
    (
        "crates/world-bevy/src/slave.rs",
        "deliver_delay_magic_impl：RM_DELAYMAGIC 到期结算",
    ),
    (
        "crates/world-bevy/src/combat.rs",
        "GetAttackPower 的随机区间",
    ),
    (
        "crates/world-bevy/src/merchant.rs",
        "`MapRandomMove` 的两次初始取数（`BaseObject.cs:1214-1237`；21 的 `MagSaceMove` 入口）",
    ),
    (
        "crates/world-bevy/src/space_move.rs",
        "`SpaceMoveGetRandXY` 的移位环取数（`BaseObject.cs:1040-1096`；21 落点不可走时进环，700×700 真图上可达）",
    ),
    (
        "bins/game-svr/src/mongen.rs",
        "造怪 ctor/初始化抽（`BaseObject.cs:439/441`、`AnimalObject.cs:76/77`、`MonsterObject.cs:65`、`AtMonster.cs:10`、`Random(8)` 朝向、`Random(100)` cool-eye）——17/30/41 的 `MakeSlave` → `RegenMonsterByName` 路径；2026-11-26 MG-24 补 Rng 记账时实测命中的站点",
    ),
    (
        "bins/game-svr/src/lib.rs",
        "宿主侧施法接线（帧参数/训练尾）",
    ),
];

pub fn site_allowed(site: &str) -> bool {
    SITE_ALLOWLIST.iter().any(|(p, _)| site.starts_with(p))
}

fn as_array<'a>(v: &'a Value, key: &str) -> Result<&'a Vec<Value>, String> {
    v.get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| format!("缺字段或非数组: {key}"))
}

fn num(v: &Value, k: &str) -> i64 {
    match v.get(k) {
        // `late` 是 JSON bool（`RM_DELAYMAGIC` 的 `LateDelivery`）；按 1/0 比较，
        // 否则两侧都回落 `i64::MIN` ⇒ `late` 列**永远相等**（假绿）。
        Some(Value::Bool(b)) => i64::from(*b),
        Some(x) => x.as_i64().unwrap_or(i64::MIN),
        None => i64::MIN,
    }
}

fn text<'a>(v: &'a Value, k: &str) -> &'a str {
    v.get(k).and_then(Value::as_str).unwrap_or("")
}

/// RNG 段（数量 / 逐次 / site 清单 / seq 连续）。
///
/// **MG-24 起独立成函数**：骨架（`real_rust_ledger_skeleton`）等非 `verify` 的比较路径
/// 必须走**同一份**判据，否则「台账里有 Rng 但没人比」= 验证面漏洞（2026-11-26 实证：
/// 驱动 9 抽 vs golden 6 抽、无 `RngSites`，骨架比较器仍 rc=0）。
fn verify_rng(label: &str, gc: &Value, rc: &Value) -> Vec<String> {
    let mut errs = verify_rng_values(label, gc, rc);
    errs.extend(verify_rng_self(label, rc));
    errs
}

/// 值面：`Rng` 行数 + 逐值。
///
/// 仅当两侧随机流**同位置**时可用（`verify` 的常规路径）；错位口径（装置消费不同抽数）不得
/// 走这里，见 `real_rust_ledger_skeleton` 的 ⑤ 段（声明口径）。
fn verify_rng_values(label: &str, gc: &Value, rc: &Value) -> Vec<String> {
    let mut errs: Vec<String> = Vec::new();
    let g_rng = as_array(gc, "Rng").cloned().unwrap_or_default();
    let r_rng = as_array(rc, "Rng").cloned().unwrap_or_default();
    if g_rng.len() != r_rng.len() {
        errs.push(format!(
            "[{label}] RNG 行数不等：golden={} rust={}",
            g_rng.len(),
            r_rng.len()
        ));
    } else {
        for (i, (g, r)) in g_rng.iter().zip(r_rng.iter()).enumerate() {
            if g.as_i64() != r.as_i64() {
                errs.push(format!("[{label}] RNG[{i}] 不等：golden={g} rust={r}"));
            }
        }
    }
    errs
}

/// 自洽面：`RngSites` 与 `Rng` 等长、每个 site 在允许清单内、`RngSeqs` 连续。
///
/// 与值面解耦：**错位口径的 stem 也必须过这一面**（site 清单/seq 连续是台账自身的完整性，
/// 与两边怎么对齐无关）。
fn verify_rng_self(label: &str, rc: &Value) -> Vec<String> {
    let mut errs: Vec<String> = Vec::new();
    let r_rng = as_array(rc, "Rng").cloned().unwrap_or_default();
    let r_sites = as_array(rc, "RngSites").cloned().unwrap_or_default();
    if r_sites.len() != r_rng.len() {
        errs.push(format!(
            "[{label}] RngSites 与 Rng 长度不等：{} vs {}",
            r_sites.len(),
            r_rng.len()
        ));
    }
    for (i, s) in r_sites.iter().enumerate() {
        let site = s.as_str().unwrap_or("");
        if !site_allowed(site) {
            errs.push(format!("[{label}] RNG[{i}] site 不在允许清单：{site}"));
        }
    }
    let r_seqs = as_array(rc, "RngSeqs").cloned().unwrap_or_default();
    for w in r_seqs.windows(2) {
        let a = w[0].as_i64().unwrap_or(i64::MIN);
        let b = w[1].as_i64().unwrap_or(i64::MIN);
        if b != a.saturating_add(1) {
            errs.push(format!("[{label}] seq 不连续：{a} → {b}"));
            break;
        }
    }
    errs
}

/// 造怪类 stem（17 骷髅 / 30 神兽 / 41 精灵）的 Rng 判据。
///
/// **两侧抽样结构不同，值面只比「同位置」的那一段**（依据：golden 离线替身的
/// `RegenMonsterByName` 只 `new MonsterObject`；Rust 按生产 `MonsterTableSpawner` 走派生
/// ctor/初始化）：
/// - 造怪 ctor 5 抽两侧**同位置同界**：`BaseObject.cs:439/441`（`Random(1500)` /
///   `Random(2000)`）、`AnimalObject.cs:76/77`（`Random(3000)` ×2）、
///   `MonsterObject.cs:65`（`Random(2000)`）；Rust 侧即 `bins/game-svr/src/mongen.rs` 的
///   `draw_base_and_animal` 与 `draw_monster_object`；
/// - golden 第 6 抽 = 训练尾 `Random(3)`（`MagicManager.cs:663`）；
/// - Rust 台账多出的「装置差」抽（朝向 `Random(8)`、cool-eye `Random(100)`，骷髅另多
///   `draw_at_monster` 的 `Random(1500)`）**不进值面**，差额登记 `DEV-MG-24-RNGCOUNT`；
/// - Rust 末抽是训练尾，但流位置与 golden 不同 ⇒ 值不可比（同 seed、不同位置）。
///
/// 抽数与调用形态由调用方声明的 `declared_bounds` 钉住（含 ctor 5 抽 + 装置差 + 训练尾）。
/// `cap_hit`（门 `Rejected`）不造怪不训练 ⇒ 两侧都应 0 抽，**逐值比**（空==空）+ 非空即红。
fn verify_slave_family_rng(
    label: &str,
    gcase: &Value,
    rust: &Value,
    declared_bounds: &[i64],
) -> Vec<String> {
    /// 造怪 ctor 块的抽数（两侧同位置，逐值可比）。
    const SLAVE_CTOR_DRAWS: usize = 5;

    let i64s = |v: &Value| -> Vec<i64> {
        v.as_array()
            .map(|a| a.iter().filter_map(Value::as_i64).collect())
            .unwrap_or_default()
    };
    let g_rng = i64s(&gcase["Rng"]);
    let r_rng = i64s(&rust["Rng"]);
    let r_bounds = i64s(&rust["RngBounds"]);
    let mut errs = verify_rng_self(label, rust);
    match label {
        "cap_hit" => {
            errs.extend(verify_rng_values(label, gcase, rust));
            if !g_rng.is_empty() {
                errs.push(format!(
                    "[{label}] golden 在门 fail 支仍有造怪抽 {g_rng:?}（口径待复核）"
                ));
            }
            if !r_rng.is_empty() {
                errs.push(format!(
                    "[{label}] 门 fail 后 Rust 仍抽了 {r_rng:?}（bounds {r_bounds:?}）——门与造怪顺序可能被改坏"
                ));
            }
        }
        "summon_ok" => {
            // **MG-24 撤 DEV-MG-24-RNGCOUNT（2026-11-27）**：C# 替身升级为 oracle 真子类构造
            // （WhiteSkeleton→AtMonster / ElfMonster 族）后两侧 9/8 抽全同形 ⇒ 值面全开。
            // **位数硬断言（opc-27 要求）**：golden 长度必须恰等于调用方声明的
            // `declared_bounds.len()`（skeleton/angel=9、sinsu=8）——错位即红，非「取最大」。
            if g_rng.len() != declared_bounds.len() {
                errs.push(format!(
                    "[{label}] golden `Rng` 抽数 {} ≠ 声明形态 {}（ctor 5 + 生产差 + 1 训练尾）：{g_rng:?}",
                    g_rng.len(),
                    declared_bounds.len()
                ));
            }
            if r_bounds.len() != r_rng.len() {
                errs.push(format!(
                    "[{label}] RngBounds 与 Rng 长度不等：{} vs {}",
                    r_bounds.len(),
                    r_rng.len()
                ));
            }
            if r_bounds != declared_bounds {
                errs.push(format!(
                    "[{label}] Rust 抽样形态变了：bounds={r_bounds:?}，声明={declared_bounds:?}（抽数/调用序变化须显式重声明，见 DEV-MG-24-RNGCOUNT）"
                ));
            }
            if r_bounds.len() >= SLAVE_CTOR_DRAWS
                && r_bounds[..SLAVE_CTOR_DRAWS] != SLAVE_CTOR_BOUNDS
            {
                errs.push(format!("[{label}] 前 5 抽不是造怪 ctor 块：{r_bounds:?}"));
            }
            // 值面全开：逐位（ctor 5 + 生产差 + 尾）两侧逐值——训练尾值面自本批解锁。
            errs.extend(verify_rng_values(label, gcase, rust));
        }
        other => errs.push(format!(
            "[{label}] 未声明 RNG 口径的造怪支 `{other}`——新支必须显式给出判据（fail-closed）"
        )),
    }
    errs
}

/// 造怪 ctor 块的上界（`BaseObject.cs:439/441`、`AnimalObject.cs:76/77`、`MonsterObject.cs:65`）。
const SLAVE_CTOR_BOUNDS: [i64; 5] = [1500, 2000, 3000, 3000, 2000];

/// 逐 case 判定。`golden` 与 `rust` 必须同构（见模块注释）。
pub fn verify(golden: &Value, rust: &Value) -> Result<(), Vec<String>> {
    let mut errs = Vec::new();

    // 1) 防空转
    let g_cases = match as_array(golden, "cases") {
        Ok(c) if !c.is_empty() => c,
        Ok(_) => {
            errs.push("防空转：金标准 cases 为空（零用例不得判绿）".into());
            return Err(errs);
        }
        Err(e) => {
            errs.push(format!("防空转：金标准不可用（{e}）"));
            return Err(errs);
        }
    };
    let r_cases = match as_array(rust, "cases") {
        Ok(c) => c,
        Err(e) => {
            errs.push(format!("防空转：Rust 台账不可用（{e}）"));
            return Err(errs);
        }
    };
    if r_cases.len() != g_cases.len() {
        errs.push(format!(
            "用例数不等：golden={} rust={}",
            g_cases.len(),
            r_cases.len()
        ));
        return Err(errs);
    }

    for (gi, gc) in g_cases.iter().enumerate() {
        let rc = &r_cases[gi];
        let label = text(gc, "Label");
        if text(rc, "Label") != label {
            errs.push(format!(
                "case[{gi}] 标签不等：golden={label} rust={}",
                text(rc, "Label")
            ));
        }

        // 4) HP 一级断言
        for k in ["TargetHpBefore", "TargetHp", "CasterExtraAbil0"] {
            if num(gc, k) != num(rc, k) {
                errs.push(format!(
                    "[{label}] {k} 不等：golden={} rust={}",
                    num(gc, k),
                    num(rc, k)
                ));
            }
        }

        let g_recv = as_array(gc, "Receivers").cloned().unwrap_or_default();
        let r_recv = as_array(rc, "Receivers").cloned().unwrap_or_default();
        if g_recv.len() != r_recv.len() {
            errs.push(format!(
                "[{label}] 接收者数不等：golden={} rust={}",
                g_recv.len(),
                r_recv.len()
            ));
            continue;
        }

        for (ri, gr) in g_recv.iter().enumerate() {
            let rr = &r_recv[ri];
            let who = text(gr, "Who");
            if text(rr, "Who") != who {
                errs.push(format!(
                    "[{label}] 接收者[{ri}] 名不等：golden={who} rust={}",
                    text(rr, "Who")
                ));
                continue;
            }

            // 卡 §39：怪物接收者的 HP 前/后与逐目标命中门抽值（任一侧有即比；两侧都无 ⇒ 跳过）。
            for k in [
                "HpBefore",
                "HpAfter",
                "GateIndex",
                "GateDraw",
                "GateHit",
                // **MG-32（2026-11-26）**：`TargetCret` 术后值（0 = 已被 `MagMakePrivateTransparent`
                // 清空；否则 = 指向的 ActorId）。只有声明 `TargetCretCaster` 的 case 两侧都有该键。
                "TargetCret",
                // **MG-44（2026-11-26）**：`ObserveState` 支的逃跑面（只有该支两侧都有这两个键）。
                "RunAwayMode",
                "RunAwayTime",
            ] {
                match (gr.get(k), rr.get(k)) {
                    (None, None) => {}
                    (Some(_), Some(_)) => {
                        if num(gr, k) != num(rr, k) {
                            errs.push(format!(
                                "[{label}/{who}] {k} 不等：golden={} rust={}",
                                num(gr, k),
                                num(rr, k)
                            ));
                        }
                    }
                    (Some(_), None) => {
                        errs.push(format!("[{label}/{who}] 台账缺 {k}（golden 有）"))
                    }
                    (None, Some(_)) => {
                        errs.push(format!("[{label}/{who}] golden 缺 {k}（台账有）"))
                    }
                }
            }

            // 2) 负向牙①：caster 不得收 SM_SPELL(17)
            if who == "caster" {
                if let Ok(sms) = as_array(gr, "Sent") {
                    let n = sms.iter().filter(|s| num(s, "ident") == 17).count();
                    if n != 0 {
                        errs.push(format!(
                            "[{label}] 负向牙①：golden caster 收到 {n} 条 SM_SPELL(17)（应为 0）"
                        ));
                    }
                }
            }
            // 2) 负向牙②：obs_out 的 RM/SM **条数必须与金标准一致**（缺省 golden 为 0 ⇒ 仍是
            //    "不得有帧"；MG-57 P2 的收件人边界臂按设计给 obs_out 若干帧）。
            if who == "obs_out" {
                for k in ["Rm", "Sent"] {
                    let want = as_array(gr, k).map(|a| a.len()).unwrap_or(0);
                    let got = as_array(rr, k).map(|a| a.len()).unwrap_or(0);
                    if got != want {
                        errs.push(format!(
                            "[{label}] 负向牙②：obs_out 的 {k} 有 {got} 条（金标准 {want} 条）"
                        ));
                    }
                }
            }

            // 3a) RM：**多重集**比较（口径 B，opc-07 2026-11-19）。
            //
            // 为什么不是按序：C# 侧 `Rm` 是 `ActorEntity.MsgQueue`（.NET `PriorityQueue`）**两趟排空**
            // + `SendRefMsg` 的 `VisibleHumanList` 缓存共同决定的顺序（甚至同一趟 drain 里
            // 「oms 8090（prio 154）排在 8103（prio 167）之后」取决于谁在谁的 Operate 里入队），
            // Rust 宿主没有 RM 层、也不该为复制某次堆布局而引入行为。玩家可见的真实顺序由 SM 面比对。
            // 比较面 = **五个字段**（`ident/wParam/p1/p2/p3`），只对顺序免疫；缺/多条目逐条报出。
            // **`priority/late` 移出判定**（`DEV-RM-PRIORITY-UNOBSERVED`，2026-11-26 裁定）：
            // 产品 `PendingMagStruck`（`crates/world-bevy/src/world.rs`）没有这两个字段，台账的
            // `priority/late` 是驱动按 oracle 语义**重建的常量**（`fireball_rm(..., prio, late)`），
            // 拿常量与 golden 比 = 循环自证、不证明产品。仅当两侧不同时打信息行（不进 `errs`）。
            // 要恢复判定：先让产品路径真的产出并**使用**这两个值（不是为测试建模），再改回六/七字段。
            let rm_fields = ["ident", "wParam", "p1", "p2", "p3"];
            let rm_key = |v: &Value| -> Vec<i64> { rm_fields.iter().map(|f| num(v, f)).collect() };
            let g_rm = as_array(gr, "Rm").cloned().unwrap_or_default();
            let r_rm = as_array(rr, "Rm").cloned().unwrap_or_default();
            let mut g_keys: Vec<Vec<i64>> = g_rm.iter().map(rm_key).collect();
            let mut r_keys: Vec<Vec<i64>> = r_rm.iter().map(rm_key).collect();
            g_keys.sort();
            r_keys.sort();
            if g_keys != r_keys {
                let mut missing = g_keys.clone();
                for k in &r_keys {
                    if let Some(pos) = missing.iter().position(|x| x == k) {
                        missing.remove(pos);
                    }
                }
                let mut extra = r_keys.clone();
                for k in &g_keys {
                    if let Some(pos) = extra.iter().position(|x| x == k) {
                        extra.remove(pos);
                    }
                }
                errs.push(format!(
                    "[{label}/{who}] Rm 多重集不等（ident,wParam,p1,p2,p3）：golden 有 rust 无={missing:?} rust 有 golden 无={extra:?}"
                ));
            }

            // 3a-info) `priority/late` 信息列（**不判定**，`DEV-RM-PRIORITY-UNOBSERVED`）：
            // 只在两侧不同时打一行，便于人工看差异；两侧相同则静默。
            {
                let pl = |v: &Value| -> Vec<(i64, i64)> {
                    let mut out: Vec<(i64, i64)> = as_array(v, "Rm")
                        .map(|a| {
                            a.iter()
                                .map(|m| (num(m, "priority"), num(m, "late")))
                                .collect()
                        })
                        .unwrap_or_default();
                    out.sort();
                    out
                };
                let (gp, rp) = (pl(gr), pl(rr));
                if gp != rp {
                    eprintln!(
                        "[{label}/{who}] INFO（不参与判定）RM priority/late 差异：golden={gp:?} rust={rp:?}（DEV-RM-PRIORITY-UNOBSERVED）"
                    );
                }
            }

            // 3b) SM：**严格顺序** + 逐字段 + 尾串（玩家可见的真实发送顺序）。
            //
            // **唯一例外（`frame_order_scope = "multi_victim_same_tick"`）**：多受害者同刻
            // 延迟受击时，产品是批量 flush、oracle 是逐 actor `Run()`（可观测差异，见
            // `docs/issues/openmir2-b4-multivictim-strike-order.md`）。该标记下本接收者的
            // `Sent` 改为**多重集**比较（字段全比、只放宽顺序）。
            // **撤销条件**：产品卡修复落地后，删掉该标记并恢复严格按序——标记缺失即严格。
            let multi_victim =
                gc["Fixture"]["frame_order_scope"].as_str() == Some("multi_victim_same_tick");
            let ga = as_array(gr, "Sent").cloned().unwrap_or_default();
            let ra = as_array(rr, "Sent").cloned().unwrap_or_default();
            // 新件带 `recog` ⇒ 纳入逐帧严格比较；旧件无该键 ⇒ 不比较（全有或全无已断言）。
            let mut sent_fields: Vec<&str> = vec!["ident", "param", "tag", "series"];
            if ga.first().and_then(|f| f.get("recog")).is_some() {
                sent_fields.push("recog");
            }
            // `due_ms_rel`（相对施法时刻的毫秒；**不是墙钟**）：进比较面 ⇒
            // 多受害者同刻口径放宽的只是"同一 due 内的次序"，时序本身仍被判。
            //
            // **建模约定**：施法批（17/638）的 `due_ms_rel = 0` 是**装置口径**——oracle 实际
            // 在下一个 `Run()` 处理（≈100ms），那是投递调度语义、不是生成时刻语义；
            // 该字段**不作为行为判据**，只用于区分"施法批 vs 交付批"（0 vs 600/1000）。
            if ga.first().and_then(|f| f.get("due_ms_rel")).is_some() {
                sent_fields.push("due_ms_rel");
            }
            if multi_victim {
                // **同根第二实例（施毒 6/48）复用本标记**（不再造第二个名字）：oracle 逐 actor
                // `Run()` vs 产品批量 flush（见 `openmir2-b4-multivictim-strike-order.md`
                // 「施毒 4 支同源」一节，含实测帧序证据与移除条件）。
                // 施毒件的**派生帧**（656/657/100）`due_ms_rel` 是装置落地步（`Sleep` 过冲 ⇒
                // 900/1000），只判**批次类**（0 vs >0）；`640` 的 due（`DeliveryTime − t0`，
                // `TickCount` 15.6ms 粒度）按 **|Δ|≤32ms** 容差判。**字段面不放松**：
                // ident/recog/param/tag/series/tail 仍逐项进多重集键。
                // **按魔号判定**（定义性，不看夹具形状）：6 = AMYOUNSUL、48 = GROUPAMYOUNSUL。
                // 不用 `Fixture.amulet_shape` 存在性——那会让"将来别的 stem 加了护身符旋钮"
                // 静默套上施毒的 due 语义（宽松口径静默扩散）。
                let poison_frames = matches!(
                    golden.get("magic_id").and_then(Value::as_i64),
                    Some(6) | Some(48)
                );
                let due_key = |v: &Value| -> i64 {
                    if poison_frames {
                        if num(v, "ident") == 640 {
                            0
                        } else {
                            i64::from(num(v, "due_ms_rel") != 0)
                        }
                    } else {
                        num(v, "due_ms_rel")
                    }
                };
                let sent_key = |v: &Value| -> (i64, i64, i64, i64, i64, i64, String) {
                    (
                        num(v, "ident"),
                        num(v, "recog"),
                        num(v, "param"),
                        num(v, "tag"),
                        num(v, "series"),
                        due_key(v),
                        text(v, "tail").to_string(),
                    )
                };
                let mut g_keys: Vec<_> = ga.iter().map(sent_key).collect();
                let mut r_keys: Vec<_> = ra.iter().map(sent_key).collect();
                g_keys.sort();
                r_keys.sort();
                if g_keys != r_keys {
                    let mut missing = g_keys.clone();
                    for k in &r_keys {
                        if let Some(pos) = missing.iter().position(|x| x == k) {
                            missing.remove(pos);
                        }
                    }
                    let mut extra = r_keys.clone();
                    for k in &g_keys {
                        if let Some(pos) = extra.iter().position(|x| x == k) {
                            extra.remove(pos);
                        }
                    }
                    errs.push(format!(
                        "[{label}/{who}] Sent 多重集不等（多受害者同刻口径）：golden 有 rust 无={missing:?} rust 有 golden 无={extra:?}"
                    ));
                } else if poison_frames {
                    // late 帧（640）按 TickCount 容差判；判别力：950/600 vs 1000（≥50ms）> 32ms。
                    let mut gd: Vec<i64> = ga
                        .iter()
                        .filter(|v| num(v, "ident") == 640)
                        .map(|v| num(v, "due_ms_rel"))
                        .collect();
                    let mut rd: Vec<i64> = ra
                        .iter()
                        .filter(|v| num(v, "ident") == 640)
                        .map(|v| num(v, "due_ms_rel"))
                        .collect();
                    gd.sort();
                    rd.sort();
                    if gd.iter().chain(rd.iter()).any(|&x| x == i64::MIN) {
                        errs.push(format!(
                            "[{label}/{who}] 640 late due_ms_rel 缺失（一侧 i64::MIN）：golden {gd:?} vs rust {rd:?}"
                        ));
                    } else {
                        let bad = gd.len() != rd.len()
                            || gd
                                .iter()
                                .zip(rd.iter())
                                .any(|(g, r)| (g - r).abs() > mir2_shared::DUE_MS_TOLERANCE_MS);
                        if bad {
                            errs.push(format!(
                                "[{label}/{who}] 640 late due 超出 TickCount 容差（|Δ|≤32ms）：golden {gd:?} vs rust {rd:?}"
                            ));
                        }
                    }
                }
            } else if ga.len() != ra.len() {
                // 逐帧报缺/多（不打印整表；逐字段比较留给等长分支）。
                // **`640`（`SM_MAGIC_LVEXP`）的 `due_ms_rel` 只取批次类**：它是
                // `DeliveryTime − t0`，由 oracle 的 `Environment.TickCount`（15.6ms 粒度）决定
                // ⇒ 同一次生成会在 1000/1016 间抖动；行为判据是「施法批 0 vs 交付批 >0」，
                // 具体毫秒另按 |Δ|≤32ms 判（下方，与施毒件同口径）。
                let due_key = |v: &Value| -> i64 {
                    if num(v, "ident") == 640 {
                        0
                    } else {
                        num(v, "due_ms_rel")
                    }
                };
                let sent_key = |v: &Value| -> (i64, i64, i64, i64, i64, i64, String) {
                    (
                        num(v, "ident"),
                        num(v, "recog"),
                        num(v, "param"),
                        num(v, "tag"),
                        num(v, "series"),
                        due_key(v),
                        text(v, "tail").to_string(),
                    )
                };
                let g_keys: Vec<_> = ga.iter().map(sent_key).collect();
                let r_keys: Vec<_> = ra.iter().map(sent_key).collect();
                let mut missing = g_keys.clone();
                for k in &r_keys {
                    if let Some(pos) = missing.iter().position(|x| x == k) {
                        missing.remove(pos);
                    }
                }
                let mut extra = r_keys.clone();
                for k in &g_keys {
                    if let Some(pos) = extra.iter().position(|x| x == k) {
                        extra.remove(pos);
                    }
                }
                errs.push(format!(
                    "[{label}/{who}] Sent 条数不等（golden={} rust={}）：golden 有 rust 无={missing:?} \
                     rust 有 golden 无={extra:?}",
                    ga.len(),
                    ra.len()
                ));
            } else {
                for (i, (g, r)) in ga.iter().zip(ra.iter()).enumerate() {
                    for &f in &sent_fields {
                        // **`640` 的 `due_ms_rel` 按 |Δ|≤32ms 判**（TickCount 15.6ms 粒度；
                        // 行为判据是批次类，见上方 `multi_victim` 注释与施毒件同口径）。
                        if f == "due_ms_rel" && num(g, "ident") == 640 {
                            let (a, b) = (num(g, f), num(r, f));
                            if a == i64::MIN || b == i64::MIN {
                                errs.push(format!("[{label}/{who}] Sent[{i}].due_ms_rel 缺失（一侧 i64::MIN，不比较差值）：golden={a} rust={b}"));
                            } else if (a - b).abs() > mir2_shared::DUE_MS_TOLERANCE_MS {
                                errs.push(format!(
                                    "[{label}/{who}] Sent[{i}].due_ms_rel 超出 TickCount 容差（|Δ|≤{}ms）：golden={a} rust={b}",
                                    mir2_shared::DUE_MS_TOLERANCE_MS
                                ));
                            }
                            continue;
                        }
                        if num(g, f) != num(r, f) {
                            errs.push(format!(
                                "[{label}/{who}] Sent[{i}].{f} 不等：golden={} rust={}",
                                num(g, f),
                                num(r, f)
                            ));
                        }
                    }
                    if text(g, "tail") != text(r, "tail") {
                        errs.push(format!(
                            "[{label}/{who}] Sent[{i}].tail 不等：golden={:?} rust={:?}",
                            text(g, "tail"),
                            text(r, "tail")
                        ));
                    }
                }
            }
        }

        // 5) RNG：数量 / 逐次 / site 清单 / seq 连续（与骨架比较器共用同一实现）
        errs.extend(verify_rng(label, gc, rc));
    }

    if errs.is_empty() {
        Ok(())
    } else {
        Err(errs)
    }
}

// ---------------------------------------------------------------------------
// 真实台账接入（卡 §22）：game-svr 三支驱动产出台账 ⇒ 喂给同一个 `verify`。
// ---------------------------------------------------------------------------

/// 台账目录：与 game-svr 驱动同口径（`MIR2_MAGIC_LEDGER_DIR` 可覆盖；缺省 `<workspace>/target/magic-parity`）。
/// **标签从 golden 取**（缺 case ⇒ 调用方 assert；防空转）。
fn golden_labels<'a>(golden: &'a Value, stem: &str) -> Vec<&'a str> {
    let labels: Vec<&str> = golden["cases"]
        .as_array()
        .map(|cs| cs.iter().filter_map(|c| c["Label"].as_str()).collect())
        .unwrap_or_default();
    assert!(!labels.is_empty(), "[{stem}] 金标准无 case（防空转）");
    labels
}

fn ledger_dir() -> std::path::PathBuf {
    if let Ok(d) = std::env::var("MIR2_MAGIC_LEDGER_DIR") {
        return std::path::PathBuf::from(d);
    }
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/magic-parity")
}

/// **真实台账 → 判定面**（卡 §22 的收口：不再是 `ledger_from(golden)` 的 identity）。
///
/// 前置（每支必须**各自独立进程**，`rng_trace` 是进程级 `OnceLock`，见卡 §16c）：
/// ```text
/// id=1：cargo test -p game-svr --lib tests::fireball_parity_miss  -- --exact --ignored --test-threads=1
///       （edge / hit 同，换标签）
/// id=5：cargo test -p game-svr --lib tests::fireball5_parity_miss -- --exact --ignored --test-threads=1
///       （edge / hit 同）
/// 比较：cargo test -p mir2-parity-tests --test magic_fireball_parity -- --ignored --exact <用例名>
/// ```
///
/// 防空转：任一份台账缺失/不是合法 JSON/不是恰 1 个 case/`Label` 不符 ⇒ 红（不静默跳过）。
/// **标签参数化只换"取哪几份台账"**：`verify` 与逐字段比对逻辑一行未改。
///
/// **空转防线（卡 §28，2026-11-20 牙 A 假绿事件后加）**：每份台账旁必须有 sidecar
/// `<stem>_<label>.run.json`（驱动写，`{"run_id","seed"}`），且其 `run_id` 必须等于**本次**环境
/// 变量 `MIR2_LEDGER_RUN_ID`。驱动与比较器用同一个 id 运行：
/// ```text
/// $env:MIR2_LEDGER_RUN_ID = "<时间戳+随机串>"
/// cargo test -p game-svr --lib tests::<drive> -- --exact --ignored --test-threads=1
/// cargo test -p mir2-parity-tests --test magic_fireball_parity -- --ignored --exact <用例名>
/// ```
/// 侧车缺失或 id 不符 ⇒ 红并提示「台账陈旧/驱动未跑」；**不设环境变量 ⇒ 红**（fail-closed），
/// 专门拦「驱动编译失败、比较器读上一次台账还绿」这类空转。
/// **统一 RNG 判据（opc-07 2026-11-20 定；卡 `openmir2-golden-fixture-drift` §6）**：
/// `golden.Rng == rust.Rng ++ rust.DeliverRng` —— 两段抽数都要比、按此序拼接；
/// 不接受"记了不判"，也不接受按 seam 字段切换比法（`freeze_at_dospell` 旧件只作
/// legacy artifact、不进比较面）。
///
/// 返回拼接后的序列；**缺列即红**（`Rng`/`DeliverRng` 任一缺失都不允许默认空——
/// 牙④专门钉这一点）。
fn union_rng(case: &Value, label: &str) -> Result<Vec<i64>, String> {
    let seg = |k: &str| -> Result<Vec<i64>, String> {
        let arr = case.get(k).and_then(Value::as_array).ok_or_else(|| {
            format!("[{label}] 台账缺 `{k}` 列（不允许「不记 = 不比」；卡 §6 牙④）")
        })?;
        Ok(arr.iter().filter_map(Value::as_i64).collect())
    };
    let mut out = seg("Rng")?;
    out.extend(seg("DeliverRng")?);
    Ok(out)
}

/// **冻结的 `NeedExps` 表**（`Exps.conf` 的 `[Exp] LevelN` → `SystemShare.Config.NeedExps[N]`，
/// `GetLevelExp` 直接下标取；卡 §8.3）。文件由 M51 生成、与 golden 生成器**同源**：
/// `tests/parity/fixtures/exps-need-exps.json` = `{source, sha256, mtime, need_exps[]}`，
/// `need_exps[level]` 即 `NeedExps[level]`。**缺文件/坏文件即红**（fail-closed）。
fn need_exps_table() -> serde_json::Value {
    let path = format!(
        "{}/fixtures/exps-need-exps.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "冻结的 NeedExps 表缺失：{path}（{e}）——M51 应产出该件；缺它 `derived.max_exp` 无法机检"
        )
    });
    let v: Value = serde_json::from_str(&text).expect("exps-need-exps.json 必须是合法 JSON");
    let sha = v.get("sha256").and_then(Value::as_str).unwrap_or("");
    assert!(
        sha.len() == 64 && sha.chars().all(|c| c.is_ascii_hexdigit()),
        "exps-need-exps.json 的 `sha256` 必须是 64 位 hex（卡 §8.3）"
    );
    assert!(
        v.get("source")
            .and_then(Value::as_str)
            .is_some_and(|s| !s.is_empty()),
        "exps-need-exps.json 缺 `source`（部署 Exps.conf 路径）"
    );
    assert!(
        v.get("need_exps")
            .and_then(Value::as_array)
            .is_some_and(|a| !a.is_empty()),
        "exps-need-exps.json 缺 `need_exps` 数组"
    );
    v
}

/// `NeedExps[level]`（= `GetLevelExp(level)`）。
fn need_exp_for(level: i64) -> Result<i64, String> {
    let table = need_exps_table();
    let arr = table.get("need_exps").and_then(Value::as_array).unwrap();
    usize::try_from(level)
        .ok()
        .and_then(|i| arr.get(i))
        .and_then(Value::as_i64)
        .ok_or_else(|| format!("`need_exps` 未覆盖 level={level}（表长 {}）", arr.len()))
}

/// 金标准该支的 **`Fixture` 契约**（卡 `openmir2-golden-fixture-drift` §8.3/§8.7）：
/// 必须同时满足 —— `rng_mode == "cover_drain"`、`reachability == "reachable"`、
/// 且六类状态字段齐全（`login_order`/`anti_magic`/`target_level`/`target_hp`/`target_max_hp`）；
/// **缺任意键即红**（不允许"默认值兜底"）。`legacy/` 目录的件**不作 verified 依据**。
fn check_golden_contract(golden: &Value, label: &str) -> Result<(), String> {
    let case = golden
        .get("cases")
        .and_then(Value::as_array)
        .and_then(|cs| {
            cs.iter()
                .find(|c| c.get("Label").and_then(Value::as_str) == Some(label))
        })
        .ok_or_else(|| format!("[{label}] 金标准缺该支"))?;
    let fixture = case.get("Fixture").ok_or_else(|| {
        format!("[{label}] 金标准缺 `Fixture`（契约键都应在 Fixture 内；卡 §8.3）")
    })?;
    match fixture.get("rng_mode").and_then(Value::as_str) {
        Some("cover_drain") => {}
        other => {
            return Err(format!(
                "[{label}] 金标准 rng_mode = {other:?}：`verified` 条目必须 cover_drain；                 `freeze_at_dospell` 旧件只作 legacy artifact、不进比较面（卡 §6 牙③）"
            ))
        }
    }
    match fixture.get("reachability").and_then(Value::as_str) {
        Some("reachable") => {}
        other => {
            return Err(format!(
                "[{label}] 金标准 reachability = {other:?}：`verified` 依据必须是 reachable                 （卡 §7/§8.7；unverified/unreachable 不得作依据）"
            ))
        }
    }
    if !fixture.get("login_order").is_some_and(Value::is_boolean) {
        return Err(format!(
            "[{label}] 金标准缺 `Fixture.login_order`（bool；卡 §8.7 缺键即红）"
        ));
    }
    for k in ["anti_magic", "target_level", "target_hp", "target_max_hp"] {
        if !fixture.get(k).is_some_and(Value::is_i64) {
            return Err(format!(
                "[{label}] 金标准缺 `Fixture.{k}`（int；卡 §8.3 缺键即红，不许默认兜底）"
            ));
        }
    }
    // ---- construction（§7 要求的可机检构造路径）----
    let construction = fixture
        .get("construction")
        .ok_or_else(|| format!("[{label}] 金标准缺 `Fixture.construction`（§7 机检构造路径）"))?;
    // 装置（`MagicGolden` `EmitGolden`/`FireballCase`）写的是职业**名**（`spec.Job.ToString()`），
    // 早期设计稿写 0/1/2；两种写法都接受，其余一律红（不兜底）。
    let job_field = construction
        .get("job")
        .ok_or_else(|| format!("[{label}] `construction.job` 缺失（§8.3）"))?;
    let job = match job_field {
        Value::String(s) => match s.as_str() {
            "Warrior" => PlayerJob::Warrior,
            "Wizard" => PlayerJob::Wizard,
            "Taoist" => PlayerJob::Taoist,
            other => {
                return Err(format!(
                    "[{label}] `construction.job` = {other:?} 不在 Warrior/Wizard/Taoist（§8.7）"
                ))
            }
        },
        Value::Number(n) => match n.as_i64() {
            Some(0) => PlayerJob::Warrior,
            Some(1) => PlayerJob::Wizard,
            Some(2) => PlayerJob::Taoist,
            _ => {
                return Err(format!(
                    "[{label}] `construction.job` = {n} 不在 0/1/2（§8.7）"
                ))
            }
        },
        _ => return Err(format!("[{label}] `construction.job` 类型非法（§8.3）")),
    };
    let level = construction
        .get("level")
        .and_then(Value::as_i64)
        .ok_or_else(|| format!("[{label}] `construction.level` 缺失/非整数（§8.3）"))?;
    let equipment = construction
        .get("equipment")
        .and_then(Value::as_array)
        .ok_or_else(|| format!("[{label}] `construction.equipment` 缺失/非数组（§8.3）"))?;
    if construction
        .get("skills")
        .and_then(Value::as_array)
        .is_none()
    {
        return Err(format!(
            "[{label}] `construction.skills` 缺失/非数组（§8.3）"
        ));
    }
    if fixture.get("target_level").and_then(Value::as_i64) != Some(level) {
        return Err(format!(
            "[{label}] `construction.level` {level} ≠ `target_level` {:?}（§8.3）",
            fixture.get("target_level")
        ));
    }
    // 登录序无装备、无技能 ⇒ AntiMagic = 1 + 0 = 1。
    if equipment.is_empty() {
        if fixture.get("anti_magic").and_then(Value::as_i64) != Some(1) {
            return Err(format!(
                "[{label}] `construction.equipment=[]` ⇒ `anti_magic` 必须为 1；实得 {:?}",
                fixture.get("anti_magic")
            ));
        }
    } else {
        return Err(format!(
            "[{label}] `construction.equipment` 非空 ⇒ 装备 MgAvoid 复算未实现（fail-closed；卡 §8.7）"
        ));
    }
    // ---- derived：按构造路径复算，必须与平铺值一致 ----
    let derived = fixture
        .get("derived")
        .ok_or_else(|| format!("[{label}] 金标准缺 `Fixture.derived`（§8.3）"))?;
    for k in ["max_hp", "max_mp", "dc", "ac", "mac", "max_exp"] {
        if !derived.get(k).is_some_and(Value::is_i64) {
            return Err(format!("[{label}] `derived.{k}` 缺失/非整数（§8.3）"));
        }
    }
    let Ok(level_u8) = u8::try_from(level) else {
        return Err(format!(
            "[{label}] `construction.level` = {level} 越界（§8.7）"
        ));
    };
    let lv = recalc_level_abilitys(job, level_u8, &LevelValueConfig::default());
    let got = |k: &str| derived.get(k).and_then(Value::as_i64).unwrap_or(i64::MIN);
    for (k, want) in [
        ("max_hp", i64::from(lv.max_hp)),
        ("max_mp", i64::from(lv.max_mp)),
        ("dc", i64::from(lv.dc)),
        ("ac", i64::from(lv.ac)),
        ("mac", i64::from(lv.mac)),
    ] {
        if got(k) != want {
            return Err(format!(
                "[{label}] `derived.{k}` = {} 与 (job={:?},level={}) 的公式复算 {want} 不一致（§7）",
                got(k),
                job,
                level_u8
            ));
        }
    }
    // 平铺 `target_max_hp` 必须等于 derived.max_hp（不是只信 derived）。
    if fixture.get("target_max_hp").and_then(Value::as_i64) != Some(got("max_hp")) {
        return Err(format!(
            "[{label}] `target_max_hp` {:?} ≠ `derived.max_hp` {}（§8.3 复算一致）",
            fixture.get("target_max_hp"),
            got("max_hp")
        ));
    }
    let hp = fixture
        .get("target_hp")
        .and_then(Value::as_i64)
        .unwrap_or(i64::MIN);
    if hp > got("max_hp") {
        return Err(format!(
            "[{label}] `target_hp` {hp} > `max_hp` {}（§8.3）",
            got("max_hp")
        ));
    }
    // `derived.max_exp` = `GetLevelExp(level)` = 冻结表的 `need_exps[level]`（卡 §8.3）。
    // 该字段**有消费点**：MG-28/29 的 52 帧尾封 `WAbil` 全量（含 `Exp/MaxExp`），不可只查存在。
    let want_exp = need_exp_for(level)?;
    if got("max_exp") != want_exp {
        return Err(format!(
            "[{label}] `derived.max_exp` = {} 与冻结表 `need_exps[{level}]` = {want_exp} 不一致（§8.3）",
            got("max_exp")
        ));
    }
    // ---- 独立抽取计数器（卡 §8.3c）：防手改金标准把 `Rng` 截短以凑对 ----
    let total = fixture
        .get("rng_total_draws")
        .and_then(Value::as_i64)
        .ok_or_else(|| {
            format!("[{label}] 金标准缺 `Fixture.rng_total_draws`（int；缺键即红，§8.3c）")
        })?;
    let rng_len = case
        .get("Rng")
        .and_then(Value::as_array)
        .map(|a| a.len() as i64)
        .ok_or_else(|| format!("[{label}] 金标准缺 `Rng` 数组（§8.3c）"))?;
    if rng_len != total {
        return Err(format!(
            "[{label}] `len(Rng)`={rng_len} ≠ `Fixture.rng_total_draws`={total}：cover_drain 下抽数必须相等（§8.3c）"
        ));
    }
    // ---- `target_hp` 是**施法前**输入（卡 §8.3d）----
    // 有伤害时：`SM_STRUCK(31)` 的 `param` 是**该收件人看到的当前 HP**、`series` 是本次伤害。
    // **同一目标可能经过多次伤害事件**，且 31 的 `param` 可能是"批量刷新后的当前 HP"
    // （例：50 `MabMabe` 先结算第一条 `RM_DELAYMAGIC`（`nPower/3`）再是成功臂一击；两支 31 的
    // `param` 都是最终 HP。见 `magic-serverconf-nondefault-arm-plan` 的差分证据）⇒ 不再逐帧链式
    // 断言 `param`，改用**总量恒等式**：该收件人的 target-31 序列
    // **末帧 `param` + Σ`series` == `target_hp`**。
    // 这只依赖"伤害总额 + 最终 HP 回到施法前值"，与 31 的批量/逐条发送时机无关；同时仍能拦住
    // "把施法后 HP 当 `target_hp`"（那样等式必然不成立）。
    // 致死支（末帧 `param == 0`）：伤害不被 HP 截断（`series` 是掷出值）⇒ 只要求
    // `Σseries >= target_hp`。反例来源：MG-18（10 `(ushort)` 溢出 ⇒ 单击伤害 45921 ≥ HP 100）。
    //
    // **本契约的口径（d8 独立复核 2026-11-21 的三项落字）**：
    // 1) 它只做「输入来源 + 总量自洽」检查，**不是逐帧完整性检查**：**中间** 31 的
    //    `param/series` 由 `verify()` 的跨栈逐帧比较守（golden vs Rust 台账逐值）；"契约绿"
    //    ≠ "每帧都对"。
    // 2) 致死支只验「末帧 `param==0` + `Σseries >= target_hp`」，**不验逐事件致死**；
    //    前提 `target_hp > 0`（`target_hp==0` 时 `Σ>=0` 恒真，属既有弱面）。
    // 3) **已知接受面**（不加补偿帧约束）：`(85,15),(85,0)`、`(70,15)→(80,5)` 这类
    //    "中间帧/末帧单值错但总量凑平"的形态本契约不抓。要抓中间帧就得回到逐帧链式，
    //    而那正是本改动要避开的"批量刷新误红"——**后来人不要为此再补牙把口径拧回去**。
    let target_hp = fixture
        .get("target_hp")
        .and_then(Value::as_i64)
        .unwrap_or(i64::MIN);
    for r in case
        .get("Receivers")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let mut last_hp: Option<i64> = None;
        let mut total_damage: i64 = 0;
        for f in r
            .get("Sent")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if f.get("ident").and_then(Value::as_i64) != Some(31) {
                continue;
            }
            // 新件带 `recog`（受击者 ActorId）⇒ 只对**关于 target 自己**的 31 施加契约；
            // 旁观他人受击的 31（穿透/群体）不在这里判。旧件无 recog ⇒ 维持原口径。
            if let (Some(tid), Some(rc)) = (
                fixture.get("target_actor_id").and_then(Value::as_i64),
                f.get("recog").and_then(Value::as_i64),
            ) {
                if rc != tid {
                    continue;
                }
            }
            let hp_after = f.get("param").and_then(Value::as_i64).unwrap_or(i64::MIN);
            let dmg = f.get("series").and_then(Value::as_i64).unwrap_or(0);
            last_hp = Some(hp_after);
            total_damage = total_damage.saturating_add(dmg);
        }
        let Some(last_hp) = last_hp else { continue };
        if last_hp == 0 {
            if total_damage < target_hp {
                return Err(format!(
                    "[{label}] 31 序列末帧 `HP_after=0` 但 Σ`damage`({total_damage}) < `target_hp`({target_hp})——既非致死一击，也不满足 §8.3d 的恒等式；疑似输入/输出混淆"
                ));
            }
        } else {
            let pre = last_hp.saturating_add(total_damage);
            if pre != target_hp {
                return Err(format!(
                    "[{label}] 31 序列 末帧`HP_after({last_hp})+Σdamage({total_damage})` = {pre} ≠ `Fixture.target_hp` {target_hp}——`target_hp` 必须是**施法前**输入（§8.3d）；**疑似输入/输出混淆**（施法后值当输入，帧面会整体错位且伪装成真分歧）"
                ));
            }
        }
    }
    Ok(())
}

/// 把台账 case 的 `Rng` 换成 `Rng ++ DeliverRng`（`verify` 的 `Rng` 比较面即统一后序列）。
fn pin_union_rng(case: &Value, label: &str) -> Result<Value, String> {
    let union = union_rng(case, label)?;
    let mut c = case.clone();
    if let Some(obj) = c.as_object_mut() {
        obj.insert(
            "Rng".to_string(),
            Value::Array(union.into_iter().map(Value::from).collect()),
        );
        // `RngSites` 与 `Rng` 同源：拼接后 `verify` 的长度自洽检查才成立（`DeliverRngSites` 一并并入）。
        let sites = |k: &str| -> Vec<Value> {
            case.get(k)
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
        };
        let mut all_sites = sites("RngSites");
        all_sites.extend(sites("DeliverRngSites"));
        if !all_sites.is_empty() {
            obj.insert("RngSites".to_string(), Value::Array(all_sites));
        }
        obj.remove("DeliverRng");
        obj.remove("DeliverRngSites");
    }
    Ok(c)
}

/// 卡 §39：把每支每接收者 `Sent` 里 `ident` 相同的帧按 `(param,tag,series,tail)` 排序。
/// 只用于**等优先级广播帧的 tie 序**（§29：泵序是 4 叉堆 artifact，不是语义序）。
fn sort_sm_ident(v: &mut Value, ident: i64) {
    let Some(cases) = v.get_mut("cases").and_then(Value::as_array_mut) else {
        return;
    };
    for c in cases {
        let Some(recv) = c.get_mut("Receivers").and_then(Value::as_array_mut) else {
            continue;
        };
        for r in recv {
            let Some(sent) = r.get_mut("Sent").and_then(Value::as_array_mut) else {
                continue;
            };
            let idxs: Vec<usize> = sent
                .iter()
                .enumerate()
                .filter(|(_, s)| s.get("ident").and_then(Value::as_i64) == Some(ident))
                .map(|(i, _)| i)
                .collect();
            let mut items: Vec<Value> = idxs.iter().map(|i| sent[*i].clone()).collect();
            let key = |v: &Value| -> (i64, i64, i64, String) {
                (
                    v.get("param").and_then(Value::as_i64).unwrap_or(i64::MIN),
                    v.get("tag").and_then(Value::as_i64).unwrap_or(i64::MIN),
                    v.get("series").and_then(Value::as_i64).unwrap_or(i64::MIN),
                    v.get("tail")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                )
            };
            items.sort_by_key(key);
            for (pos, i) in idxs.iter().enumerate() {
                sent[*i] = items[pos].clone();
            }
        }
    }
}

fn real_ledger_verify_labels(stem: &str, golden: &Value, labels: &[&str]) {
    real_ledger_verify_labels_opts(stem, golden, labels, false)
}

/// 删掉 `Receivers` 里 `Sent` 为**空数组**的收件人。
///
/// 台账只登记**有帧**的收件人；金标准对"零帧对象"（如 `holyshield` 的两只怪 `mon0/mon1`）
/// 留一条空 `Sent`。两侧都删空表再 `verify`，否则接收者数必然不等——**不是**放宽判据：
/// 空表没有任何帧可比，非空表的字段仍逐帧严格比较。
fn strip_empty_receivers(v: &mut Value) {
    if let Some(cases) = v.get_mut("cases").and_then(Value::as_array_mut) {
        for c in cases {
            if let Some(rec) = c.get_mut("Receivers").and_then(Value::as_array_mut) {
                rec.retain(|r| {
                    r.get("Sent")
                        .and_then(Value::as_array)
                        .is_some_and(|a| !a.is_empty())
                });
            }
        }
    }
}

/// **MG-38（2026-11-26，卡 `DEV-MONSTER-DISPLAY-FRAME`）**：把某 case 的 `Receivers` 收窄到
/// `mon*`（怪物侧）。用于 `lightflower/undead`——oracle 结算怪物受击后会在玩家接收者上多出
/// 怪物链的 8103/8090/31/656，而 Rust 对无 display 的怪物跳过 31/656（rig 未建 display）⇒
/// 该面如实登记为**当前不可比**，不在本支比较；玩家侧由同 seed 的 `s1` 支全量覆盖。
/// **只按 (stem,label) 调用**，不改变 `verify` 本身的判据。
fn project_monster_only_receivers(v: &mut Value, label: &str) {
    let Some(cases) = v.get_mut("cases").and_then(Value::as_array_mut) else {
        return;
    };
    let Some(c) = cases.iter_mut().find(|c| c["Label"] == label) else {
        return;
    };
    if let Some(rec) = c.get_mut("Receivers").and_then(Value::as_array_mut) {
        rec.retain(|r| r["Who"].as_str().is_some_and(|w| w.starts_with("mon")));
    }
}

/// **MG-50 新臂的 `Rm` 面政策**（opc-27 2026-11-26 裁决：A/B/C 三类分开，不许一起放宽）。
///
/// - **A. 怪物 `Rm` 结构性排除**：Rust 宿主**不建怪物收件人**（B32）⇒ 不比对 `mon*` 的 `Rm`，
///   但**同时断言** Rust 台账里怪物收件人的 `Rm` 确为空——哪天宿主开始建，这条先红，
///   提示"B32 已修复、删掉本排除"（不是 `continue` 了事）。
/// - **B. `10139`（`RM_CHARSTATUSCHANGED`）多出**：与 `657` 同一机制（毒到点 `StatusChanged()`
///   广播 vs oracle 怪物发送队列**头-only 去重**）⇒ 只允许 **Rust 多出**，每玩家收件人恰多
///   `extra_10139` 条（= 本 case 被投毒的怪物数），少于或超过都红；其它 ident 不动。
/// - **C. `10125`（`RM_DURACHANGE`）排除**：**B33**——台账侧 `10125` 是"从线面 642 反推"的，
///   用 `find(...)` 只取**第一**条 `SM_DURACHANGE` 帧（`bins/game-svr/src/lib.rs` 的
///   `RM_DURACHANGE(10125)`（护身符消耗，`UseAmulet`）` 区块）⇒ 多目标时只剩一条。
///   线面 `642` 三条齐且驱动断言 `642.recog == [300,400,500]`（防"排除后把真少扣咽掉"）。
fn apply_poison_arm_rm_policy(
    g: &mut Value,
    r: &mut Value,
    extra_10139_of: &dyn Fn(&str) -> usize,
) {
    // A0：先断言 Rust 怪物收件人的 `Rm` 确为空（B32 撤销信号：宿主一旦建怪物收件人，本排除必须删）。
    for case in r["cases"].as_array().into_iter().flatten() {
        for rec in case["Receivers"].as_array().into_iter().flatten() {
            if rec["Who"].as_str().is_some_and(|w| w.starts_with("mon")) {
                assert!(
                    rec["Rm"].as_array().is_some_and(|a| a.is_empty()),
                    "B32 撤销信号：Rust 已建怪物收件人 Rm ⇒ 删除本排除"
                );
            }
        }
    }
    // 臂定义表（与驱动 `group_amyounsul_arm_assert` 的 base 同源）：怪物 actor id = base+10..
    let base_of = |label: &str| -> i64 {
        match label {
            "lvl1" => 6100,
            "lvl4" => 6200,
            "multi_amulet_monster_first" => 6300,
            "multi_amulet_player_first" => 6400,
            "multi_amulet_three" => 6500,
            "prebuff_green" => 6600,
            "prebuff_double" => 6700,
            "prebuff_mon" => 6900,
            "prebuff_mon_red" => 7000,
            _ => 0,
        }
    };
    let rc = r["cases"].as_array_mut().expect("rust cases");
    let gc = g["cases"].as_array_mut().expect("golden cases");
    for (gcc, rcc) in gc.iter_mut().zip(rc.iter_mut()) {
        let label = rcc["Label"].as_str().unwrap_or("?").to_string();
        let extra = extra_10139_of(&label);
        let base = base_of(&label);
        let is_mon657 = |m: &Value, base: i64| -> bool {
            m["ident"].as_i64() == Some(657)
                && m["recog"]
                    .as_i64()
                    .is_some_and(|v| v >= base + 10 && v < base + 30)
        };
        // A：怪物收件人的 Rm 两边都清空（不可比面）。
        for case in [&mut *gcc, &mut *rcc] {
            for rec in case["Receivers"].as_array_mut().into_iter().flatten() {
                if rec["Who"].as_str().is_some_and(|w| w.starts_with("mon")) {
                    rec["Rm"] = Value::Array(Vec::new());
                }
            }
        }
        // C：两边都剔除 10125（B33：台账只反推第一条线面 642；线面判据在驱动侧）。
        for case in [&mut *gcc, &mut *rcc] {
            for rec in case["Receivers"].as_array_mut().into_iter().flatten() {
                if let Some(rm) = rec["Rm"].as_array_mut() {
                    rm.retain(|m| m["ident"].as_i64() != Some(10125));
                }
            }
        }
        // B：10139 与 657 只允许 **Rust 多出**，逐收件人恰多 `extra` 条（射程外收件人为 0）。
        // `extra == 0` 的臂（毒的是玩家）两边都照旧全严格——不去碰。
        let mut hits = 0usize;
        if extra == 0 {
            continue;
        }
        let n_rec = rcc["Receivers"].as_array().map_or(0, Vec::len);
        for idx in 0..n_rec {
            let who = rcc["Receivers"][idx]["Who"]
                .as_str()
                .unwrap_or("?")
                .to_string();
            if who.starts_with("mon") {
                continue;
            }
            // 金标准侧不许有（缺少方向不接受）。
            for (g_rm, id) in [
                (&gcc["Receivers"][idx]["Rm"], 10139i64),
                (&gcc["Receivers"][idx]["Sent"], 657i64),
            ] {
                let n = g_rm
                    .as_array()
                    .map_or(0, |a| a.iter().filter(|m| m["ident"] == id).count());
                assert_eq!(
                    n, 0,
                    "[{label}/{who}] golden 侧出现 {id}（缺少方向）⇒ 不接受"
                );
            }
            let rec_ref = &rcc["Receivers"].as_array().expect("r recv")[idx];
            let n39 = rec_ref["Rm"]
                .as_array()
                .map_or(0, |a| a.iter().filter(|m| m["ident"] == 10139).count());
            let n657 = rec_ref["Sent"]
                .as_array()
                .map_or(0, |a| a.iter().filter(|m| is_mon657(m, base)).count());
            for (n, id) in [(n39, 10139i64), (n657, 657i64)] {
                assert!(
                    n == 0 || n == extra,
                    "[{label}/{who}] 多出的 {id} 条数必须为 0 或恰为 {extra}（本 case 被投毒怪物数），实测 {n}"
                );
                if n == extra && extra > 0 {
                    hits += 1;
                }
            }
            if n39 == extra && extra > 0 {
                let mut left = extra;
                let rm = rcc["Receivers"].as_array_mut().expect("r recv")[idx]["Rm"]
                    .as_array_mut()
                    .expect("Rm 数组");
                rm.retain(|m| {
                    if left > 0 && m["ident"] == 10139 {
                        left -= 1;
                        false
                    } else {
                        true
                    }
                });
            }
            if n657 == extra && extra > 0 {
                let mut left = extra;
                let sent = rcc["Receivers"].as_array_mut().expect("r recv")[idx]["Sent"]
                    .as_array_mut()
                    .expect("Sent 数组");
                sent.retain(|m| {
                    if left > 0 && is_mon657(m, base) {
                        left -= 1;
                        false
                    } else {
                        true
                    }
                });
            }
        }
        assert!(
            extra == 0 || hits > 0,
            "[{label}] 计数白名单空转：没有任何收件人恰多 {extra} 条"
        );
    }
}

/// `frame_order_scope` 白名单/静态一致性判定（从 `real_ledger_verify_labels_opts` 抽出，行为不变）：
/// `multi_victim_same_tick` 只允许 `fire`/`skill10`（MG-17/18 穿透）与施毒 5 支；出现在别的 stem ⇒ Err。
/// 同 stem 的 `magic_id` 必须与定义一致（fire=9 / skill10=10 / amyounsul*=6 / groupamyounsul*=48）。
fn check_frame_order_scope(golden: &Value, stem: &str) -> Result<(), String> {
    let scope_marked = golden["cases"].as_array().is_some_and(|cs| {
        cs.iter()
            .any(|c| c["Fixture"]["frame_order_scope"].as_str() == Some("multi_victim_same_tick"))
    });
    if scope_marked
        && !matches!(
            stem,
            "fire"
                | "skill10"
                | "amyounsul"
                | "amyounsul2"
                | "amyounsul-shape0"
                | "groupamyounsul"
                | "groupamyounsul2"
                | "groupamyounsul-shape0"
                | "groupamyounsul-shape2"
        )
    {
        return Err(format!(
            "`frame_order_scope=multi_victim_same_tick` 只允许 fire / skill10（MG-17/18）与施毒 5 支；当前 stem={stem}"
        ));
    }
    if scope_marked {
        let mid = golden["magic_id"].as_i64().unwrap_or(-1);
        let ok = match stem {
            "fire" => mid == 9,
            "skill10" => mid == 10,
            "amyounsul" | "amyounsul2" | "amyounsul-shape0" => mid == 6,
            "groupamyounsul"
            | "groupamyounsul2"
            | "groupamyounsul-shape0"
            | "groupamyounsul-shape2" => mid == 48,
            _ => false,
        };
        if !ok {
            return Err(format!(
                "[{stem}] 顺序放宽 marker 与 magic_id 不一致（magic_id={mid}）：fire=9 / skill10=10 / amyounsul*=6 / groupamyounsul*=48"
            ));
        }
    }
    Ok(())
}

fn real_ledger_verify_labels_opts(
    stem: &str,
    golden: &Value,
    labels: &[&str],
    drop_empty_receivers: bool,
) {
    let dir = ledger_dir();
    let magic_id = golden.get("magic_id").cloned().unwrap_or(Value::from(0));
    // **`frame_order_scope` 自检**（判定抽成 `check_frame_order_scope`，行为不变；负例见
    // `frame_order_scope_marker_is_refused_outside_the_whitelist`）。
    check_frame_order_scope(golden, stem).unwrap_or_else(|m| panic!("{m}"));
    // **不**把金标准顶层 `anti_magic` 搬进比较对象：该字段是构造参数（`fireball` 顶层仍是 3），
    // 生效值在**逐 case** 的 `Fixture.anti_magic`（登录序 1），已由 `check_golden_contract` 断言。
    // 搬过来会把金标准与自身比较（恒等），制造"查过"的假象。
    // **`recog` 全有或全无**：同一份 golden 的所有 SM 帧要么全带 `recog`、要么全不带。
    // 混合出现 ⇒ 直接 panic（否则比较器会"部分严格、部分宽松"而没人察觉）。
    if let Some(cs) = golden.get("cases").and_then(Value::as_array) {
        for c in cs {
            let mut with = 0usize;
            let mut without = 0usize;
            for r in c
                .get("Receivers")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                for f in r
                    .get("Sent")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    if f.get("recog").is_some() {
                        with += 1;
                    } else {
                        without += 1;
                    }
                }
            }
            assert!(
                !(with > 0 && without > 0),
                "[{stem}] golden `recog` 混合：{} 帧带、{} 帧不带（必须全有或全无）",
                with,
                without
            );
        }
    }
    let mut cases = Vec::new();
    for label in labels {
        let path = dir.join(format!("{stem}_{label}.json"));
        let text = std::fs::read_to_string(&path).unwrap_or_else(|e| {
            panic!(
                "防空转：缺真实台账 {}（{e}）。先跑 game-svr 对应驱动（命令见文档注释）",
                path.display()
            )
        });
        // **空转防线（卡 §28）**：台账必须来自**本次**驱动运行。
        let want = std::env::var("MIR2_LEDGER_RUN_ID").unwrap_or_default();
        assert!(
            !want.is_empty(),
            "未设置 MIR2_LEDGER_RUN_ID：比较器必须与驱动用同一 id（卡 §28 空转防线）"
        );
        let side = path.with_extension("run.json");
        let side_text = std::fs::read_to_string(&side).unwrap_or_else(|e| {
            panic!(
                "防空转：台账缺 run.json 侧车 {}（{e}）——多见于「驱动未跑/编译失败，读到上一次的陈旧台账」；用同一 MIR2_LEDGER_RUN_ID 重跑驱动",
                side.display()
            )
        });
        let side_v: Value = serde_json::from_str(&side_text).expect("run.json 必须是合法 JSON");
        assert_eq!(
            side_v.get("run_id").and_then(Value::as_str),
            Some(want.as_str()),
            "[{label}] 台账陈旧：run_id 不匹配（当前环境 = {want}）；先用同一 id 重跑驱动"
        );
        let v: Value = serde_json::from_str(&text).expect("防空转：台账必须是合法 JSON");
        let arr = v.as_array().expect("防空转：台账必须是 [case] 数组");
        assert_eq!(arr.len(), 1, "[{label}] 每支台账恰 1 个 case");
        assert_eq!(
            arr[0].get("Label").and_then(Value::as_str),
            Some(*label),
            "[{label}] 台账 Label 不符"
        );
        // **统一 RNG 判据**：金标准必须 cover_drain；台账 Rng 与 DeliverRng 拼接后参与比较。
        if let Err(msg) = check_golden_contract(golden, label) {
            panic!("{msg}");
        }
        let pinned = pin_union_rng(&arr[0], label).unwrap_or_else(|m| panic!("{m}"));
        cases.push(pinned);
    }
    let rust = serde_json::json!({"magic_id": magic_id, "cases": cases});
    // 卡 §39：SM_716 是等优先级广播（RM_10205，prio 221）⇒ 同一接收者的先后是 4 叉堆出队序
    // artifact（§29），不是语义序；宿主把 RM_SPELL 直发、不进 SpellFrameBuf，无法复现「带 17 时
    // 221 组的 tie 序」。仅 groupdeding 对 SM_716 组做多重集比较（逐帧字段仍严格）；其余 SM 与
    // 其他魔号仍严格按序。
    let mut g = golden.clone();
    let mut r = rust;
    if drop_empty_receivers {
        strip_empty_receivers(&mut g);
        strip_empty_receivers(&mut r);
    }
    // **MG-38（2026-11-26，卡 `DEV-MONSTER-DISPLAY-FRAME`）**：`lightflower/undead` 支的**声明式
    // 比较面**收窄到怪物侧——只比 `mon*` 接收者的 RM/SM 与怪物 HP（顶层 RNG / 玩家 `TargetHp`
    // 仍由 `verify` 比较）。理由：`MonsterSpec.OperateStruck` 让 oracle 像真怪一样结算受击，
    // 于是 caster/target/obs_in 会多出**怪物链**的 8103/8090/31/656；而 Rust
    // `drain_mag_struck_b2` 对**无 display 的怪物**早退（跳过 `SM_STRUCK(31)`/656），rig 也
    // 尚未给怪物建 display ⇒ 该面当前**不可比**（撤销条件见卡）。
    // **不放松其它支**：同 seed 的 `s1` 支仍逐接收者全量比较；此处只对这一个 (stem,label) 生效。
    if stem == "lightflower" {
        project_monster_only_receivers(&mut g, "undead");
        project_monster_only_receivers(&mut r, "undead");
    }
    // **MG-50**：新臂（lvl1/lvl4/multi_amulet_*）的 Rm 面按 A/B/C 三类政策处理
    // （B32 怪物 Rm 结构性排除 + 10139 计数白名单 + B33 的 10125 记录口径排除）。
    // **MG-64/65 非 target 扩展（2026-11-28）**：`prebuff_mon`/`prebuff_mon_red` 同族
    // （两怪中毒 ⇒ 每收件人恰多 2 条）；shape2 stem 也进政策（其 `prebuff_mon_red`）。
    if stem == "groupamyounsul" || stem == "groupamyounsul-shape2" {
        apply_poison_arm_rm_policy(&mut g, &mut r, &|label| match label {
            "multi_amulet_monster_first" => 1,
            "multi_amulet_three" => 3,
            "prebuff_mon" => 2,
            "prebuff_mon_red" => 2,
            _ => 0,
        });
    }
    let result = if stem == "groupdeding" {
        sort_sm_ident(&mut g, 716);
        sort_sm_ident(&mut r, 716);
        verify(&g, &r)
    } else {
        verify(&g, &r)
    };
    if let Err(errs) = result {
        panic!(
            "真实台账与金标准不等（{} 条）：\n{}",
            errs.len(),
            errs.join("\n")
        );
    }
}

/// 牙用：把合成的 rust cases 走**与 `real_ledger_verify_labels` 同一条**判据路径
/// （cover_drain 门 → `Rng ++ DeliverRng` 拼接 → `verify`）。
fn verify_union(golden: &Value, rust: &Value) -> Result<(), Vec<String>> {
    let mut out_cases = Vec::new();
    let cases = rust
        .get("cases")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    for c in &cases {
        let label = c
            .get("Label")
            .and_then(Value::as_str)
            .unwrap_or("?")
            .to_string();
        if let Err(msg) = check_golden_contract(golden, &label) {
            return Err(vec![msg]);
        }
        match pin_union_rng(c, &label) {
            Ok(v) => out_cases.push(v),
            Err(m) => return Err(vec![m]),
        }
    }
    let r = serde_json::json!({
        "magic_id": rust.get("magic_id").cloned().unwrap_or(Value::from(0)),
        "anti_magic": rust.get("anti_magic").cloned().unwrap_or(Value::from(0)),
        "cases": out_cases,
    });
    verify(golden, &r)
}

fn synth_golden(label: &str, rng: Value, mode: Option<&str>) -> Value {
    let total = rng.as_array().map_or(0, |a| a.len() as i64);
    // 合成夹具按**统一 schema** 填全（Warrior L20 登录序：MaxHP=224/MaxMP=81/DC=0x0403/AC=0x0200）。
    let mut fixture = serde_json::json!({
        "login_order": true,
        "anti_magic": 1,
        "target_level": 20,
        "target_hp": 100,
        "target_max_hp": 224,
        "reachability": "reachable",
        "construction": { "job": 0, "level": 20, "equipment": [], "skills": [] },
        "derived": {
            "max_hp": 224, "max_mp": 81, "dc": 0x0403, "ac": 0x0200, "mac": 0,
            "max_exp": need_exp_for(20).unwrap_or_else(|m| panic!("{m}")),
        },
        "rng_total_draws": total,
    });
    if let Some(m) = mode {
        fixture["rng_mode"] = Value::from(m);
    } else {
        fixture["rng_mode"] = Value::from("cover_drain"); // 牙③用"缺失"以外的场景单独构造
        fixture.as_object_mut().unwrap().remove("rng_mode");
    }
    serde_json::json!({
        "magic_id": 33,
        "cases": [{ "Label": label, "Fixture": fixture, "Rng": rng, "TargetHpBefore": 100, "TargetHp": 85, "Receivers": [] }],
    })
}

/// 施毒件合成夹具：`magic_id = 6`（定义性判定；`verify` 的施毒 due 语义按魔号触发）。
fn synth_poison_golden(label: &str, rng: Value) -> Value {
    let mut g = synth_golden(label, rng, Some("cover_drain"));
    g["magic_id"] = Value::from(6);
    g
}

/// 施毒件合成台账：`magic_id = 6`（与 [`synth_poison_golden`] 同源）。
fn synth_poison_rust(label: &str, rng: Value) -> Value {
    let mut r = synth_rust(label, rng, Some(serde_json::json!([])));
    r["magic_id"] = Value::from(6);
    r
}

fn synth_rust(label: &str, rng: Value, deliver: Option<Value>) -> Value {
    let mut c = serde_json::json!({
        "Label": label, "TargetHpBefore": 100, "TargetHp": 85, "Receivers": []
    });
    // 台账形态：`RngSites` 与 `Rng` 等长（`verify` 有长度自洽检查）；`DeliverRngSites` 同理。
    let sites = |n: usize| -> Value {
        Value::Array(
            (0..n)
                .map(|_| Value::from("crates/world-bevy/src/magic_effect.rs:1"))
                .collect(),
        )
    };
    let cast_n = rng.as_array().map_or(0, |a| a.len());
    c["RngSites"] = sites(cast_n);
    c["Rng"] = rng;
    if let Some(d) = deliver {
        let dn = d.as_array().map_or(0, |a| a.len());
        c["DeliverRngSites"] = sites(dn);
        c["DeliverRng"] = d;
    }
    serde_json::json!({ "magic_id": 33, "cases": [c] })
}

/// **牙⑦b：多伤害事件（31 的 `param` 批量刷成最终 HP）下总量恒等式仍有牙**。
/// 两支 31：(HP_after=80, damage=15) 与 (HP_after=80, damage=5)，`target_hp=100` 绿；
/// `target_hp=80`（施法后值当输入）必须红并点名。
#[test]
fn tooth_target_hp_multi_event_batch_refresh() {
    let label = "s1"; // 与 `synth_golden` 生成的 Label 一致
    let mk = |target_hp: i64| {
        let mut g = synth_golden("s1", serde_json::json!([1]), Some("cover_drain"));
        let c = &mut g["cases"][0];
        c["Fixture"]["target_actor_id"] = serde_json::json!(1002);
        c["Fixture"]["target_hp"] = serde_json::json!(target_hp);
        c["Receivers"] = serde_json::json!([
            {
                "Who": "target",
                "Rm": [],
                "Sent": [
                    {"ident": 31, "recog": 1002, "param": 80, "series": 15},
                    {"ident": 31, "recog": 1002, "param": 80, "series": 5}
                ]
            }
        ]);
        g
    };
    if let Err(e) = check_golden_contract(&mk(100), label) {
        panic!("多事件 100（施法前）应绿：80 + 15 + 5 = 100；实得 {e}");
    }
    let err = check_golden_contract(&mk(80), label).unwrap_err();
    eprintln!("RED(new, multi-event): {err}");
    assert!(
        err.contains("target_hp") && err.contains("施法前") && err.contains("疑似输入/输出混淆"),
        "施法后值当输入必须红并点名：{err}"
    );
}

/// **等价性证据（共享判据放宽的必要条件）**：把 §8.3d 旧判据（逐帧
/// `param + series == target_hp`）能抓的变异，重放到**既有 `mabmabe` 三支**上：
/// 旧判据必须红、新判据（总量恒等式）也必须红；未变异时两者都绿。
/// 判据从"单次伤害事件"放宽到"总量"后，等价性由此测试证明（卡 §8.3d / 非缺省臂 review）。
#[test]
fn tooth_target_hp_new_criterion_matches_old_on_existing_mabmabe() {
    /// 旧判据（逐帧恒等式）的判定：**任一无伤害帧不成立即 false**；无 31 帧 ⇒ true（两版一致）。
    fn old_criterion_ok(case: &Value) -> bool {
        let fixture = &case["Fixture"];
        let target_hp = fixture["target_hp"].as_i64().unwrap_or(i64::MIN);
        let tid = fixture.get("target_actor_id").and_then(Value::as_i64);
        for r in case["Receivers"].as_array().into_iter().flatten() {
            for f in r["Sent"].as_array().into_iter().flatten() {
                if f["ident"].as_i64() != Some(31) {
                    continue;
                }
                if let (Some(tid), Some(rc)) = (tid, f.get("recog").and_then(Value::as_i64)) {
                    if rc != tid {
                        continue;
                    }
                }
                let hp_after = f["param"].as_i64().unwrap_or(i64::MIN);
                let dmg = f["series"].as_i64().unwrap_or(0);
                if hp_after == 0 {
                    if dmg < target_hp {
                        return false;
                    }
                } else if hp_after.saturating_add(dmg) != target_hp {
                    return false;
                }
            }
        }
        true
    }
    fn mutate_case(mut case: Value, kind: u8) -> Value {
        match kind {
            // A：把施法前 HP 改坏（旧判据必红）
            1 => {
                let hp = case["Fixture"]["target_hp"].as_i64().unwrap();
                case["Fixture"]["target_hp"] = Value::from(hp + 5);
            }
            // B：把某条 target-31 的 series 改坏（旧判据必红）
            2 => {
                let tid = case["Fixture"]["target_actor_id"].as_i64().unwrap();
                let mut done = false;
                for r in case["Receivers"].as_array_mut().into_iter().flatten() {
                    for f in r["Sent"].as_array_mut().into_iter().flatten() {
                        if !done
                            && f["ident"].as_i64() == Some(31)
                            && f.get("recog").and_then(Value::as_i64) == Some(tid)
                            && f["series"].as_i64().unwrap_or(0) > 0
                        {
                            let v = f["series"].as_i64().unwrap() + 1;
                            f["series"] = Value::from(v);
                            done = true;
                        }
                    }
                }
                assert!(done, "B 变异必须命中至少一条 target-31");
            }
            _ => {}
        }
        case
    }
    let g = golden_file("mabmabe.json");
    let mut checked = 0;
    for case in g["cases"].as_array().unwrap() {
        let label = case["Label"].as_str().unwrap();
        // 只对"有 target-31 帧"的支判等价（无伤害帧时两版判据都空过，变异也无从触发）。
        let has_strike = case["Receivers"].as_array().unwrap().iter().any(|r| {
            r["Sent"]
                .as_array()
                .is_some_and(|a| a.iter().any(|f| f["ident"] == 31))
        });
        if !has_strike {
            continue;
        }
        checked += 1;
        let base = case.clone();
        let mut one = g.clone();
        one["cases"] = serde_json::json!([base.clone()]);
        assert!(
            old_criterion_ok(&base),
            "[{label}] 未变异的既有 goldens 必须满足旧判据（前置）"
        );
        assert!(
            check_golden_contract(&one, label).is_ok(),
            "[{label}] 未变异件在新判据下也必须绿"
        );
        for kind in [1u8, 2u8] {
            let mutated = mutate_case(base.clone(), kind);
            assert!(
                !old_criterion_ok(&mutated),
                "[{label}] 变异 {kind} 必须能被**旧**判据抓住（等价性前置）"
            );
            let mut mg = g.clone();
            mg["cases"] = serde_json::json!([mutated]);
            let err = check_golden_contract(&mg, label)
                .expect_err(&format!("[{label}] 变异 {kind}：新判据必须仍然红"));
            eprintln!("RED(new, existing {label}, mutate {kind}): {err}");
        }
    }
    assert!(
        checked >= 2,
        "至少要有两支既有 mabmabe 带伤害帧才能证明等价性；实得 {checked}"
    );
}

/// **牙⑦：`target_hp` 必须是施法前输入**（卡 §8.3d）。合成一个 31 帧（HP=85, damage=15）：
/// `target_hp=100`（施法前）绿；`target_hp=209`（施法后值当输入）红并点名。
#[test]
fn tooth_target_hp_must_be_pre_cast() {
    let label = "s1";
    let mk = |target_hp: i64| {
        let mut g = synth_golden(label, serde_json::json!([1, 2]), Some("cover_drain"));
        g["cases"][0]["Fixture"]["target_hp"] = Value::from(target_hp);
        g["cases"][0]["Receivers"] = serde_json::json!([{
            "Who": "target",
            "Rm": [],
            "Sent": [{ "ident": 31, "param": 85, "tag": 224, "series": 15, "tail": "" }],
        }]);
        g
    };
    assert!(
        check_golden_contract(&mk(100), label).is_ok(),
        "target_hp=100（施法前）应绿"
    );
    let err = check_golden_contract(&mk(209), label).unwrap_err();
    assert!(
        err.contains("target_hp") && err.contains("施法前") && err.contains("疑似输入/输出混淆"),
        "target_hp=209（施法后值）必须红并点名：{err}"
    );
}

/// **牙⑥：含 `WAbil` 的 52 尾里 `Exp/MaxExp` 字节在比较面内**（卡 §8.7）。
///
/// `SM_ABILITY(52)` 的尾 = `EDCode.EncodeMessage(WAbil)`（40B：`Exp` 在 24..28、`MaxExp` 在 28..32），
/// 比较器对 `tail` 做严格串比较 ⇒ 这两个字段天然在判定面内。本牙取真实 golden 的 52 尾，
/// 分别翻掉 `Exp`/`MaxExp` 的一个字节再编码回串，`verify` 必须红在 `tail`。
#[test]
fn tooth_wabil_exp_bytes_compared_red() {
    let golden = golden_file("hangmajinbub.json");
    let cases = golden.get("cases").and_then(Value::as_array).unwrap();
    let s1 = cases
        .iter()
        .find(|c| c.get("Label").and_then(Value::as_str) == Some("s1"))
        .expect("hangmajinbub 必须有 s1");
    // 找任意接收者里的 52 帧
    let (ri, fi, tail) = {
        let mut found = None;
        for (ri, r) in s1
            .get("Receivers")
            .and_then(Value::as_array)
            .unwrap()
            .iter()
            .enumerate()
        {
            for (fi, f) in r
                .get("Sent")
                .and_then(Value::as_array)
                .unwrap()
                .iter()
                .enumerate()
            {
                if f.get("ident").and_then(Value::as_i64) == Some(52) {
                    found = Some((
                        ri,
                        fi,
                        f.get("tail").and_then(Value::as_str).unwrap().to_string(),
                    ));
                    break;
                }
            }
            if found.is_some() {
                break;
            }
        }
        found.expect("golden 必须含 SM_ABILITY(52) 帧")
    };
    let bytes = mir2_protocol::edcode::decode(tail.as_bytes());
    assert_eq!(bytes.len(), 40, "52 尾必须解出 40B WAbil");
    let mut rust_case = s1.clone();
    let with_tail = |c: &mut Value, new_tail: &str| {
        c["Receivers"][ri]["Sent"][fi]["tail"] = Value::from(new_tail);
    };
    for (name, off) in [("Exp", 24usize), ("MaxExp", 28usize)] {
        let mut b = bytes.clone();
        b[off] ^= 0xFF;
        let t = String::from_utf8(mir2_protocol::edcode::encode(&b)).expect("EDCode 输出是 ASCII");
        with_tail(&mut rust_case, &t);
        let r = serde_json::json!({"magic_id": golden.get("magic_id").cloned().unwrap_or(Value::from(0)),
                                   "cases": [rust_case.clone()]});
        let g = serde_json::json!({"magic_id": golden.get("magic_id").cloned().unwrap_or(Value::from(0)),
                                   "cases": [s1.clone()]});
        let err = verify(&g, &r).unwrap_err();
        assert!(
            err.iter().any(|e| e.contains("tail")),
            "翻掉 {name} 字节必须红在 tail：{err:?}"
        );
        // 复原以便下一字段独立测试
        with_tail(&mut rust_case, &tail);
    }
}

/// **牙⑤（契约键）**：`reachability` 缺失或非 `reachable`、或缺任一状态键 ⇒ 红。
#[test]
fn tooth_contract_missing_keys_red() {
    let label = "s1";
    // 缺 reachability
    let mut g = synth_golden(label, serde_json::json!([1, 2]), Some("cover_drain"));
    g["cases"][0]["Fixture"]
        .as_object_mut()
        .unwrap()
        .remove("reachability");
    let msg = check_golden_contract(&g, label).unwrap_err();
    assert!(msg.contains("reachability"), "必须点名 reachability：{msg}");
    // reachability = unverified
    let mut g2 = synth_golden(label, serde_json::json!([1, 2]), Some("cover_drain"));
    g2["cases"][0]["Fixture"]["reachability"] = Value::from("unverified");
    let msg2 = check_golden_contract(&g2, label).unwrap_err();
    assert!(
        msg2.contains("reachability"),
        "必须点名 reachability：{msg2}"
    );
    // 缺 anti_magic
    let mut g3 = synth_golden(label, serde_json::json!([1, 2]), Some("cover_drain"));
    g3["cases"][0]["Fixture"]
        .as_object_mut()
        .unwrap()
        .remove("anti_magic");
    let msg3 = check_golden_contract(&g3, label).unwrap_err();
    assert!(msg3.contains("anti_magic"), "必须点名 anti_magic：{msg3}");
    // 缺 construction
    let mut g4 = synth_golden(label, serde_json::json!([1, 2]), Some("cover_drain"));
    g4["cases"][0]["Fixture"]
        .as_object_mut()
        .unwrap()
        .remove("construction");
    let msg4 = check_golden_contract(&g4, label).unwrap_err();
    assert!(
        msg4.contains("construction"),
        "必须点名 construction：{msg4}"
    );
    // derived 与公式复算不一致（max_hp 改 100）
    let mut g5 = synth_golden(label, serde_json::json!([1, 2]), Some("cover_drain"));
    g5["cases"][0]["Fixture"]["derived"]["max_hp"] = Value::from(100);
    let msg5 = check_golden_contract(&g5, label).unwrap_err();
    assert!(msg5.contains("max_hp"), "必须点名 derived.max_hp：{msg5}");
    // 平铺 target_max_hp 与 derived.max_hp 不一致
    let mut g6 = synth_golden(label, serde_json::json!([1, 2]), Some("cover_drain"));
    g6["cases"][0]["Fixture"]["target_max_hp"] = Value::from(100);
    let msg6 = check_golden_contract(&g6, label).unwrap_err();
    assert!(
        msg6.contains("target_max_hp"),
        "必须点名 target_max_hp：{msg6}"
    );
    // 装备非空 ⇒ fail-closed
    let mut g7 = synth_golden(label, serde_json::json!([1, 2]), Some("cover_drain"));
    g7["cases"][0]["Fixture"]["construction"]["equipment"] = serde_json::json!(["MgAvoid=1"]);
    let msg7 = check_golden_contract(&g7, label).unwrap_err();
    assert!(msg7.contains("equipment"), "必须点名 equipment：{msg7}");
    // 缺 rng_total_draws
    let mut g8 = synth_golden(label, serde_json::json!([1, 2]), Some("cover_drain"));
    g8["cases"][0]["Fixture"]
        .as_object_mut()
        .unwrap()
        .remove("rng_total_draws");
    let msg8 = check_golden_contract(&g8, label).unwrap_err();
    assert!(
        msg8.contains("rng_total_draws"),
        "必须点名 rng_total_draws：{msg8}"
    );
    // rng_total_draws 与 len(Rng) 不等（手改金标准截短 Rng 的形态）
    let mut g9 = synth_golden(label, serde_json::json!([1, 2]), Some("cover_drain"));
    g9["cases"][0]["Fixture"]["rng_total_draws"] = Value::from(5);
    let msg9 = check_golden_contract(&g9, label).unwrap_err();
    assert!(
        msg9.contains("rng_total_draws"),
        "必须点名 rng_total_draws：{msg9}"
    );
    // 正对照：齐全 ⇒ 绿
    let ok = synth_golden(label, serde_json::json!([1, 2]), Some("cover_drain"));
    assert!(check_golden_contract(&ok, label).is_ok(), "齐全应绿");
}

/// **正对照**：`golden.Rng = rust.Rng ++ rust.DeliverRng` ⇒ 绿（证明判据不是恒红）。
#[test]
fn union_rng_positive_control_green() {
    let g = synth_golden("s1", serde_json::json!([1, 2, 3, 4]), Some("cover_drain"));
    let r = synth_rust(
        "s1",
        serde_json::json!([1, 2]),
        Some(serde_json::json!([3, 4])),
    );
    let res = verify_union(&g, &r);
    assert!(res.is_ok(), "拼接一致必须绿：{res:?}");
}

/// **牙①**：删 `DeliverRng` 一条 ⇒ 拼接变短 ⇒ 红。
#[test]
fn tooth_union_drop_deliver_entry_red() {
    let g = synth_golden("s1", serde_json::json!([1, 2, 3, 4]), Some("cover_drain"));
    let r = synth_rust(
        "s1",
        serde_json::json!([1, 2]),
        Some(serde_json::json!([3])),
    );
    let err = verify_union(&g, &r).unwrap_err();
    // 诊断里的大小写是 `RNG 行数不等`/`RNG[i] 不等`：按不区分大小写匹配 RNG。
    assert!(
        err.iter().any(|e| e.to_lowercase().contains("rng")),
        "必须红在 Rng 序列：{err:?}"
    );
}

/// **牙②**：改 `DeliverRng` 任一值 ⇒ 红。
#[test]
fn tooth_union_deliver_value_changed_red() {
    let g = synth_golden("s1", serde_json::json!([1, 2, 3, 4]), Some("cover_drain"));
    let r = synth_rust(
        "s1",
        serde_json::json!([1, 2]),
        Some(serde_json::json!([3, 9])),
    );
    let err = verify_union(&g, &r).unwrap_err();
    assert!(
        err.iter().any(|e| e.to_lowercase().contains("rng")),
        "必须红在 Rng 值：{err:?}"
    );
}

/// **牙⑧（MG-17）：多受害者同刻口径下 `due_ms_rel` 必须进比较面**。
///
/// 合成三帧：**施法批 `due=0`**（638）+ **交付帧 `due=600`**（31/53）+ **交付帧 `due=1000`**（640）。
/// 三颗子断言：
/// ① 基准（golden == rust）⇒ 绿；
/// ② 改**交付帧** 600→601（其余不动）⇒ 多重集不等 ⇒ 红；
/// ③ 改**投递帧** 640 的 1000→600 ⇒ 红（证"施法批 vs 交付批 / 交付帧先后"仍在判）；
/// ④ 改**施法帧** 0→100 ⇒ 红（证"施法批 = 0"的约定不是把 0 与交付混为一谈）。
#[test]
fn tooth_multi_victim_due_ms_rel_compared_red() {
    let label = "s1";
    let side = |cast_due: i64, tail_due: i64| {
        serde_json::json!([{
            "Who": "obs_in",
            "Rm": [],
            "Sent": [
                { "ident": 638, "recog": 1001, "param": 105, "tag": 100, "series": 33,
                  "due_ms_rel": cast_due, "tail": "" },
                { "ident": 31, "recog": 1002, "param": 85, "tag": 224, "series": 15,
                  "due_ms_rel": 600, "tail": "" },
                { "ident": 640, "recog": 9, "param": 1, "tag": 0, "series": 0,
                  "due_ms_rel": tail_due, "tail": "" },
            ],
        }])
    };
    let mk_golden = |cast_due: i64, tail_due: i64| {
        let mut g = synth_golden(label, serde_json::json!([1, 2]), Some("cover_drain"));
        g["cases"][0]["Fixture"]["frame_order_scope"] = Value::from("multi_victim_same_tick");
        g["cases"][0]["Receivers"] = side(cast_due, tail_due);
        g
    };
    let mk_rust = |cast_due: i64, tail_due: i64| {
        // 台账必须有 `DeliverRng` 列（空数组也算"记了"；缺列 ⇒ 牙④ 直接红）。
        let mut r = synth_rust(
            label,
            serde_json::json!([1, 2]),
            Some(serde_json::json!([])),
        );
        r["cases"][0]["Receivers"] = side(cast_due, tail_due);
        r
    };
    let g = mk_golden(0, 1000);
    let base = verify_union(&g, &mk_rust(0, 1000));
    assert!(base.is_ok(), "基准（0/600/1000）必须绿：{base:?}");
    // ② 交付帧 600→601
    let mut r = mk_rust(0, 1000);
    r["cases"][0]["Receivers"][0]["Sent"][1]["due_ms_rel"] = Value::from(601);
    let err = verify_union(&g, &r).unwrap_err();
    assert!(
        err.iter().any(|e| e.contains("多重集")),
        "交付帧 600→601 必须红在多重集：{err:?}"
    );
    // ③ 640 的 1000→600（交付帧先后仍在判）
    let err = verify_union(&g, &mk_rust(0, 600)).unwrap_err();
    assert!(
        err.iter().any(|e| e.contains("多重集")),
        "640 的 due 1000→600 必须红：{err:?}"
    );
    // ④ 施法帧 0→100（施法批约定不是把 0 与交付混同）
    let err = verify_union(&g, &mk_rust(100, 1000)).unwrap_err();
    assert!(
        err.iter().any(|e| e.contains("多重集")),
        "施法帧 0→100 必须红：{err:?}"
    );
}

/// **牙（B22 比较器健壮性）：640 帧缺 `due_ms_rel`（一侧 i64::MIN）⇒ 明确报「缺失」，不得在 `(a-b).abs()` 溢出 panic。**
///
/// 构造：非 multi_victim（不设 frame_order_scope）、magic_id=33（非施毒）、等长两帧
/// `[31(due=600), 640(due=1000)]`；rust 侧 640 帧不写 `due_ms_rel` ⇒ `num` 回落 `i64::MIN`。
/// 旧代码 `(1000 - i64::MIN).abs()` 溢出 panic；修复后必须返回 Err 且含「缺失」。
#[test]
fn tooth_missing_due_640_reports_not_panic() {
    let label = "s1";
    let side = |with_640_due: bool| {
        let mut f640 = serde_json::json!({ "ident": 640, "recog": 9, "param": 1, "tag": 0, "series": 0, "tail": "" });
        if with_640_due {
            f640["due_ms_rel"] = Value::from(1000);
        }
        serde_json::json!([{
            "Who": "obs_in",
            "Rm": [],
            "Sent": [
                { "ident": 31, "recog": 1002, "param": 85, "tag": 224, "series": 15, "due_ms_rel": 600, "tail": "" },
                f640,
            ],
        }])
    };
    let mut g = synth_golden(label, serde_json::json!([1, 2]), Some("cover_drain"));
    g["cases"][0]["Receivers"] = side(true);
    // 退化形态：两侧 640 都有 due ⇒ 必须绿（牙不恒红）
    let mut r_ok = synth_rust(
        label,
        serde_json::json!([1, 2]),
        Some(serde_json::json!([])),
    );
    r_ok["cases"][0]["Receivers"] = side(true);
    assert!(verify_union(&g, &r_ok).is_ok(), "640 两侧都有 due 时应绿");
    // 触发形态：rust 640 缺 due ⇒ i64::MIN ⇒ 旧代码溢出 panic
    let mut r = synth_rust(
        label,
        serde_json::json!([1, 2]),
        Some(serde_json::json!([])),
    );
    r["cases"][0]["Receivers"] = side(false);
    let res = verify_union(&g, &r);
    assert!(res.is_err(), "640 缺 due 必须报错：{res:?}");
    let err = res.unwrap_err();
    assert!(
        err.iter()
            .any(|e| e.contains("缺失") && e.contains("due_ms_rel")),
        "640 缺 due_ms_rel 必须报「缺失」而非 panic：{err:?}"
    );
}

/// **牙③**：旧口径（`freeze_at_dospell` / 无 `rng_mode`）的金标准 ⇒ 直接红。
#[test]
fn tooth_union_legacy_golden_red() {
    let g = synth_golden(
        "s1",
        serde_json::json!([1, 2, 3, 4]),
        Some("freeze_at_dospell"),
    );
    let r = synth_rust(
        "s1",
        serde_json::json!([1, 2]),
        Some(serde_json::json!([3, 4])),
    );
    let err = verify_union(&g, &r).unwrap_err();
    assert!(
        err.iter().any(|e| e.contains("cover_drain")),
        "必须点名 cover_drain：{err:?}"
    );
    let g2 = synth_golden("s1", serde_json::json!([1, 2, 3, 4]), None);
    let err2 = verify_union(&g2, &r).unwrap_err();
    assert!(
        err2.iter().any(|e| e.contains("cover_drain")),
        "缺 rng_mode 同样红：{err2:?}"
    );
}

/// **牙④**：整列 `DeliverRng` 消失（不记了）⇒ 红，**不允许默认空**。
#[test]
fn tooth_union_missing_deliver_column_red() {
    let g = synth_golden("s1", serde_json::json!([1, 2]), Some("cover_drain"));
    let r = synth_rust("s1", serde_json::json!([1, 2]), None);
    let err = verify_union(&g, &r).unwrap_err();
    assert!(
        err.iter().any(|e| e.contains("DeliverRng")),
        "必须点名 DeliverRng 缺列：{err:?}"
    );
}

fn real_ledger_verify(stem: &str, golden: &Value) {
    real_ledger_verify_labels(stem, golden, &["miss", "edge", "hit"]);
}

/// 本侧「帧 `recog` == `id`」的命中数（扫 `Receivers[].Sent` + `Clone.receivers[].frames`）。
/// **B20**：oracle clone actor-id 非确定 ⇒ 只有 Rust 侧有数值 id；golden 侧用 `clone_count`
/// 与"无 10/41 帧"的关系面替代绝对值比较。
fn clone_recog_hits(case: &Value, id: i64) -> usize {
    let from_sent = case["Receivers"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|r| r["Sent"].as_array().into_iter().flatten());
    let from_clone = case["Clone"]["receivers"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|r| r["frames"].as_array().into_iter().flatten());
    from_sent
        .chain(from_clone)
        .filter(|f| num(f, "recog") == id)
        .count()
}

/// Clone 块的逐字段 + 推导面检查（`verify` 之外的部分）。
fn clone_block_checks(g: &Value, r: &Value) -> Vec<String> {
    let mut errs = Vec::new();
    let gc = &g["cases"][0];
    let rc = &r["cases"][0];
    let gb = &gc["Clone"];
    let rb = &rc["Clone"];
    if !gb.is_object() {
        errs.push("golden 缺 `Clone` 块（46 夹具必须落盘）".into());
    }
    if !rb.is_object() {
        errs.push("台账缺 `Clone` 块（驱动必须落盘）".into());
    }
    if !errs.is_empty() {
        return errs;
    }
    if text(gb, "name") != text(rb, "name") {
        errs.push(format!(
            "Clone.name 不等：golden={:?} rust={:?}",
            text(gb, "name"),
            text(rb, "name")
        ));
    }
    for k in [
        "x",
        "y",
        "dir",
        "gender",
        "hair",
        "caster_dir",
        "caster_gender",
        "caster_hair",
        "clone_ordinal",
        "clone_count",
    ] {
        if num(gb, k) != num(rb, k) {
            errs.push(format!(
                "Clone.{k} 不等：golden={} rust={}",
                num(gb, k),
                num(rb, k)
            ));
        }
    }
    // 装置状态断言：克隆体建时为隐藏态（零帧的输入根因）。只断 golden 侧——
    // Rust 骨架未建模 `fixed_hide_mode`（残差登记在卡 §6.7）。
    if gb.get("fixed_hide_mode").and_then(Value::as_bool) != Some(true) {
        errs.push(format!(
            "golden Clone.fixed_hide_mode 必须为 true（PlayObject ctor 置隐藏）；实得 {:?}",
            gb.get("fixed_hide_mode")
        ));
    }
    if gb.get("ob_mode").and_then(Value::as_bool) != Some(false) {
        errs.push(format!(
            "golden Clone.ob_mode 必须为 false；实得 {:?}",
            gb.get("ob_mode")
        ));
    }
    if gb.get("has_640").and_then(Value::as_bool) != rb.get("has_640").and_then(Value::as_bool) {
        errs.push(format!(
            "Clone.has_640 不等：golden={:?} rust={:?}",
            gb.get("has_640"),
            rb.get("has_640")
        ));
    }
    // 逐收件人 10/41 帧：严格按序 + 逐字段 + 尾串（与 `Receivers[].Sent` 同口径）。
    let ga = gb["receivers"].as_array().cloned().unwrap_or_default();
    let ra = rb["receivers"].as_array().cloned().unwrap_or_default();
    if ga.len() != ra.len() {
        errs.push(format!(
            "Clone.receivers 数不等：golden={} rust={}",
            ga.len(),
            ra.len()
        ));
    } else {
        for (i, (gr, rr)) in ga.iter().zip(ra.iter()).enumerate() {
            if text(gr, "who") != text(rr, "who") {
                errs.push(format!(
                    "Clone.receivers[{i}] who 不等：{:?} vs {:?}",
                    text(gr, "who"),
                    text(rr, "who")
                ));
                continue;
            }
            let gf = gr["frames"].as_array().cloned().unwrap_or_default();
            let rf = rr["frames"].as_array().cloned().unwrap_or_default();
            let who = text(gr, "who");
            if gf.len() != rf.len() {
                errs.push(format!(
                    "Clone.receivers[{who}] 帧数不等：golden={} rust={}",
                    gf.len(),
                    rf.len()
                ));
                continue;
            }
            for (j, (a, b)) in gf.iter().zip(rf.iter()).enumerate() {
                for k in ["ident", "recog", "param", "tag", "series", "due_ms_rel"] {
                    if num(a, k) != num(b, k) {
                        errs.push(format!(
                            "Clone.receivers[{who}].frames[{j}].{k} 不等：golden={} rust={}",
                            num(a, k),
                            num(b, k)
                        ));
                    }
                }
                if text(a, "tail") != text(b, "tail") {
                    errs.push(format!(
                        "Clone.receivers[{who}].frames[{j}].tail 不等：golden={:?} rust={:?}",
                        text(a, "tail"),
                        text(b, "tail")
                    ));
                }
            }
        }
    }
    // 推导面（两侧各自成立；不依赖另一侧输出）。
    for (side, c) in [("golden", gc), ("rust", rc)] {
        let b = &c["Clone"];
        let dir = num(b, "dir");
        let caster_dir = num(b, "caster_dir");
        let want_dir = (caster_dir + 4) % 8;
        if dir != want_dir {
            errs.push(format!(
                "[{side}] Clone.dir={dir} ≠ GetBackDir(caster_dir={caster_dir})={want_dir}"
            ));
        }
        let has640 = b.get("has_640").and_then(Value::as_bool).unwrap_or(false);
        let spell_return = c
            .get("SpellReturn")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if spell_return != has640 {
            errs.push(format!(
                "[{side}] `SpellReturn({spell_return}) != has_640({has640})`（训练前提已钉死 ⇒ 必须恒等）"
            ));
        }
        // **出生零帧（负向判据）**：两侧都不得出现 SM_TURN(10)/SM_FEATURECHANGED(41)。
        // oracle 根因：`PlayObject` ctor `FixedHideMode = true`（`PlayObject.cs:993`），克隆体
        // 不登录、无复位 ⇒ ctor 尾 `SendRefMsg(RM_TURN)` 自投递（`BaseObject.Message.cs:390-394`）。
        let all_frames: Vec<&Value> = c["Receivers"]
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|r| r["Sent"].as_array().into_iter().flatten())
            .chain(
                c["Clone"]["receivers"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .flat_map(|r| r["frames"].as_array().into_iter().flatten()),
            )
            .collect();
        let turns = all_frames
            .iter()
            .filter(|f| {
                let id = num(f, "ident");
                id == 10 || id == 41
            })
            .count();
        if turns != 0 {
            errs.push(format!(
                "[{side}] 出生不得有 SM_TURN(10)/SM_FEATURECHANGED(41)；实得 {turns} 帧"
            ));
        }
    }
    errs
}

/// **MG-55（46 分身术）** 真实台账 → 判定面（单支 `clone`，金标准 `golden/magic/clone46.json`）。
///
/// 判据分层：
/// - **驱动**（`tests::clone46_parity_clone`）只记录 + 自带 oracle 形态牙；
/// - **本比较器**：`verify` 的通用面（RM 多重集 / SM 严格序 / RNG / HP）+ `Clone` 块逐字段
///   + 由输入推导的关系（`dir == back_dir(caster_dir)`、`SpellReturn == has_640`、
///     出生帧负向：两侧均不得有 SM_TURN/SM_FEATURECHANGED）。
///
/// 前置（与其它 magic 台账一致）：先跑驱动（`--exact --test-threads=1`），并与本比较器共用
/// 同一个 `MIR2_LEDGER_RUN_ID`（空转防线）。
///
/// 红检：删 `broadcast_ref_msg`（10/41 缺失）、`back_dir→同向`、不复制 Gender/Hair、
/// 技能等级置 3（绕过训练前提）⇒ 分别在 `frames` / `dir` / `feature` / `SpellReturn==has_640` 红。
#[test]
#[ignore = "需先跑 game-svr 驱动（tests::clone46_parity_clone）生成真实台账"]
fn real_rust_ledger_clone46() {
    let stem = "clone46";
    let label = "clone";
    let golden = golden_file("clone46.json");
    check_golden_contract(&golden, label).unwrap_or_else(|m| panic!("{m}"));
    let dir = ledger_dir();
    let want = std::env::var("MIR2_LEDGER_RUN_ID").unwrap_or_default();
    assert!(
        !want.is_empty(),
        "未设置 MIR2_LEDGER_RUN_ID：比较器必须与驱动用同一 id（卡 §28 空转防线）"
    );
    let path = dir.join(format!("{stem}_{label}.json"));
    let txt = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "防空转：缺真实台账 {}（{e}）。先跑驱动 tests::clone46_parity_clone",
            path.display()
        )
    });
    let side = path.with_extension("run.json");
    let side_text = std::fs::read_to_string(&side).unwrap_or_else(|e| {
        panic!(
            "防空转：台账缺 run.json 侧车 {}（{e}）——多见于「驱动未跑/编译失败，读到上一次的陈旧台账」",
            side.display()
        )
    });
    let side_v: Value = serde_json::from_str(&side_text).expect("run.json 必须是合法 JSON");
    assert_eq!(
        side_v.get("run_id").and_then(Value::as_str),
        Some(want.as_str()),
        "[{label}] 台账陈旧：run_id 不匹配（当前环境 = {want}）；先用同一 id 重跑驱动"
    );
    let v: Value = serde_json::from_str(&txt).expect("防空转：台账必须是合法 JSON");
    let arr = v.as_array().expect("防空转：台账必须是 [case] 数组");
    assert_eq!(arr.len(), 1, "[{label}] 每支台账恰 1 个 case");
    assert_eq!(
        arr[0].get("Label").and_then(Value::as_str),
        Some(label),
        "[{label}] 台账 Label 不符"
    );
    let pinned = pin_union_rng(&arr[0], label).unwrap_or_else(|m| panic!("{m}"));
    let g = golden.clone();
    let r = serde_json::json!({
        "magic_id": golden.get("magic_id").cloned().unwrap_or(Value::from(0)),
        "cases": [pinned],
    });
    // **B20（2026-11-21 裁定）**：oracle `ActorEntity` ctor 的 `ActorMgr.GetNextIdentity()`
    // 走 GUID/随机池 ⇒ clone 的绝对 actor-id 跨运行非确定，**不是建模差异**，只是装置在
    // 用同一非确定源。golden 只记 policy/序数/数量；绝对值不跨栈比。
    let gc0 = &g["cases"][0];
    let rc0 = &r["cases"][0];
    let gb0 = &gc0["Clone"];
    let rb0 = &rc0["Clone"];
    assert!(
        gb0.get("actor_id").is_none(),
        "golden 不得记录 clone 绝对 actor_id（B20：GUID 非确定，绝对 id 无跨栈意义）"
    );
    assert!(
        gb0.get("rel_to_caster").is_none(),
        "golden 不得记录 rel_to_caster（同上）"
    );
    assert_eq!(
        text(gb0, "actor_id_policy"),
        "oracle_guid_nondeterministic",
        "golden 必须带 B20 policy 标记（不是漏记）"
    );
    for (side, b) in [("golden", gb0), ("rust", rb0)] {
        assert_eq!(num(b, "clone_ordinal"), 0, "[{side}] clone 创建序必须为 0");
        assert_eq!(num(b, "clone_count"), 1, "[{side}] 必须恰有一个 clone");
    }
    let r_cid = num(rb0, "actor_id");
    assert!(r_cid > 0, "Rust 台账必须记本侧 clone actor_id（可见性）");
    let siblings = rb0["sibling_actor_ids"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(
        !siblings.iter().any(|v| v.as_i64() == Some(r_cid)),
        "Rust clone id 不得与 caster/target/obs 的 id 相撞"
    );
    // oracle 出生零帧 ⇒ 任何一侧都不应有回指 clone 的 10/41 帧（有则必须先解释）。
    assert_eq!(
        clone_recog_hits(gc0, r_cid) + clone_recog_hits(rc0, r_cid),
        0,
        "不得出现 recog == clone id 的 10/41 帧（oracle FixedHideMode 自投递 ⇒ 零帧）"
    );
    let mut errs = clone_block_checks(&g, &r);
    if let Err(mut e) = verify(&g, &r) {
        errs.append(&mut e);
    }
    assert!(
        errs.is_empty(),
        "克隆台账与金标准不等（{} 条）：\n{}",
        errs.len(),
        errs.join("\n")
    );
}

/// FireCross 块检查（`verify` 之外）：事件表逐项 + 安全区/预占的输入-输出一致性。
fn firecross_block_checks(g: &Value, r: &Value) -> Vec<String> {
    let mut errs = Vec::new();
    for (gi, gc) in g["cases"].as_array().into_iter().flatten().enumerate() {
        let rc = &r["cases"][gi];
        let label = text(gc, "Label");
        let gb = &gc["FireCross"];
        let rb = &rc["FireCross"];
        if !gb.is_object() {
            errs.push(format!("[{label}] golden 缺 FireCross 块"));
            continue;
        }
        if !rb.is_object() {
            errs.push(format!("[{label}] 台账缺 FireCross 块"));
            continue;
        }
        // 输入面 / 结果标量。
        for k in [
            "safe_zone_flag",
            "disable_in_safe_zone_fire_cross",
            "in_safe_zone_at_cast",
            "red_sysmsg",
            "has_640",
        ] {
            if gb.get(k).and_then(Value::as_bool) != rb.get(k).and_then(Value::as_bool) {
                errs.push(format!(
                    "[{label}] FireCross.{k} 不等：golden={:?} rust={:?}",
                    gb.get(k),
                    rb.get(k)
                ));
            }
        }
        let gp = gb["pre_event_cells"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let rp = rb["pre_event_cells"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        if gp != rp {
            errs.push(format!(
                "[{label}] FireCross.pre_event_cells 不等：golden={gp:?} rust={rp:?}"
            ));
        }
        // 事件表：两侧均按 (x,y) 排序后逐项比。
        let ge = gb["events"].as_array().cloned().unwrap_or_default();
        let re = rb["events"].as_array().cloned().unwrap_or_default();
        if ge.len() != re.len() {
            errs.push(format!(
                "[{label}] FireCross.events 条数不等：golden={} rust={}",
                ge.len(),
                re.len()
            ));
        } else {
            for (i, (a, b)) in ge.iter().zip(re.iter()).enumerate() {
                for k in ["x", "y", "type", "damage", "continue_ms"] {
                    if num(a, k) != num(b, k) {
                        errs.push(format!(
                            "[{label}] FireCross.events[{i}].{k} 不等：golden={} rust={}",
                            num(a, k),
                            num(b, k)
                        ));
                    }
                }
            }
        }
        // 推导面（两侧各自成立）：事件类型/持续时间形态、安全区支零事件、非安全区支 5−预占。
        for (side, c) in [("golden", gc), ("rust", rc)] {
            let b = &c["FireCross"];
            let events = b["events"].as_array().cloned().unwrap_or_default();
            let pre = b["pre_event_cells"].as_array().map_or(0usize, |a| a.len());
            let disable = b
                .get("disable_in_safe_zone_fire_cross")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let insafe = b
                .get("in_safe_zone_at_cast")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let blocked = disable && insafe;
            let want = if blocked { 0 } else { 5 - pre };
            if events.len() != want {
                errs.push(format!(
                    "[{side}] events 条数 {} ≠ {want}（blocked={blocked}，pre={pre}）",
                    events.len()
                ));
            }
            if blocked {
                if b.get("red_sysmsg").and_then(Value::as_bool) != Some(true) {
                    errs.push(format!("[{side}] 安全区支必须有 red_sysmsg"));
                }
                if b.get("has_640").and_then(Value::as_bool) != Some(false) {
                    errs.push(format!("[{side}] 安全区支不得有 640"));
                }
            } else {
                if b.get("red_sysmsg").and_then(Value::as_bool) != Some(false) {
                    errs.push(format!("[{side}] 非安全区支不得有红字"));
                }
                if b.get("has_640").and_then(Value::as_bool) != Some(true) {
                    errs.push(format!("[{side}] 非安全区支必须有 640"));
                }
            }
            let mut cont: Option<i64> = None;
            for e in &events {
                if num(e, "type") != 5 {
                    errs.push(format!(
                        "[{side}] 事件 type={} ≠ ET_FIRE(5)",
                        num(e, "type")
                    ));
                }
                let cm = num(e, "continue_ms");
                if cm <= 0 || cm % 1000 != 0 {
                    errs.push(format!(
                        "[{side}] continue_ms={cm} 必须 >0 且为 1000 的整数倍"
                    ));
                }
                if let Some(prev) = cont {
                    if prev != cm {
                        errs.push(format!(
                            "[{side}] 同支事件 continue_ms 不一致：{prev} vs {cm}"
                        ));
                    }
                }
                cont = Some(cm);
            }
            // 安全区支的 golden 必须记录红字原文（台账不记文本，Sent 尾已逐字节比）。
            if blocked && side == "golden" {
                let t = text(b, "red_sysmsg_text");
                if t.is_empty() {
                    errs.push("[golden] 安全区支 red_sysmsg_text 为空".into());
                }
            }
        }
    }
    errs
}

/// **MG-36 结算面（2026-11-27）**：`FireTrace` 逐行全字段比对（golden vs 台账，case 级块）。
/// 两侧都缺键/null ⇒ 跳过（golden 非 tick 支不带该键；台账形状对齐）；
/// 一侧非空一侧缺/null ⇒ 红（防「台账静默丢轨迹列」）；
/// 行数不等 ⇒ 红；逐行比 `delta/ms/target_hp/target_hp_after_run/struck` 五字段。
fn fire_trace_checks(g: &Value, r: &Value) -> Vec<String> {
    let mut errs = Vec::new();
    let g_cases = g["cases"].as_array().cloned().unwrap_or_default();
    let r_cases = r["cases"].as_array().cloned().unwrap_or_default();
    for (gc, rc) in g_cases.iter().zip(r_cases.iter()) {
        let label = text(gc, "Label");
        let g_absent = gc.get("FireTrace").is_none_or(Value::is_null);
        let r_absent = rc.get("FireTrace").is_none_or(Value::is_null);
        match (g_absent, r_absent) {
            (true, true) => {}
            (false, false) => {
                let gr = gc["FireTrace"].as_array().cloned().unwrap_or_default();
                let rr = rc["FireTrace"].as_array().cloned().unwrap_or_default();
                if gr.len() != rr.len() {
                    errs.push(format!(
                        "[{label}] FireTrace 行数不等：golden={} rust={}",
                        gr.len(),
                        rr.len()
                    ));
                    continue;
                }
                for (i, (a, b)) in gr.iter().zip(rr.iter()).enumerate() {
                    for k in ["delta", "ms", "target_hp", "target_hp_after_run", "struck"] {
                        if num(a, k) != num(b, k) {
                            errs.push(format!(
                                "[{label}] FireTrace[{i}].{k} 不等：golden={} rust={}",
                                num(a, k),
                                num(b, k)
                            ));
                        }
                    }
                }
            }
            (false, true) => errs.push(format!("[{label}] 台账缺/null FireTrace（golden 有轨迹）")),
            (true, false) => errs.push(format!("[{label}] golden 缺/null FireTrace（台账有值）")),
        }
    }
    errs
}

/// **MG-36（22 火墙）** 真实台账 → 判定面（四支 base/pre_center/safe_zone/tick_boundaries）。
///
/// 判据分层：驱动只记录 + 自带退化守卫（事件数/帧序/RNG 抽数）；本比较器做
/// `verify` 的通用面 + `FireCross` 块逐字段 + 由输入推导的一致性（安全区支零事件/无 640、
/// 非安全区 5−预占、`continue_ms` 为 1000 倍数且同支一致、`type==ET_FIRE(5)`）。
#[test]
#[ignore = "需先跑 game-svr 驱动（tests::firecross_parity_*）生成真实台账"]
fn real_rust_ledger_firecross() {
    let stem = "firecross";
    // **MG-36 ①（2026-11-26）**：`tick_boundaries`——地图事件两门整点臂（驱动侧解析硬写断言）。
    let labels = ["base", "pre_center", "safe_zone", "tick_boundaries"];
    let golden = golden_file("firecross.json");
    for label in labels {
        check_golden_contract(&golden, label).unwrap_or_else(|m| panic!("{m}"));
    }
    let dir = ledger_dir();
    let want = std::env::var("MIR2_LEDGER_RUN_ID").unwrap_or_default();
    assert!(
        !want.is_empty(),
        "未设置 MIR2_LEDGER_RUN_ID：比较器必须与驱动用同一 id（卡 §28 空转防线）"
    );
    let mut cases = Vec::new();
    for label in labels {
        let path = dir.join(format!("{stem}_{label}.json"));
        let txt = std::fs::read_to_string(&path).unwrap_or_else(|e| {
            panic!(
                "防空转：缺真实台账 {}（{e}）。先跑驱动 tests::firecross_parity_*",
                path.display()
            )
        });
        let side = path.with_extension("run.json");
        let side_text = std::fs::read_to_string(&side).unwrap_or_else(|e| {
            panic!(
                "防空转：台账缺 run.json 侧车 {}（{e}）——多见于「驱动未跑/编译失败，读到上一次的陈旧台账」",
                side.display()
            )
        });
        let side_v: Value = serde_json::from_str(&side_text).expect("run.json 必须是合法 JSON");
        assert_eq!(
            side_v.get("run_id").and_then(Value::as_str),
            Some(want.as_str()),
            "[{label}] 台账陈旧：run_id 不匹配（当前环境 = {want}）；先用同一 id 重跑驱动"
        );
        let v: Value = serde_json::from_str(&txt).expect("防空转：台账必须是合法 JSON");
        let arr = v.as_array().expect("防空转：台账必须是 [case] 数组");
        assert_eq!(arr.len(), 1, "[{label}] 每支台账恰 1 个 case");
        assert_eq!(
            arr[0].get("Label").and_then(Value::as_str),
            Some(label),
            "[{label}] 台账 Label 不符"
        );
        let pinned = pin_union_rng(&arr[0], label).unwrap_or_else(|m| panic!("{m}"));
        cases.push(pinned);
    }
    let g = golden.clone();
    let r = serde_json::json!({
        "magic_id": golden.get("magic_id").cloned().unwrap_or(Value::from(0)),
        "cases": cases,
    });
    let mut errs = firecross_block_checks(&g, &r);
    // **MG-36 结算面（2026-11-27）**：`FireTrace`（含 `target_hp_after_run` HP 轨迹）逐行比对。
    errs.append(&mut fire_trace_checks(&g, &r));
    if let Err(mut e) = verify(&g, &r) {
        errs.append(&mut e);
    }
    assert!(
        errs.is_empty(),
        "火墙台账与金标准不等（{} 条）：\n{}",
        errs.len(),
        errs.join("\n")
    );
}

/// Tamming 块检查（`verify` 之外）：建模面逐字段 + 分支推导（60005 公式/受奴后效/狂暴）。
fn tamming_block_checks(g: &Value, r: &Value) -> Vec<String> {
    let mut errs = Vec::new();
    let g_cases = g["cases"].as_array().cloned().unwrap_or_default();
    let r_cases = r["cases"].as_array().cloned().unwrap_or_default();
    if g_cases.len() != r_cases.len() {
        errs.push(format!(
            "Tamming cases 数不等：golden={} rust={}",
            g_cases.len(),
            r_cases.len()
        ));
        return errs;
    }
    for (gc, rc) in g_cases.iter().zip(r_cases.iter()) {
        let label = text(gc, "Label");
        let gb = &gc["Tamming"];
        let rb = &rc["Tamming"];
        if !gb.is_object() || !rb.is_object() {
            errs.push(format!("[{label}] Tamming 块缺失"));
            continue;
        }
        for k in ["spell_return", "has_640"] {
            if gb.get(k).and_then(Value::as_bool) != rb.get(k).and_then(Value::as_bool) {
                errs.push(format!(
                    "[{label}] Tamming.{k} 不等：golden={:?} rust={:?}",
                    gb.get(k),
                    rb.get(k)
                ));
            }
        }
        for k in ["slave_list_rel", "slave_count_after"] {
            if gb.get(k) != rb.get(k) {
                errs.push(format!(
                    "[{label}] Tamming.{k} 不等：golden={:?} rust={:?}",
                    gb.get(k),
                    rb.get(k)
                ));
            }
        }
        // 60005 投递（列表逐项）。
        let gr = gb["rm_makeholyseize"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let rr = rb["rm_makeholyseize"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        if gr.len() != rr.len() {
            errs.push(format!(
                "[{label}] rm_makeholyseize 条数不等：golden={} rust={}",
                gr.len(),
                rr.len()
            ));
        } else {
            for (j, (a, b)) in gr.iter().zip(rr.iter()).enumerate() {
                for k in ["who", "w_param"] {
                    if a.get(k) != b.get(k) {
                        errs.push(format!(
                            "[{label}] rm_makeholyseize[{j}].{k} 不等：golden={:?} rust={:?}",
                            a.get(k),
                            b.get(k)
                        ));
                    }
                }
            }
        }
        // monsters_after：逐字段（**DEV-MG-34 撤豁免**——原「CanReAlive 后效 / MonGen 不建模」
        // 随 `Tamming.monsters_after` 二字段进台账而撤；golden 侧为 oracle 真 `MagTamming`
        // 对真 `MonsterObject` 执行的产物。该白名单只服务 tamming 本 stem；16（holyshield）的
        // 比较器（`real_ledger_verify_labels_opts`）只比帧/收件人、无怪物状态面 ⇒ 无「16 侧
        // 同步」面，留痕）。
        let ga = gb["monsters_after"].as_array().cloned().unwrap_or_default();
        let ra = rb["monsters_after"].as_array().cloned().unwrap_or_default();
        if ga.len() != ra.len() {
            errs.push(format!(
                "[{label}] monsters_after 数不等：golden={} rust={}",
                ga.len(),
                ra.len()
            ));
        } else {
            for (j, (a, b)) in ga.iter().zip(ra.iter()).enumerate() {
                for k in ["name", "master"] {
                    if a.get(k) != b.get(k) {
                        errs.push(format!(
                            "[{label}] monsters_after[{j}].{k} 不等：golden={:?} rust={:?}",
                            a.get(k),
                            b.get(k)
                        ));
                    }
                }
                for k in [
                    "hp",
                    "max_hp",
                    "can_re_alive",
                    "mongen_active",
                    "walk_speed",
                    "next_hit_time",
                    "slave_make_level",
                    "crazy_mode",
                    "crazy_interval_ms",
                ] {
                    if a.get(k) != b.get(k) {
                        errs.push(format!(
                            "[{label}] monsters_after[{j}].{k} 不等：golden={:?} rust={:?}",
                            a.get(k),
                            b.get(k)
                        ));
                    }
                }
            }
        }
        // 推导面（两侧各自）：60005 公式、分支后效自洽。
        let lvl = num(&gc["Fixture"], "user_magic_level");
        let want_w = (lvl * 5 + 10) * 1000;
        for (side, c) in [("golden", gc), ("rust", rc)] {
            let b = &c["Tamming"];
            let holy = b["rm_makeholyseize"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            for e in &holy {
                if num(e, "w_param") != want_w {
                    errs.push(format!(
                        "[{side}/{label}] 60005 w_param={} ≠ (L*5+10)*1000={want_w}",
                        num(e, "w_param")
                    ));
                }
            }
            let slave = b["slave_list_rel"].as_array().cloned().unwrap_or_default();
            let mon = &b["monsters_after"][0];
            let master = text(mon, "master");
            if !slave.is_empty() {
                if master != "caster" {
                    errs.push(format!("[{side}/{label}] 有 SlaveList 但 master≠caster"));
                }
                if num(mon, "walk_speed") != 1500 || num(mon, "next_hit_time") != 2000 {
                    errs.push(format!(
                        "[{side}/{label}] 受奴后钳制值应为 1500/2000，实得 {}/{}",
                        num(mon, "walk_speed"),
                        num(mon, "next_hit_time")
                    ));
                }
            }
            if b.get("has_640").and_then(Value::as_bool) != Some(true)
                && label != "outer_fail_false"
            {
                errs.push(format!("[{side}/{label}] 非 outer_fail_false 支必须有 640"));
            }
            if label == "outer_fail_false"
                && b.get("has_640").and_then(Value::as_bool) != Some(false)
            {
                errs.push(format!("[{side}/{label}] Result=false ⇒ 不得有 640"));
            }
        }
    }
    errs
}

/// **MG-34（20 诱惑之光）** 真实台账 → 判定面（七支）。
///
/// 判据分层：驱动只记录（输入全从 golden 读、配置四键构造 `PetSlaveConfig`）；比较器做
/// `verify` 通用面 + `Tamming` 块逐字段（建模面）+ 推导面（60005 公式 `(L*5+10)*1000`、
/// 受奴后 `Master=caster`/`SlaveList`/WalkSpeed/NextHitTime 钳制、`Result=false ⇒ 无 640`）。
/// **DEV-MG-34 撤豁免**：原「不比较 CanReAlive/MonGen」随台账 `monsters_after` 二字段
/// （`can_re_alive`/`mongen_active`）与 golden `adopt_zero` 臂而撤；golden 侧为 oracle 真
/// `MagTamming` 对真 `MonsterObject` 执行的产物（RecordingMonster 继承），非手工期望。
#[test]
#[ignore = "需先跑 game-svr 驱动（tests::tamming_parity_*）生成真实台账"]
fn real_rust_ledger_tamming() {
    let stem = "tamming";
    let labels = [
        "self_master",
        "adopt_ok",
        "level_gate_fail",
        "branch_else",
        "outer_fail_true",
        "outer_fail_false",
        "crazy",
        "adopt_zero",
    ];
    let golden = golden_file("tamming.json");
    for label in labels {
        check_golden_contract(&golden, label).unwrap_or_else(|m| panic!("{m}"));
    }
    let dir = ledger_dir();
    let want = std::env::var("MIR2_LEDGER_RUN_ID").unwrap_or_default();
    assert!(
        !want.is_empty(),
        "未设置 MIR2_LEDGER_RUN_ID：比较器必须与驱动用同一 id（卡 §28 空转防线）"
    );
    let mut cases = Vec::new();
    for label in labels {
        let path = dir.join(format!("{stem}_{label}.json"));
        let txt = std::fs::read_to_string(&path).unwrap_or_else(|e| {
            panic!(
                "防空转：缺真实台账 {}（{e}）。先跑驱动 tests::tamming_parity_*",
                path.display()
            )
        });
        let side = path.with_extension("run.json");
        let side_text = std::fs::read_to_string(&side).unwrap_or_else(|e| {
            panic!(
                "防空转：台账缺 run.json 侧车 {}（{e}）——多见于「驱动未跑/编译失败，读到上一次的陈旧台账」",
                side.display()
            )
        });
        let side_v: Value = serde_json::from_str(&side_text).expect("run.json 合法 JSON");
        assert_eq!(
            side_v.get("run_id").and_then(Value::as_str),
            Some(want.as_str()),
            "[{label}] 台账陈旧：run_id 不匹配"
        );
        let v: Value = serde_json::from_str(&txt).expect("台账合法 JSON");
        let arr = v.as_array().expect("台账是数组");
        assert_eq!(arr.len(), 1, "[{label}] 每支台账恰 1 case");
        assert_eq!(arr[0].get("Label").and_then(Value::as_str), Some(label));
        let pinned = pin_union_rng(&arr[0], label).unwrap_or_else(|m| panic!("{m}"));
        cases.push(pinned);
    }
    let g = golden.clone();
    let r = serde_json::json!({
        "magic_id": golden.get("magic_id").cloned().unwrap_or(Value::from(0)),
        "cases": cases,
    });
    let mut errs = tamming_block_checks(&g, &r);
    if let Err(mut e) = verify(&g, &r) {
        errs.append(&mut e);
    }
    assert!(
        errs.is_empty(),
        "诱惑台账与金标准不等（{} 条）：\n{}",
        errs.len(),
        errs.join("\n")
    );
}

/// **`36`（`SKILL_UENHANCER`）**真实台账 → 判定面（三支 s1/s5/s13，卡 openmir2-uenhancer）。
///
/// scope：只证 DoSpell 时刻 `ExtraAbil[0]/Times[0]`（`CasterExtraAbil0`）与帧；施法者 `Abil`
/// 已按登录序对齐。SM 严格序 + RM 多重集 + `CasterExtraAbil0`。
#[test]
#[ignore = "需先跑 game-svr 三支驱动（tests::uenhancer_parity_*）生成真实台账"]
fn real_rust_ledger_uenhancer() {
    // **MG-59（2026-11-28，① 档）**：`friend_target`（U-1）/`amulet_off_fail`（U-2 639 失败面）。
    real_ledger_verify_labels(
        "uenhancer",
        &golden_file("uenhancer.json"),
        &["s1", "s5", "s13", "friend_target", "amulet_off_fail"],
    );
}

/// **`MG-23 holyshield(16)`** 真实台账 → 判定面。四支：两条门禁负例 + 部分命中 + 全命中
/// （帧集合退化必须与金标准逐条一致）。驱动：`tests::holyshield_parity_*`（§16c 需各自独立进程）。
#[test]
#[ignore = "需先跑 game-svr 四支驱动（tests::holyshield_parity_*）生成真实台账"]
fn real_rust_ledger_holyshield() {
    // 该 stem 的金标准含**零帧收件人**（`mon0`/`mon1`）；台账只登记有帧的收件人 ⇒
    // 两侧都删空表再比（见 `strip_empty_receivers`；非空帧仍逐帧严格比较）。
    real_ledger_verify_labels_opts(
        "holyshield",
        &golden_file("holyshield.json"),
        &[
            "mon_hit_two",
            "mon_blocked_level",
            "mon_blocked_master",
            "mon_partial",
            // **MG-23（2026-11-28，① 档收尾）**：caster_level=210 ⇒ 等级比较面幅度足够
            // （draw+209 > 100 恒真 ⇒ level=100 怪中盾 ⇒ result=1 ⇒ 训练尾 640 在——台账可见面）。
            "mon_high_caster_level_hit",
        ],
        true,
    );
}

/// **`MG-49 grouplightening(42)`** 真实台账 → 判定面（三 seed）。target 侧含
/// **53 = `SM_HEALTHSPELLCHANGED`**：驱动内钉 `MIR2_CLIENT_ABILITY_LAYOUT=0` ⇒ 发的是 oracle
/// header-only 12B（不带 16B 客户端尾）⇒ 全帧比较在这里是对的（生产 client 布局带尾，已登记偏差）。
#[test]
#[ignore = "需先跑 game-svr 三支驱动（tests::grouplightening_parity_s*）生成真实台账"]
fn real_rust_ledger_grouplightening() {
    real_ledger_verify_labels(
        "grouplightening",
        &golden_file("grouplightening.json"),
        &["s1", "s5", "s13"],
    );
}

/// **`MG-50 groupamyounsul(48)`** 真实台账 → 判定面（三 seed，逐目标护身符 + 毒伤）。
#[test]
#[ignore = "需先跑 game-svr 三支驱动（tests::groupamyounsul_parity_s*）生成真实台账"]
fn real_rust_ledger_groupamyounsul() {
    // **MG-50**：标签从 golden 取（s1/s5/s13 + lvl1/lvl4 + 三个 multi_amulet 臂）；
    // 缺台账即逐标签读盘失败 ⇒ 防空转。
    let golden = golden_file("groupamyounsul.json");
    let labels = golden_labels(&golden, "groupamyounsul");
    real_ledger_verify_labels("groupamyounsul", &golden, &labels);
}

/// **`MG-50` shape0 臂**（`AmuletShape=0`）：`switch` 无匹配分支 ⇒ 不投毒（零 `8037`），
/// 但 `CheckAmulet`/`UseAmulet` 与 `result` 在 `switch` **之外** ⇒ 护身符照扣、训练尾 `640` 仍在。
#[test]
#[ignore = "需先跑 game-svr 驱动（tests::group_amyounsul_shape0_parity_*）生成真实台账"]
fn real_rust_ledger_groupamyounsul_shape0() {
    let golden = golden_file("groupamyounsul-shape0.json");
    let labels = golden_labels(&golden, "groupamyounsul-shape0");
    real_ledger_verify_labels("groupamyounsul-shape0", &golden, &labels);
}
/// **`MG-65`（2026-11-27）**：Shape=2 stem 的比较器（含 `prebuff_red` 红毒位号臂）；
/// 目标是玩家 ⇒ `10139`/`10125` 两侧同形，无需 A/B/C 政策。
#[test]
#[ignore = "需先跑 game-svr 驱动（tests::group_amyounsul_shape2_parity_*）生成真实台账"]
fn real_rust_ledger_groupamyounsul_shape2() {
    let golden = golden_file("groupamyounsul-shape2.json");
    let labels = golden_labels(&golden, "groupamyounsul-shape2");
    real_ledger_verify_labels("groupamyounsul-shape2", &golden, &labels);
}

/// **`MG-16` 抗拒火环(8)** 真实台账 → 判定面。装置侧前提：驱动为 8/37 补了可走网格 +
/// `caster.visible_actors` 回填（否则 `MagPushArround` 循环零次）。退化守卫在驱动侧
/// （`SM_BACKSTEP(9)` 两侧各 ≥1）；本比较器只做台账 ↔ golden 的逐帧比对。
#[test]
#[ignore = "需先跑 game-svr 三支驱动（tests::firewind_parity_s*）生成真实台账"]
fn real_rust_ledger_firewind() {
    // **MG-57 P2**：标签从 golden 取（8 向 / 对角 2 格 / 多目标 / 被挡两施法 / 收件人边界 4 档）。
    let golden = golden_file("firewind.json");
    let labels = golden_labels(&golden, "firewind");
    real_ledger_verify_labels("firewind", &golden, &labels);
}

/// **`MG-35` 瞬息移动(21)**：驱动侧 `spacemove_rig_inputs` 读真图（`MAGICGOLDEN_MAP_DIR`）并核
/// `map_file_size`；落点由 `SpaceMoveGetRandXY` 的移位环决定，故必须真格面（合成全 `Walk` 会少抽）。
#[test]
#[ignore = "需先跑 game-svr 三支驱动（tests::spacemove_parity_s*）生成真实台账"]
fn real_rust_ledger_spacemove() {
    real_ledger_verify_labels(
        "spacemove",
        &golden_file("spacemove.json"),
        &["s1", "s5", "s13"],
    );
}

/// **`MG-57` 气功波(37)**：与 8 在 oracle 里函数体逐字相同（同 `MagPushArround`）。
#[test]
#[ignore = "需先跑 game-svr 三支驱动（tests::energyrepulsor_parity_s*）生成真实台账"]
fn real_rust_ledger_energyrepulsor() {
    real_ledger_verify_labels(
        "energyrepulsor",
        &golden_file("energyrepulsor.json"),
        &["s1", "s5", "s13"],
    );
}

/// **`17 骷髅`（`SKILL_SKELLETON`）**真实台账 → 判定面（两例 summon_ok / cap_hit，卡 17）。
///
/// 对拍范围（卡 17 SCOPE）：`Skeleton` block（`regen_calls` / `slave_count_before/after` /
/// `pre_slave_count` / `new_slaves{name,level,hp,max_hp,master_is_caster}` / `stub_*`）+
/// 造怪前帧 17/638/642（按 socket 取多重集，§47 不跨 socket 断言序）。**不比** `Rng`：
/// 造怪抽数已记录在 golden `Rng` 中且可复现（`[1500,2000,3000,3000,2000]` + `Random(3)`），
/// 而本 stem 的驱动不做 RNG 记账（台账无 `Rng` 键）——不是「抽 0 次」；`640`/`10123-p3`
/// 随造怪抽数不同。`cap_hit` 另三断各自独立：实体数不增 / slave_list 不变 /
/// spawn 0 次（`regen_calls=0`）。**656 未验（装置盲区，`docs/issues/golden-harness-blind-spots.md` B1）**。
#[test]
#[ignore = "需先跑 game-svr 两支驱动（tests::skeleton_parity_*）生成真实台账"]
fn real_rust_ledger_skeleton() {
    let dir = ledger_dir();
    let golden = golden_file("skeleton.json");
    let want = std::env::var("MIR2_LEDGER_RUN_ID").unwrap_or_default();
    assert!(
        !want.is_empty(),
        "未设置 MIR2_LEDGER_RUN_ID：比较器必须与驱动用同一 id（卡 §28 空转防线）"
    );

    for label in ["summon_ok", "cap_hit"] {
        let path = dir.join(format!("skeleton_{label}.json"));
        let text = std::fs::read_to_string(&path).unwrap_or_else(|e| {
            panic!(
                "防空转：缺真实台账 {}（{e}）。先跑 game-svr 驱动 tests::skeleton_parity_{label}",
                path.display()
            )
        });
        let side = path.with_extension("run.json");
        let side_text = std::fs::read_to_string(&side).unwrap_or_else(|e| {
            panic!(
                "[{label}] 台账缺 run.json 侧车 {}（{e}）——多见于「驱动未跑/编译失败，读到陈旧台账」",
                side.display()
            )
        });
        let side_v: Value = serde_json::from_str(&side_text).expect("run.json 必须合法 JSON");
        assert_eq!(
            side_v.get("run_id").and_then(Value::as_str),
            Some(want.as_str()),
            "[{label}] 台账陈旧：run_id 不匹配（当前={want}）；用同一 id 重跑驱动"
        );
        let v: Value = serde_json::from_str(&text).expect("台账必须合法 JSON");
        let arr = v.as_array().expect("台账必须是 [case] 数组");
        assert_eq!(arr.len(), 1, "[{label}] 每支台账恰 1 个 case");
        let rust = &arr[0];
        assert_eq!(
            rust.get("Label").and_then(Value::as_str),
            Some(label),
            "[{label}] 台账 Label 不符"
        );

        let gcase = golden["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["Label"] == label)
            .unwrap_or_else(|| panic!("金标准缺 {label} 支"));
        let gskel = &gcase["Skeleton"];
        let rskel = &rust["Skeleton"];

        // ① cap_hit 三断（先验语义层：实体数 / slave_list / spawn 次数；各自独立断言）。
        //    放在 block 标量之前 ⇒ ②③ 变异在语义层先红，不被标量差抢先。
        if label == "cap_hit" {
            let eb = rust["world_entities_before"].as_u64();
            let ea = rust["world_entities_after"].as_u64();
            assert_eq!(
                ea, eb,
                "[cap_hit 断①/实体数不增] 施法前后实体数变化：before={eb:?} after={ea:?}"
            );
            assert_eq!(
                rskel["slave_count_after"], rskel["slave_count_before"],
                "[cap_hit 断②/slave_list 不变] slave_count 变化：before={} after={}",
                rskel["slave_count_before"], rskel["slave_count_after"]
            );
            assert_eq!(
                rskel["new_slaves"].as_array().map_or(usize::MAX, Vec::len),
                0,
                "[cap_hit 断②/slave_list 不变] new_slaves 应为空"
            );
            assert_eq!(
                rskel["regen_calls"].as_u64(),
                Some(0),
                "[cap_hit 断③/spawn 0 次] regen_calls 应为 0（门 Rejected）"
            );
        }

        // ② Skeleton block 标量逐字段 + new_slaves（①的删半补牙落此层：new_slaves[].hp 实值）。
        for key in [
            "regen_calls",
            "slave_count_before",
            "slave_count_after",
            "pre_slave_count",
            "stub_wabil_hp",
            "stub_abil_max_hp",
            "stub_monster_race",
        ] {
            assert_eq!(
                rskel.get(key),
                gskel.get(key),
                "[{label}] Skeleton.{key} 不等"
            );
        }
        // new_slaves 逐项。
        let g_new = gskel["new_slaves"].as_array().unwrap();
        let r_new = rskel["new_slaves"].as_array().unwrap();
        assert_eq!(
            r_new.len(),
            g_new.len(),
            "[{label}] new_slaves 条数不等：Rust {} vs golden {}",
            r_new.len(),
            g_new.len()
        );
        for (i, (g, r)) in g_new.iter().zip(r_new.iter()).enumerate() {
            for key in ["name", "level", "hp", "max_hp", "master_is_caster"] {
                assert_eq!(
                    r.get(key),
                    g.get(key),
                    "[{label}] new_slaves[{i}].{key} 不等"
                );
            }
        }

        // ③ 造怪前帧 17/638/642（按 socket 多重集）。
        let keep: &[i64] = &[17, 638, 642];
        for who in ["caster", "target", "obs_in", "obs_out"] {
            let g_sent = sent_filter(gcase, who, keep);
            let r_sent = sent_filter(rust, who, keep);
            assert_eq!(
                r_sent, g_sent,
                "[{label}] {who} 的 17/638/642 帧不一致：Rust {r_sent:?} vs golden {g_sent:?}"
            );
        }

        // ④ 训练尾 640（本卡起自延迟 1000ms）：payload 实测**部分可比**——`(recog,param,series,due)`
        // 两侧一致；`tag`(=TranPoint) 不同（golden=2 / Rust=1，根因：两侧造怪抽数的记账/消费
        // 序列不同 ⇒ RNG 流错位）⇒ 本断言**不比 `tag`**（先实测后入比较面，不凭假设）。
        let g640 = frames_640_caster(gcase);
        let r640 = frames_640_caster(rust);
        let want640 = usize::from(label == "summon_ok");
        assert_eq!(
            r640.len(),
            want640,
            "[{label}] Rust 640 条数（自延迟后应在交付批）"
        );
        assert_eq!(g640.len(), want640, "[{label}] golden 640 条数");
        assert_eq!(
            r640, g640,
            "[{label}] 640 (recog,param,series,due) 不一致：Rust {r640:?} vs golden {g640:?}"
        );

        // 声明的台账形态：5 ctor + 3 装置差（`draw_at_monster` 1500 / 朝向 8 / cool-eye 100）
        // + 1 训练尾。形态变了（抽数或界变）⇒ 红：必须显式重声明，不许静默漂移。
        const SKELETON_RUST_BOUNDS: [i64; 9] = [1500, 2000, 3000, 3000, 2000, 1500, 8, 100, 3];
        // ⑤ RNG：与 MG-41（30 神兽）/ MG-42（41 精灵）共用同一份判据。
        let rng_errs = verify_slave_family_rng(label, gcase, rust, &SKELETON_RUST_BOUNDS);
        assert!(
            rng_errs.is_empty(),
            "[{label}] RNG 判据红（{} 条）：\n{}",
            rng_errs.len(),
            rng_errs.join("\n")
        );

        eprintln!("[skeleton {label}] PASS");
    }
}

/// 41 精灵的台账抽样形态（精灵 race=100，与 17 骷髅同形的派生 ctor 路径）：5 ctor +
/// 3 装置差（`draw_at_monster` 1500 / 朝向 8 / cool-eye 100）+ 1 训练尾。
/// 形态变了（抽数或界变）⇒ 红：必须显式重声明，不许静默漂移。
const ANGEL_RUST_BOUNDS: &[i64] = &[1500, 2000, 3000, 3000, 2000, 1500, 8, 100, 3];

/// **`MG-42（41 SKILL_ANGEL）`**真实台账 → 判定面（两例 summon_ok / cap_hit）。
///
/// **SCOPE（stub + 生产数据缺失）**：`Angel` block（`regen_calls` / `slave_count_before/after` /
/// `pre_slave_count` / `new_slaves{name,level,hp,max_hp,master_is_caster}` / `stub_*`）+
/// 造怪前帧 41/638/642（按 socket 取多重集，§47 不跨 socket 断言序）。
/// `Rng` **已纳入比较面（2026-11-26，与 17 骷髅 / 30 神兽共用 `verify_slave_family_rng`）**：
/// `cap_hit` 两侧 0 抽逐值；`summon_ok` 比造怪 ctor 5 抽逐值，台账抽样形态由 `RngBounds`
/// 按 `ANGEL_RUST_BOUNDS` 钉住；装置差抽（golden 离线替身不存在的派生 ctor/初始化抽）
/// 不进值面，登记 `DEV-MG-24-RNGCOUNT`。`640`/`10123-p3` 随造怪抽数不同。`cap_hit` 另三断各自独立：实体数不增 / slave_list 不变 /
/// spawn 0 次（`regen_calls=0`）。**656 未验（装置盲区，`docs/issues/golden-harness-blind-spots.md` B1）**。
/// **本夹具证明「给定怪物之后的 adopt 行为」，不证明生产可召唤**（生产 `monsters` 无 `精灵`
/// 行 ⇒ `MakeSlave` 恒 null；卡 `openmir2-angel-summon-monster-missing`）。
#[test]
#[ignore = "需先跑 game-svr 两支驱动（tests::angel_parity_*）生成真实台账"]
fn real_rust_ledger_angel() {
    let dir = ledger_dir();
    let golden = golden_file("angel.json");
    let want = std::env::var("MIR2_LEDGER_RUN_ID").unwrap_or_default();
    assert!(
        !want.is_empty(),
        "未设置 MIR2_LEDGER_RUN_ID：比较器必须与驱动用同一 id（卡 §28 空转防线）"
    );

    for label in ["summon_ok", "cap_hit"] {
        let path = dir.join(format!("angel_{label}.json"));
        let text = std::fs::read_to_string(&path).unwrap_or_else(|e| {
            panic!(
                "防空转：缺真实台账 {}（{e}）。先跑 game-svr 驱动 tests::angel_parity_{label}",
                path.display()
            )
        });
        let side = path.with_extension("run.json");
        let side_text = std::fs::read_to_string(&side).unwrap_or_else(|e| {
            panic!(
                "[{label}] 台账缺 run.json 侧车 {}（{e}）——多见于「驱动未跑/编译失败，读到陈旧台账」",
                side.display()
            )
        });
        let side_v: Value = serde_json::from_str(&side_text).expect("run.json 必须合法 JSON");
        assert_eq!(
            side_v.get("run_id").and_then(Value::as_str),
            Some(want.as_str()),
            "[{label}] 台账陈旧：run_id 不匹配（当前={want}）；用同一 id 重跑驱动"
        );
        let v: Value = serde_json::from_str(&text).expect("台账必须合法 JSON");
        let arr = v.as_array().expect("台账必须是 [case] 数组");
        assert_eq!(arr.len(), 1, "[{label}] 每支台账恰 1 个 case");
        let rust = &arr[0];
        assert_eq!(
            rust.get("Label").and_then(Value::as_str),
            Some(label),
            "[{label}] 台账 Label 不符"
        );

        let gcase = golden["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["Label"] == label)
            .unwrap_or_else(|| panic!("金标准缺 {label} 支"));
        let gskel = &gcase["Angel"];
        let rskel = &rust["Angel"];

        // ① cap_hit 三断（先验语义层：实体数 / slave_list / spawn 次数；各自独立断言）。
        //    放在 block 标量之前 ⇒ ②③ 变异在语义层先红，不被标量差抢先。
        if label == "cap_hit" {
            let eb = rust["world_entities_before"].as_u64();
            let ea = rust["world_entities_after"].as_u64();
            assert_eq!(
                ea, eb,
                "[cap_hit 断①/实体数不增] 施法前后实体数变化：before={eb:?} after={ea:?}"
            );
            assert_eq!(
                rskel["slave_count_after"], rskel["slave_count_before"],
                "[cap_hit 断②/slave_list 不变] slave_count 变化：before={} after={}",
                rskel["slave_count_before"], rskel["slave_count_after"]
            );
            assert_eq!(
                rskel["new_slaves"].as_array().map_or(usize::MAX, Vec::len),
                0,
                "[cap_hit 断②/slave_list 不变] new_slaves 应为空"
            );
            assert_eq!(
                rskel["regen_calls"].as_u64(),
                Some(0),
                "[cap_hit 断③/spawn 0 次] regen_calls 应为 0（门 Rejected）"
            );
        }

        // ② Skeleton block 标量逐字段 + new_slaves（①的删半补牙落此层：new_slaves[].hp 实值）。
        for key in [
            "regen_calls",
            "slave_count_before",
            "slave_count_after",
            "pre_slave_count",
            "stub_wabil_hp",
            "stub_abil_max_hp",
            "stub_monster_race",
        ] {
            assert_eq!(rskel.get(key), gskel.get(key), "[{label}] Angel.{key} 不等");
        }
        // new_slaves 逐项。
        let g_new = gskel["new_slaves"].as_array().unwrap();
        let r_new = rskel["new_slaves"].as_array().unwrap();
        assert_eq!(
            r_new.len(),
            g_new.len(),
            "[{label}] new_slaves 条数不等：Rust {} vs golden {}",
            r_new.len(),
            g_new.len()
        );
        for (i, (g, r)) in g_new.iter().zip(r_new.iter()).enumerate() {
            for key in ["name", "level", "hp", "max_hp", "master_is_caster"] {
                assert_eq!(
                    r.get(key),
                    g.get(key),
                    "[{label}] new_slaves[{i}].{key} 不等"
                );
            }
        }

        // ③ 造怪前帧 17/638/642（按 socket 多重集）。
        let keep: &[i64] = &[17, 638, 642];
        for who in ["caster", "target", "obs_in", "obs_out"] {
            let g_sent = sent_filter(gcase, who, keep);
            let r_sent = sent_filter(rust, who, keep);
            assert_eq!(
                r_sent, g_sent,
                "[{label}] {who} 的 17/638/642 帧不一致：Rust {r_sent:?} vs golden {g_sent:?}"
            );
        }

        // ④ 训练尾 640（本卡起自延迟 1000ms）：payload 实测**部分可比**——`(recog,param,series,due)`
        // 两侧一致；`tag`(=TranPoint) 不同（golden=2 / Rust=1，根因：两侧造怪抽数的记账/消费
        // 序列不同 ⇒ RNG 流错位）⇒ 本断言**不比 `tag`**（先实测后入比较面，不凭假设）。
        let g640 = frames_640_caster(gcase);
        let r640 = frames_640_caster(rust);
        let want640 = usize::from(label == "summon_ok");
        assert_eq!(
            r640.len(),
            want640,
            "[{label}] Rust 640 条数（自延迟后应在交付批）"
        );
        assert_eq!(g640.len(), want640, "[{label}] golden 640 条数");
        assert_eq!(
            r640, g640,
            "[{label}] 640 (recog,param,series,due) 不一致：Rust {r640:?} vs golden {g640:?}"
        );

        // RNG：与 17 骷髅 / 30 神兽共用同一份判据（错位口径见 verify_slave_family_rng）。
        let rng_errs = verify_slave_family_rng(label, gcase, rust, ANGEL_RUST_BOUNDS);
        assert!(
            rng_errs.is_empty(),
            "[{label}] RNG 判据红（{} 条）：\n{}",
            rng_errs.len(),
            rng_errs.join("\n")
        );

        eprintln!("[angel {label}] PASS");
    }
}

/// 取某接收者 `Sent` 中 `ident ∈ keep` 的帧，按 `(ident,param,tag,series,tail)` 多重集排序。
fn sent_filter(case: &Value, who: &str, keep: &[i64]) -> Vec<(i64, i64, i64, i64, String)> {
    let Some(recv) = case["Receivers"].as_array() else {
        return Vec::new();
    };
    let Some(r) = recv.iter().find(|r| r["Who"] == who) else {
        return Vec::new();
    };
    let Some(sent) = r["Sent"].as_array() else {
        return Vec::new();
    };
    let mut out: Vec<(i64, i64, i64, i64, String)> = sent
        .iter()
        .filter(|s| keep.contains(&s["ident"].as_i64().unwrap_or(i64::MIN)))
        .map(|s| {
            (
                s["ident"].as_i64().unwrap_or(i64::MIN),
                s["param"].as_i64().unwrap_or(i64::MIN),
                s["tag"].as_i64().unwrap_or(i64::MIN),
                s["series"].as_i64().unwrap_or(i64::MIN),
                s["tail"].as_str().unwrap_or("").to_string(),
            )
        })
        .collect();
    out.sort();
    out
}

/// SINSU 用：`(ident,param,tag,series,recog,due_ms_rel,tail)` 多重集（含 golden 新键 `recog`/`due_ms_rel`；C# send 带 recog、施法批 due_ms_rel=0）。
fn sent_filter_sinsu(
    case: &Value,
    who: &str,
    keep: &[i64],
) -> Vec<(i64, i64, i64, i64, i64, i64, String)> {
    let Some(recv) = case["Receivers"].as_array() else {
        return Vec::new();
    };
    let Some(r) = recv.iter().find(|r| r["Who"] == who) else {
        return Vec::new();
    };
    let Some(sent) = r["Sent"].as_array() else {
        return Vec::new();
    };
    let mut out: Vec<(i64, i64, i64, i64, i64, i64, String)> = sent
        .iter()
        .filter(|s| keep.contains(&s["ident"].as_i64().unwrap_or(i64::MIN)))
        .map(|s| {
            (
                s["ident"].as_i64().unwrap_or(i64::MIN),
                s["param"].as_i64().unwrap_or(i64::MIN),
                s["tag"].as_i64().unwrap_or(i64::MIN),
                s["series"].as_i64().unwrap_or(i64::MIN),
                s["recog"].as_i64().unwrap_or(i64::MIN),
                s["due_ms_rel"].as_i64().unwrap_or(i64::MIN),
                s["tail"].as_str().unwrap_or("").to_string(),
            )
        })
        .collect();
    out.sort();
    out
}

/// 取某 case 某 ident 的全部帧 `(who,recog,param,tag,series)`（用于 807 回召帧的不对称断言）。
fn frames_by_ident(case: &Value, ident: i64) -> Vec<(String, i64, i64, i64, i64)> {
    let mut out = Vec::new();
    if let Some(recv) = case["Receivers"].as_array() {
        for r in recv {
            let who = r["Who"].as_str().unwrap_or("").to_string();
            if let Some(sent) = r["Sent"].as_array() {
                for s in sent {
                    if s["ident"].as_i64() == Some(ident) {
                        out.push((
                            who.clone(),
                            s["recog"].as_i64().unwrap_or(i64::MIN),
                            s["param"].as_i64().unwrap_or(i64::MIN),
                            s["tag"].as_i64().unwrap_or(i64::MIN),
                            s["series"].as_i64().unwrap_or(i64::MIN),
                        ));
                    }
                }
            }
        }
    }
    out
}

/// 取某 case 中 **caster** 的 640 帧 `(recog, param, series, due_ms_rel)`——**不含 `tag`**。
/// 依据：卡 17/MG-41 SCOPE 的「造怪抽数的记账/消费序列不同 ⇒ RNG 流错位」⇒ 640 的 `tag`(=TranPoint)
/// 实测 golden=2 / Rust=1（本 head；世代或夹具变动后需重测），**不同源**；其余字段同源可比。
fn frames_640_caster(case: &Value) -> Vec<(i64, i64, i64, i64)> {
    let mut out = Vec::new();
    if let Some(recv) = case["Receivers"].as_array() {
        for r in recv {
            if r["Who"].as_str() != Some("caster") {
                continue;
            }
            if let Some(sent) = r["Sent"].as_array() {
                for s in sent {
                    if s["ident"].as_i64() == Some(640) {
                        out.push((
                            s["recog"].as_i64().unwrap_or(i64::MIN),
                            s["param"].as_i64().unwrap_or(i64::MIN),
                            s["series"].as_i64().unwrap_or(i64::MIN),
                            s["due_ms_rel"].as_i64().unwrap_or(i64::MIN),
                        ));
                    }
                }
            }
        }
    }
    out.sort();
    out
}

/// 30 神兽的台账抽样形态：5 ctor + 2 装置差（朝向 8 / cool-eye 100）+ 1 训练尾。
/// 形态变了（抽数或界变）⇒ 红：必须显式重声明，不许静默漂移。
const SINSU_RUST_BOUNDS: &[i64] = &[1500, 2000, 3000, 3000, 2000, 8, 100, 3];

/// **`30 SINSU`（`SKILL_SINSU`，神兽）**真实台账 → 判定面（两例 `summon_ok` + `cap_hit`；卡 MG-41）。
///
/// 断言集（按 opc-19 最小集）：`slave_count_before=0→after=1` + `regen_calls=1`（走过一次 RegenMonsterByName）；
/// `new_slaves[0]` 逐字段 `name/level/hp/max_hp/master_is_caster`（hp=20/max=24 是 0.3 支特征值）；
/// 帧：17/638=施法帧、642=`SM_DURACHANGE`(护身符耐久)、807=回召帧(未建模、cap_hit 显式登记)；本函数比 17/638/642 的 socket 多重集；`stub_monster_race=113`（输入记录）。
/// `Rng` **已纳入比较面（2026-11-26，与 17 骷髅 / 41 精灵共用 `verify_slave_family_rng`）**：`cap_hit` 两侧 0 抽逐值；`summon_ok` 比造怪 ctor 5 抽逐值，台账抽样形态 8 抽 = 5 ctor + 2 装置差（朝向 `Random(8)`、cool-eye `Random(100)`）+ 1 训练尾，由 `RngBounds=[1500,2000,3000,3000,2000,8,100,3]` 钉住；装置差 2 抽不进值面（`DEV-MG-24-RNGCOUNT`——神兽 race=113 不走 `draw_at_monster`，故比骷髅少那一抽）。
///
/// `keep` 不含 640——有凭据、非窗口原因：640=`SM_MAGIC_LVEXP`（训练尾），其 `tag`（= 训练 `Random(3)+1`）实测 golden=`2` vs rust=`3`，因 Rust `create_monster_oracle` 在尾抽前多消耗造怪抽、与 C# 替身的记账/消费序列不同而错位。**640 不是 drain 窗口没到，是 RNG-payload 偏移（tag golden 2 vs rust 3），根因同 Rng（造怪抽数的记账/消费序列不同 ⇒ RNG 流错位）**。638/642/17 三帧 golden==rust 逐字段相等（本 case 实测），故 `keep={17,638,642}`。
///
/// `due_ms_rel`：本 leaf 被比较帧（17/638/642）恒为 0（三帧均属**施法批** elapsed=0，驱动无交付回路）——**结构恒定、非伪造**；与两个 stub 键不同：若引入交付批，due 会随 elapsed 变化（stub 17 则永不）。
///
/// `cap_hit` 回召面见 `real_rust_ledger_sinsu_cap_hit`（mapmgr 替身已解锁）。
#[test]
#[ignore = "需先跑 game-svr 驱动 (tests::sinsu_parity_summon_ok) 生成台账"]
fn real_rust_ledger_sinsu() {
    let dir = ledger_dir();
    let golden = golden_file("sinsu.json");
    let want = std::env::var("MIR2_LEDGER_RUN_ID").unwrap_or_default();
    assert!(
        !want.is_empty(),
        "未设置 MIR2_LEDGER_RUN_ID：比较器必须与驱动用同一 id（卡 §28）"
    );
    let label = "summon_ok";
    let path = dir.join(format!("sinsu_{label}.json"));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "防空转：缺真实台账 {}（{e}）。先跑 tests::sinsu_parity_summon_ok",
            path.display()
        )
    });
    let side = path.with_extension("run.json");
    let side_v: Value = serde_json::from_str(
        &std::fs::read_to_string(&side)
            .unwrap_or_else(|e| panic!("[{label}] 台账缺 run.json 侧车（{e}）")),
    )
    .expect("run.json 合法 JSON");
    assert_eq!(
        side_v.get("run_id").and_then(Value::as_str),
        Some(want.as_str()),
        "[{label}] 台账陈旧：run_id 不匹配（当前={want}）"
    );
    let v: Value = serde_json::from_str(&text).expect("台账合法 JSON");
    let arr = v.as_array().expect("[case] 数组");
    assert_eq!(arr.len(), 1, "[{label}] 恰 1 个 case");
    let rust = &arr[0];
    assert_eq!(
        rust.get("Label").and_then(Value::as_str),
        Some(label),
        "[{label}] Label 不符"
    );
    let gcase = golden["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["Label"] == label)
        .unwrap_or_else(|| panic!("金标准缺 {label} 支"));
    let gs = &gcase["SinSu"];
    let rs = &rust["SinSu"];

    // SinSu block 标量逐字段（含输入记录 stub_monster_race）。
    for key in [
        "slave_count_before",
        "slave_count_after",
        "regen_calls",
        "pre_slave_count",
        "stub_monster_race",
        // stub_wabil_hp/stub_abil_max_hp 两侧恒 17（C# Program.cs:2317-2318 / Rust 同值）=> 恒真、非判据，不在此循环比；仅留 golden 作意图记录（drift 卡 §5）。
    ] {
        assert_eq!(rs.get(key), gs.get(key), "[{label}] SinSu.{key} 不等");
    }
    // 显式证「召出来了」：0→1 + regen=1 + race=113（不只依赖 golden 相等）。
    assert_eq!(
        rs["slave_count_before"].as_u64(),
        Some(0),
        "[{label}] slave_count_before 应 0"
    );
    assert_eq!(
        rs["slave_count_after"].as_u64(),
        Some(1),
        "[{label}] slave_count_after 应 1"
    );
    assert_eq!(
        rs["regen_calls"].as_u64(),
        Some(1),
        "[{label}] regen_calls 应 1（走过一次 RegenMonsterByName，非别的分支）"
    );
    assert_eq!(
        rs["stub_monster_race"].as_u64(),
        Some(113),
        "[{label}] stub_monster_race 应 113"
    );
    // new_slaves 逐项（name/level/hp/max_hp/master_is_caster）。
    let g_new = gs["new_slaves"].as_array().unwrap();
    let r_new = rs["new_slaves"].as_array().unwrap();
    assert_eq!(
        r_new.len(),
        g_new.len(),
        "[{label}] new_slaves 条数不等：Rust {} vs golden {}",
        r_new.len(),
        g_new.len()
    );
    for (i, (g, r)) in g_new.iter().zip(r_new.iter()).enumerate() {
        for key in ["name", "level", "hp", "max_hp", "master_is_caster"] {
            assert_eq!(
                r.get(key),
                g.get(key),
                "[{label}] new_slaves[{i}].{key} 不等"
            );
        }
    }
    // 造怪前帧 17/638/642（按 socket 多重集）。
    let keep: &[i64] = &[17, 638, 642];
    for who in ["caster", "target", "obs_in", "obs_out"] {
        let g_sent = sent_filter_sinsu(gcase, who, keep);
        let r_sent = sent_filter_sinsu(rust, who, keep);
        assert_eq!(
            r_sent, g_sent,
            "[{label}] {who} 的 17/638/642 帧不一致（含 recog/due_ms_rel）：Rust {r_sent:?} vs golden {g_sent:?}"
        );
    }
    // 训练尾 640（本卡起自延迟 1000ms）：**只判 `due_ms_rel`、不判 payload**（已裁决：tag 因
    // 造怪抽数的记账/消费序列不同而错位；640 不进 `keep`）。golden caster 恰 1 条、due=1000；Rust 同条数且 due 相等。
    let g640 = frames_640_caster(gcase);
    let r640 = frames_640_caster(rust);
    assert_eq!(g640.len(), 1, "[{label}] golden 640 条数");
    assert_eq!(
        r640.len(),
        1,
        "[{label}] Rust 640 条数（自延迟后应在交付批）"
    );
    assert_eq!(
        r640[0].3, g640[0].3,
        "[{label}] 640 due_ms_rel：Rust={} vs golden={}（golden 侧为 oracle 交付批）",
        r640[0].3, g640[0].3
    );
    // RNG：与 17 骷髅 / 41 精灵共用同一份判据（错位口径见 verify_slave_family_rng）。
    let rng_errs = verify_slave_family_rng(label, gcase, rust, SINSU_RUST_BOUNDS);
    assert!(
        rng_errs.is_empty(),
        "[{label}] RNG 判据红（{} 条）：\n{}",
        rng_errs.len(),
        rng_errs.join("\n")
    );

    eprintln!("[sinsu {label}] PASS");
}

/// **`30 SINSU` cap_hit 回召面**（卡 openmir2-golden-harness-mapmgr）：`RecallSlave→SpaceMove` 让预置 slave 移到施法者面前。
/// 断言：`regen_calls=0`＋`slave_count` 不变＋`new_slaves` 空（门 Rejected）；`recall_moved=true`（与 golden 一致）；
/// **坐标落到施法者面前格**（`GetFrontPosition` 语义，落点由 caster 坐标派生 `(caster_x+1, caster_y)`——caster 面向施法目标(东)）；
/// `before=(0,0) != after`；`pre_slave_xy_before/after` 与 golden 逐项一致；帧 17/638/642（含 recog/due）。
#[test]
#[ignore = "需先跑 game-svr 驱动 (tests::sinsu_parity_cap_hit) 生成台账"]
fn real_rust_ledger_sinsu_cap_hit() {
    let dir = ledger_dir();
    let golden = golden_file("sinsu.json");
    let want = std::env::var("MIR2_LEDGER_RUN_ID").unwrap_or_default();
    assert!(!want.is_empty(), "未设置 MIR2_LEDGER_RUN_ID");
    let label = "cap_hit";
    let path = dir.join("sinsu_cap_hit.json");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "防空转：缺 {}（{e}）先跑 tests::sinsu_parity_cap_hit",
            path.display()
        )
    });
    let side = path.with_extension("run.json");
    let side_v: Value = serde_json::from_str(
        &std::fs::read_to_string(&side).unwrap_or_else(|e| panic!("缺 run.json（{e}）")),
    )
    .expect("json");
    assert_eq!(
        side_v.get("run_id").and_then(Value::as_str),
        Some(want.as_str()),
        "[{label}] run_id 不符"
    );
    let v: Value = serde_json::from_str(&text).expect("台账 json");
    let arr = v.as_array().expect("[case]");
    assert_eq!(arr.len(), 1);
    let rust = &arr[0];
    assert_eq!(rust.get("Label").and_then(Value::as_str), Some(label));
    let gcase = golden["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["Label"] == label)
        .unwrap_or_else(|| panic!("金标准缺 {label}"));
    let gs = &gcase["SinSu"];
    let rs = &rust["SinSu"];
    for key in [
        "slave_count_before",
        "slave_count_after",
        "regen_calls",
        "pre_slave_count",
        "stub_monster_race",
    ] {
        assert_eq!(rs.get(key), gs.get(key), "[{label}] SinSu.{key} 不等");
    }
    // 门 Rejected：regen 0、slave 不变、无新实体。
    assert_eq!(
        rs["regen_calls"].as_u64(),
        Some(0),
        "[{label}] regen_calls 应 0"
    );
    assert_eq!(
        rs["slave_count_before"], rs["slave_count_after"],
        "[{label}] slave_count 应不变"
    );
    assert_eq!(
        rs["new_slaves"].as_array().map_or(usize::MAX, Vec::len),
        0,
        "[{label}] new_slaves 应空"
    );
    // recall_moved 真断言（且与 golden 一致）。
    assert_eq!(
        rs["recall_moved"].as_bool(),
        Some(true),
        "[{label}] recall_moved 应 true（回召确实移动）"
    );
    assert_eq!(
        rs["recall_moved"], gs["recall_moved"],
        "[{label}] recall_moved 与 golden 不一致"
    );
    // 坐标断言：after = 施法者面前（caster 面向施法目标=东 ⇒ (caster_x+1, caster_y)，**由 caster 坐标派生**不抄常数）。
    let cx = rs["caster_x"].as_i64().expect("caster_x");
    let cy = rs["caster_y"].as_i64().expect("caster_y");
    let after = &rs["pre_slave_xy_after"].as_array().expect("after arr")[0];
    let (ax, ay) = (after["x"].as_i64().unwrap(), after["y"].as_i64().unwrap());
    let before = &rs["pre_slave_xy_before"].as_array().expect("before arr")[0];
    let (bx, by) = (before["x"].as_i64().unwrap(), before["y"].as_i64().unwrap());
    assert_eq!(
        (ax, ay),
        (cx + 1, cy),
        "[{label}] 回召落点应由 caster 坐标派生为面前(东) ({}, {})，实得 ({ax}, {ay})",
        cx + 1,
        cy
    );
    assert_ne!(
        (bx, by),
        (ax, ay),
        "[{label}] before 应 ≠ after（否则=空集恒假）"
    );
    assert_eq!(
        rs["pre_slave_xy_before"], gs["pre_slave_xy_before"],
        "[{label}] before 与 golden 不一致"
    );
    assert_eq!(
        rs["pre_slave_xy_after"], gs["pre_slave_xy_after"],
        "[{label}] after 与 golden 不一致"
    );
    // Rng：cap_hit 两侧**同源都 0 抽**（门 fail 无造怪抽 + 落点确定无 RandXY 抽）⇒ **逐值比（与 golden 相等）**。
    // 注意这与 summon_ok 的「不比 Rng」是**两条不同口径**：那条是"两侧造怪抽数的**记账/消费序列不同**所以不比"；
    // 本条是"两侧**同源 0 抽**所以比"。断"==golden"而非"应为空"：任一侧将来多抽一即红。
    assert_eq!(
        rust["Rng"], gcase["Rng"],
        "[{label}] Rng 与 golden 不等（cap_hit 两侧同源 0 抽，逐值比）"
    );

    // **MG-41**：807 = SM_SPACEMOVE_SHOW2 回召帧（已建模，对称逐帧比——807 入 keep 集合）。
    // oracle 锚：`BaseObject.cs:1137`（`SpaceMove` `nInt==1` ⇒ `SendRefMsg(RM_SPACEMOVE_SHOW2,
    // Dir, CurrX, CurrY)`；Dir=奴隶自 Dir=`BaseObject.cs:378` ctor 出生缺省 4）；收件人 =
    // ±SendRefMsgRange（caster/target/obs_in 各 1、obs_out 零=ref 距离语义）。
    // 坐标断言方向：本 case 施法方向 = DIR_RIGHT（cast click=(101,100) → GetNextDirection=东 ⇒ 落点 (cx+1,cy)）；
    // 若 fixture 的 click 改变方向，`(cx+1, cy)` 须改为由 click 派生（勿静默错）。
    // 帧 17/638/642/807（含 recog/due_ms_rel）。
    let keep: &[i64] = &[17, 638, 642, 807];
    // **obs_out 恒零显式断言**（ref 距离语义钉住——不是"恰好没收到"）。
    for (side, case) in [("golden", gcase), ("rust", rust)] {
        assert_eq!(
            frames_by_ident(case, 807)
                .iter()
                .filter(|(w, ..)| w == "obs_out")
                .count(),
            0,
            "[{side}/{label}] obs_out 不得收到 807（ref 距离语义）"
        );
    }
    // 807 对称逐帧：两侧各恰 3（caster/target/obs_in 各 1）且五元组逐项相等。
    for (side, case) in [("golden", gcase), ("rust", rust)] {
        let f807 = frames_by_ident(case, 807);
        assert_eq!(
            f807.len(),
            3,
            "[{side}/{label}] 807 应恰 3 条（SM_SPACEMOVE_SHOW2 回召帧）"
        );
        for awho in ["caster", "target", "obs_in"] {
            assert_eq!(
                f807.iter().filter(|(w, ..)| w == awho).count(),
                1,
                "[{side}/{label}] 807 应在 {awho} 恰 1 条（防 N 条同收件人假绿）"
            );
        }
        for (who, recog, param, tag, series) in &f807 {
            assert_eq!(
                (*recog, *param, *tag, *series),
                (2050, 101, 100, 4),
                "[{side}/{label}] 807 ({who}) 字段应为 (recog=2050,param=101,tag=100,series=4)"
            );
        }
    }
    for who in ["caster", "target", "obs_in", "obs_out"] {
        assert_eq!(
            sent_filter_sinsu(gcase, who, keep),
            sent_filter_sinsu(rust, who, keep),
            "[{label}] {who} 帧不一致"
        );
    }
    // cap_hit：门 Rejected ⇒ 无训练尾 640（两侧均 0）。
    assert_eq!(
        frames_640_caster(gcase).len(),
        0,
        "[{label}] golden 不应有 640"
    );
    assert_eq!(
        frames_640_caster(rust).len(),
        0,
        "[{label}] Rust 不应有 640"
    );
    // RNG：与 17 骷髅 / 41 精灵共用同一份判据（错位口径见 verify_slave_family_rng）。
    let rng_errs = verify_slave_family_rng(label, gcase, rust, SINSU_RUST_BOUNDS);
    assert!(
        rng_errs.is_empty(),
        "[{label}] RNG 判据红（{} 条）：\n{}",
        rng_errs.len(),
        rng_errs.join("\n")
    );

    eprintln!("[sinsu {label}] PASS");
}

/// **`39`（`SKILL_GROUPDEDING`）**真实台账 → 判定面（五支，卡 §39）。
#[test]
#[ignore = "需先跑 game-svr 五支驱动（tests::groupdeding_parity_*）生成真实台账"]
fn real_rust_ledger_groupdeding() {
    real_ledger_verify_labels(
        "groupdeding",
        &golden_file("groupdeding.json"),
        &[
            "all_miss_hp0",
            "hit_ac_split",
            "miss_by_draw",
            "dead_excluded",
            "dc_range",
        ],
    );
}

/// **`33 冰咆哮`（`SKILL_SNOWWIND`）**真实台账 → 判定面（标签 s1/s5/s13）。
#[test]
#[ignore = "需先跑 game-svr 三支驱动（tests::snowwind_parity_*）生成真实台账"]
fn real_rust_ledger_snowwind() {
    real_ledger_verify_labels(
        "snowwind",
        &golden_file("snowwind.json"),
        &["s1", "s5", "s13"],
    );
}

/// **共享尾 B 类（`SKILL_49`/`51`/`52`/`53`/`54`；case 体只有 `boTrain = true;`）**真实台账 → 判定面
/// （s1/s5/s13）。五号同形，按 stem 参数化（`golden/magic/skill<id>.json` + `skill<id>_<label>.json`）。
///
/// oracle 面（`src/M2Server/Magic/MagicManager.cs`）：`:159 RM_SPELL`（处理器自守卫 ⇒ 施法者
/// **不收**自己的 `SM_SPELL(17)`）→ `:648 RM_MAGICFIRE`（含自己）→ `:659/:661` 门内
/// **恰一次** `Random(3)+1`（`:663`）→ `:666 SendSelfDelayMsg(RM_MAGIC_LVEXP, …, **1000**)`。
///
/// 除通用 `real_ledger_verify_labels`（帧/RNG/Rm/HP 面）外另钉：
/// - 帧序/时延：caster = `[638@0]` + `640@1000ms` 且 **恰 0 条 17**（负向断言，钉自守卫）；
///   target/obs_in = `[17@0, 638@0]`；obs_out = 空。
/// - `Train` 块逐字段两侧相等（`Level` 不变、`TranPoint` 恰加 `1..=3`）。
/// - **前提断言**：`max_train[n_level] > tran_point_after`——MaxTrain 调小会让
///   `CheckMagicLevelUp` 成功 ⇒ 640 改走 `PlayObject.cs:5216` 的 800ms 支（本批 scope 外）；
///   没有这条，下面的「Level 不变」在前提被破坏后会静默变永真。
#[test]
#[ignore = "需先跑 game-svr 三支驱动（tests::skill49_parity_*）生成真实台账"]
fn real_rust_ledger_skill49() {
    verify_skill_tail_ledger("skill49");
}

#[test]
#[ignore = "需先跑 game-svr 三支驱动（tests::skill51_parity_*）生成真实台账"]
fn real_rust_ledger_skill51() {
    verify_skill_tail_ledger("skill51");
}

#[test]
#[ignore = "需先跑 game-svr 三支驱动（tests::skill52_parity_*）生成真实台账"]
fn real_rust_ledger_skill52() {
    verify_skill_tail_ledger("skill52");
}

#[test]
#[ignore = "需先跑 game-svr 三支驱动（tests::skill53_parity_*）生成真实台账"]
fn real_rust_ledger_skill53() {
    verify_skill_tail_ledger("skill53");
}

#[test]
#[ignore = "需先跑 game-svr 三支驱动（tests::skill54_parity_*）生成真实台账"]
fn real_rust_ledger_skill54() {
    verify_skill_tail_ledger("skill54");
}

fn verify_skill_tail_ledger(stem: &str) {
    let golden = golden_file(&format!("{stem}.json"));
    let dir = ledger_dir();
    let want = std::env::var("MIR2_LEDGER_RUN_ID").unwrap_or_default();
    assert!(
        !want.is_empty(),
        "未设置 MIR2_LEDGER_RUN_ID：比较器必须与驱动用同一 id（卡 §28 空转防线）"
    );
    // 某 case 某接收者的 `(ident, due_ms_rel)` 序列。
    let seq = |case: &Value, who: &str| -> Vec<(i64, i64)> {
        case["Receivers"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|r| r["Who"] == who)
            .and_then(|r| r["Sent"].as_array())
            .map(|a| {
                a.iter()
                    .map(|f| (num(f, "ident"), num(f, "due_ms_rel")))
                    .collect()
            })
            .unwrap_or_default()
    };
    // 标签**从 golden 取**（不是写死三支）：新增 case（如 `s1_up`）自动进面；金标准有而台账缺 ⇒ 防空转红。
    let labels: Vec<String> = golden["cases"]
        .as_array()
        .map(|cs| {
            cs.iter()
                .filter_map(|c| c["Label"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    assert!(!labels.is_empty(), "[{stem}] 金标准无 case");
    for label in &labels {
        let label = label.as_str();
        // ---- golden 侧 ----
        let g_case = golden["cases"]
            .as_array()
            .and_then(|cs| cs.iter().find(|c| c["Label"] == label))
            .unwrap_or_else(|| panic!("[{label}] 金标准缺该支"));
        let g_train = &g_case["Train"];
        assert!(
            g_train.is_object(),
            "[{label}] 金标准缺 `Train` 块（`SkillTail` 夹具必须落盘训练面）"
        );
        // ---- 台账侧（防空转：缺件/陈旧/标签不符一律红，不静默跳过）----
        let path = dir.join(format!("{stem}_{label}.json"));
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("防空转：缺真实台账 {}（{e}）", path.display()));
        let side = path.with_extension("run.json");
        let side_text = std::fs::read_to_string(&side).unwrap_or_else(|e| {
            panic!(
                "防空转：台账缺 run.json 侧车 {}（{e}）——多见于「驱动未跑/编译失败，读到上一次的陈旧台账」",
                side.display()
            )
        });
        let side_v: Value = serde_json::from_str(&side_text).expect("run.json 必须是合法 JSON");
        assert_eq!(
            side_v.get("run_id").and_then(Value::as_str),
            Some(want.as_str()),
            "[{label}] 台账陈旧：run_id 不匹配（当前环境 = {want}）；先用同一 id 重跑驱动"
        );
        let v: Value = serde_json::from_str(&text).expect("台账必须是合法 JSON");
        let arr = v.as_array().expect("台账必须是 [case] 数组");
        assert_eq!(arr.len(), 1, "[{label}] 每支台账恰 1 个 case");
        let r_case = &arr[0];
        assert_eq!(
            r_case["Label"].as_str(),
            Some(label),
            "[{label}] 台账 Label 不符"
        );
        let r_train = &r_case["Train"];
        for k in [
            "level_before",
            "level_after",
            "tran_point_before",
            "tran_point_after",
            "train_lv",
            "max_train0",
            "n_level",
            "max_train_at_level",
            "train_level_at_level",
        ] {
            assert_eq!(
                num(g_train, k),
                num(r_train, k),
                "[{label}] Train.{k} 两侧不等（golden={} 台账={}）",
                num(g_train, k),
                num(r_train, k)
            );
        }
        // ---- 训练面形态（前提与载荷；两侧相等已在上；这里钉语义）----
        assert_eq!(
            num(g_train, "tran_point_before"),
            0,
            "[{label}] 夹具前提：初始 TranPoint 必须显式 0（否则「恰加 1..=3」不可钉）"
        );
        let upgrade = num(g_train, "level_after") != num(g_train, "level_before");
        let due640: i64 = if upgrade { 800 } else { 1000 };
        if upgrade {
            // 800ms 升级支（`PlayObject.cs:5210-5216`）：门成立 ⇒ `TranPoint -= MaxTrain`、`Level++`。
            // 用 golden 自己的抽值（`Rng[0]` = `Random(3)`，点值 = 抽+1）独立复算 TranPoint 后值，
            // 不拿实现输出自证门。
            assert_eq!(
                num(g_train, "level_after"),
                num(g_train, "level_before") + 1,
                "[{label}] 升级支：Level 应恰 +1"
            );
            assert_eq!(
                num(g_train, "n_level"),
                num(g_train, "level_before"),
                "[{label}] 升级支：`nLevel` 必须由**升级前** Level 推导"
            );
            let points = g_case["Rng"][0].as_i64().unwrap_or(i64::MIN) + 1;
            let mt = num(g_train, "max_train_at_level");
            let tp_after = num(g_train, "tran_point_after");
            assert!(
                mt <= num(g_train, "tran_point_before") + points,
                "[{label}] 升级支前提：MaxTrain={mt} 必须 ≤ 前值 TranPoint + 抽点 {points}"
            );
            assert_eq!(
                tp_after,
                num(g_train, "tran_point_before") + points - mt,
                "[{label}] 升级支：TranPoint 后值必须 = 前值 + 抽点 − MaxTrain"
            );
            assert!(
                tp_after >= 0,
                "[{label}] 升级支：TranPoint 后值必须 ≥ 0，实得 {tp_after}"
            );
        } else {
            assert_eq!(
                num(g_train, "level_after"),
                num(g_train, "level_before"),
                "[{label}] 未升级支：Level 不得变化"
            );
            let tp = num(g_train, "tran_point_after");
            assert!(
                (1..=3).contains(&tp),
                "[{label}] TranPoint 应 = Random(3)+1 ∈ 1..=3，实得 {tp}"
            );
            assert!(
                num(g_train, "max_train_at_level") > tp,
                "[{label}] 前提：MaxTrain[n_level] 必须 > TranPoint（否则 640 走 800ms 升级支）"
            );
            assert_eq!(
                num(g_train, "n_level"),
                num(g_train, "level_after"),
                "[{label}] `nLevel` 规则（Level<4 && TrainLv>=Level ⇒ nLevel=Level）不成立 ⇒ MaxTrain 断言核错了项"
            );
        }
        // ---- 帧面（golden 与台账同判；负向面只对**产品侧**有意义，故台账侧单独钉）----
        for (case, side_name) in [(g_case, "golden"), (r_case, "台账")] {
            let caster = seq(case, "caster");
            let n17 = caster.iter().filter(|(id, _)| *id == 17).count();
            assert_eq!(
                n17, 0,
                "[{label}/{side_name}] caster 恰 0 条 SM_SPELL(17)（`PlayObject.Message.cs:1455` 自守卫），实得 {caster:?}"
            );
            assert_eq!(
                caster,
                vec![(638, 0), (640, due640)],
                "[{label}/{side_name}] caster 帧序应为 [638@0, 640@{due640}ms]"
            );
            for who in ["target", "obs_in"] {
                assert_eq!(
                    seq(case, who),
                    vec![(17, 0), (638, 0)],
                    "[{label}/{side_name}] {who} 帧序应为 [17@0, 638@0]"
                );
            }
            assert!(
                seq(case, "obs_out").is_empty(),
                "[{label}/{side_name}] obs_out 必须零帧"
            );
        }
    }
    // 通用面：帧逐字段（含 recog/due_ms_rel/tail）+ `Rng == Rng ++ DeliverRng` + Rm 多重集 + HP。
    // 标签同上（从 golden 取）：golden 与台账的 case 集必须一一对应（多了/少了都红）。
    let label_refs: Vec<&str> = labels.iter().map(String::as_str).collect();
    real_ledger_verify_labels(stem, &golden, &label_refs);
}

/// **`10 疾光电影`（`SKILL_SHOOTLIGHTEN`，MG-18）**真实台账 → 判定面（标签 s1/s5/s13）。
///
/// 与 9 同形（`MagicManager.cs:277-289`），差别只有两处：`steps = 8`（9 是 5）与
/// `undeadAttack = true` ⇒ **逐击** `Round(pow*1.5)` 复合（与目标是否不死系无关）。
/// 判据沿用 `real_ledger_verify_labels`（帧/RNG/Rm/HP），本 leaf 的"复合确实发生"由
/// `Rng` 与 `HpBefore/HpAfter` 逐值比较兜住（伤害值进入 `SM_STRUCK(31)` 头与 `RM_MAGSTRUCK`）。
#[test]
#[ignore = "需先跑 game-svr 三支驱动（tests::skill10_parity_*）生成真实台账"]
fn real_rust_ledger_skill10() {
    real_ledger_verify_labels(
        "skill10",
        &golden_file("skill10.json"),
        &["s1", "s5", "s13"],
    );
}

/// **`9 地狱火`（`SKILL_FIRE`）**真实台账 → 判定面（标签 s1/s5/s13）。
///
/// 判定面：逐格穿透骰门 + `RM_MAGSTRUCK(8044)` 逐怪（怪物不 `Operate` ⇒ `Sent` 空、RM 为准）
/// + 玩家 target 的术后 HP（600ms 延迟在该支 `WaitMs` 内交付）。
#[test]
#[ignore = "需先跑 game-svr 三支驱动（tests::fire_parity_*）生成真实台账"]
fn real_rust_ledger_fire() {
    real_ledger_verify_labels("fire", &golden_file("fire.json"), &["s1", "s5", "s13"]);
}

/// **`23 爆裂火焰`（`SKILL_FIREBOOM`）**真实台账 → 判定面（标签 s1/s5/s13）。
#[test]
#[ignore = "需先跑 game-svr 三支驱动（tests::fireboom_parity_*）生成真实台账"]
fn real_rust_ledger_fireboom() {
    real_ledger_verify_labels(
        "fireboom",
        &golden_file("fireboom.json"),
        &["s1", "s5", "s13"],
    );
}

/// **`47`（`SKILL_47`，与 23 同 helper `MagBigExplosion(..., FireBoomRage)`）**真实台账 →
/// 判定面（标签 s1/s5/s13；金标准 `golden/magic/fireboom47.json`）。
#[test]
#[ignore = "需先跑 game-svr 三支驱动（tests::fireboom47_parity_*）生成真实台账"]
fn real_rust_ledger_fireboom47() {
    real_ledger_verify_labels(
        "fireboom47",
        &golden_file("fireboom47.json"),
        &["s1", "s5", "s13"],
    );
}

/// **`24`（`SKILL_LIGHTFLOWER` → `MagElecBlizzard(ElecBlizzardRange)`）**真实台账 →
/// 判定面（金标准 `golden/magic/lightflower.json`，C# oracle 生成）。
///
/// **标签从 golden 取**（同 `verify_skill_tail_ledger` 口径）：`s1`/`s5`/`s13` + `undead` 自动进面；
/// 金标准有而台账缺 ⇒ `real_ledger_verify_labels` 的防空转 panic 点名（不静默跳过）。
///
/// **MG-38（§35 对象类型分支，2026-11-26）**：`undead` 支在同场放 `(101,100)/race=80/
/// life_attrib=LA_UNDEAD` 的 `mon0`（装置 `MonsterSpec.OperateStruck=true` 像真怪一样结算）。
/// oracle `MagElecBlizzard`（`MagicManager.cs:1128/1134`）对非不死目标 `nPower/10`、对不死
/// 目标 `nPower` ⇒ `mon0` 的 `RM_MAGSTRUCK(8044).p1 = 15`，而同支玩家 target = 1（golden 实测）。
#[test]
#[ignore = "需先跑 game-svr 四支驱动（tests::lightflower_parity_*）生成真实台账"]
fn real_rust_ledger_lightflower() {
    let golden = golden_file("lightflower.json");
    let labels: Vec<String> = golden["cases"]
        .as_array()
        .map(|cs| {
            cs.iter()
                .filter_map(|c| c["Label"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    assert!(!labels.is_empty(), "[lightflower] 金标准无 case（防空转）");
    let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
    real_ledger_verify_labels("lightflower", &golden, &refs);
}

/// **`40`（`SKILL_UNAMYOUNSUL`）**真实台账 → 判定面（四支分支矩阵，卡 §34）。
///
/// 除常规帧面/RM/RNG 比较外，**三态计时器前后值三处逐值比**：golden `Fixture.status_time_arr`
/// ↔ 台账 `StatusTimeArrBefore`、golden case `StatusTimeArr` ↔ 台账 `StatusTimeArrAfter`
/// （术后值由装置侧与驱动侧各自从"门结果 + 术前值"独立推导，不比实现）。
#[test]
#[ignore = "需先跑 game-svr 四支驱动（tests::unamyounsul_parity_*）生成真实台账"]
fn real_rust_ledger_unamyounsul() {
    let labels = [
        "gate_open_all",
        "gate_closed_all",
        "gate_open_zero",
        "gate_open_mixed",
    ];
    let golden = golden_file("unamyounsul.json");
    real_ledger_verify_labels("unamyounsul", &golden, &labels);
    let dir = ledger_dir();
    for label in labels {
        let g = golden["cases"]
            .as_array()
            .expect("golden.cases")
            .iter()
            .find(|c| c["Label"] == label)
            .unwrap_or_else(|| panic!("golden 缺 {label} 支"))
            .clone();
        let path = dir.join(format!("unamyounsul_{label}.json"));
        let raw = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("缺真实台账 {}（{e}）", path.display()));
        let v: Value = serde_json::from_str(&raw).unwrap();
        let c = &v[0];
        assert_eq!(
            c["StatusTimeArrBefore"], g["Fixture"]["status_time_arr"],
            "[{label}] 三态术前值必须等于 golden Fixture.status_time_arr"
        );
        assert_eq!(
            c["StatusTimeArrAfter"], g["StatusTimeArr"],
            "[{label}] 三态术后值必须等于 golden StatusTimeArr"
        );
    }
}

/// **`19 集体隐身术`（`SKILL_BIGCLOAK`）**真实台账 → 判定面（标签 s1/s5/s13）。
#[test]
#[ignore = "需先跑 game-svr 三支驱动（tests::big_cloak_parity_*）生成真实台账"]
fn real_rust_ledger_bigcloak() {
    real_ledger_verify_labels(
        "bigcloak",
        &golden_file("bigcloak.json"),
        &["s1", "s5", "s13"],
    );
}

/// **`29 群体治愈术`（`SKILL_BIGHEALLING`）**真实台账 → 判定面（标签 heal/full/both）。
#[test]
#[ignore = "需先跑 game-svr 三支驱动（tests::big_healing_parity_*）生成真实台账"]
fn real_rust_ledger_bighealing() {
    // **MG-12/40**：标签从 golden 取（恢复 tick 臂一并进判据）。
    let golden = golden_file("bighealing.json");
    let labels = golden_labels(&golden, "bighealing");
    real_ledger_verify_labels("bighealing", &golden, &labels);
}

/// **`18 隐身术`（`SKILL_CLOAK`）**真实台账 → 判定面（标签 s1/s5/s13）。
#[test]
#[ignore = "需先跑 game-svr 三支驱动（tests::cloak_parity_*）生成真实台账"]
fn real_rust_ledger_cloak() {
    // **标签从 golden 取**（MG-32 起含 `monster_clear`/`monster_keep`/`monster_far`；缺台账即红）。
    let golden = golden_file("cloak.json");
    let labels: Vec<String> = golden["cases"]
        .as_array()
        .map(|cs| {
            cs.iter()
                .filter_map(|c| c["Label"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    assert!(!labels.is_empty(), "[cloak] 金标准无 case（防空转）");
    let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
    real_ledger_verify_labels("cloak", &golden, &refs);
}

/// **`28 心灵启示`（`SKILL_SHOWHP`）**真实台账 → 判定面（标签 miss/edge/hit = `Random(6)` 门）。
#[test]
#[ignore = "需先跑 game-svr 三支驱动（tests::showhp_parity_*）生成真实台账"]
fn real_rust_ledger_showhp() {
    real_ledger_verify_labels(
        "showhp",
        &golden_file("showhp.json"),
        &["miss", "edge", "hit"],
    );
}

/// **`14/15`（`SKILL_HANGMAJINBUB`/`SKILL_DEJIWONHO`）**真实台账 → 判定面（标签 s1/s5/s13）。
#[test]
#[ignore = "需先跑 game-svr 三支驱动（tests::hangmajinbub_parity_*）生成真实台账"]
fn real_rust_ledger_hangmajinbub() {
    real_ledger_verify_labels(
        "hangmajinbub",
        &golden_file("hangmajinbub.json"),
        &["s1", "s5", "s13"],
    );
}

#[test]
#[ignore = "需先跑 game-svr 三支驱动（tests::dejiwonho_parity_*）生成真实台账"]
fn real_rust_ledger_dejiwonho() {
    real_ledger_verify_labels(
        "dejiwonho",
        &golden_file("dejiwonho.json"),
        &["s1", "s5", "s13"],
    );
}

/// **`31 魔法盾`**真实台账 → 判定面（标签 = `s1`/`s5`/`s13`；本支无 AntiMagic 骰）；
/// 金标准 `golden/magic/shield.json`。
#[test]
#[ignore = "需先跑 game-svr 三支驱动（tests::shield_parity_*）生成真实台账"]
fn real_rust_ledger_shield() {
    real_ledger_verify_labels("shield", &golden_file("shield.json"), &["s1", "s5", "s13"]);
}

/// **`44 灭天火`**真实台账 → 判定面（金标准 `golden/magic/hbfireball.json`）。
#[test]
#[ignore = "需先跑 game-svr 三支驱动（tests::hbfireball_parity_*）生成真实台账"]
fn real_rust_ledger_hbfireball() {
    // **标签从 golden 取**（MG-53 起含 `player_lv0`/`monster_lv0`；缺台账即防空转红）。
    let golden = golden_file("hbfireball.json");
    let labels: Vec<String> = golden["cases"]
        .as_array()
        .map(|cs| {
            cs.iter()
                .filter_map(|c| c["Label"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    assert!(!labels.is_empty(), "[hbfireball] 金标准无 case（防空转）");
    let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
    real_ledger_verify_labels("hbfireball", &golden, &refs);
}

/// **`13 火符`**真实台账 → 判定面（金标准 `golden/magic/firecharm.json`）。
#[test]
#[ignore = "需先跑 game-svr 三支驱动（tests::firecharm_parity_*）生成真实台账"]
fn real_rust_ledger_firecharm() {
    // **标签从 golden 取**（MG-27 起含 `monster`；缺台账即防空转红）。
    let golden = golden_file("firecharm.json");
    let labels: Vec<String> = golden["cases"]
        .as_array()
        .map(|cs| {
            cs.iter()
                .filter_map(|c| c["Label"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    assert!(!labels.is_empty(), "[firecharm] 金标准无 case（防空转）");
    let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
    real_ledger_verify_labels("firecharm", &golden, &refs);
}

/// **`2 治愈`**真实台账 → 判定面（标签 = `heal` / `full` / `self`；本支无 AntiMagic 骰）；
/// 金标准 `golden/magic/healing.json`。
#[test]
#[ignore = "需先跑 game-svr 三支驱动（tests::healing_parity_*）生成真实台账"]
fn real_rust_ledger_healing() {
    // **MG-12/40**：标签从 golden 取（恢复 tick 臂一并进判据）。
    let golden = golden_file("healing.json");
    let labels = golden_labels(&golden, "healing");
    real_ledger_verify_labels("healing", &golden, &labels);
}

/// 火球 id=1（`SKILL_FIREBALL`）真实台账 → 判定面。
#[test]
#[ignore = "需先跑 game-svr 三支驱动（tests::fireball_parity_*）生成真实台账"]
fn real_rust_ledger_parity() {
    real_ledger_verify("fireball", &golden());
}

/// **11/45 雷电（`SKILL_LIGHTENING` / `SKILL_45`）真实台账 → 判定面**；
/// 金标准 `golden/magic/lightning-11.json` / `lightning-45.json`。
#[test]
#[ignore = "需先跑 game-svr 三支驱动（tests::lightning11_parity_*）生成真实台账"]
fn real_rust_ledger_lightning11() {
    // **标签从 golden 取**（MG-19 起含 `monster` / `monster_undead`；缺台账即防空转红）。
    let golden = golden_file("lightning-11.json");
    let labels: Vec<String> = golden["cases"]
        .as_array()
        .map(|cs| {
            cs.iter()
                .filter_map(|c| c["Label"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    assert!(!labels.is_empty(), "[lightning11] 金标准无 case（防空转）");
    let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
    real_ledger_verify_labels("lightning11", &golden, &refs);
}

#[test]
#[ignore = "需先跑 game-svr 三支驱动（tests::lightning45_parity_*）生成真实台账"]
fn real_rust_ledger_lightning45() {
    // **标签从 golden 取**（MG-54 起含 `monster` / `monster_undead`；缺台账即防空转红）。
    let golden = golden_file("lightning-45.json");
    let labels: Vec<String> = golden["cases"]
        .as_array()
        .map(|cs| {
            cs.iter()
                .filter_map(|c| c["Label"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    assert!(!labels.is_empty(), "[lightning45] 金标准无 case（防空转）");
    let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
    real_ledger_verify_labels("lightning45", &golden, &refs);
}

/// **id=5（`SKILL_FIREBALL2`）真实台账 → 判定面**；金标准 `golden/magic/fireball-5.json`。
#[test]
#[ignore = "需先跑 game-svr 四支驱动（tests::fireball5_parity_*）生成真实台账"]
fn real_rust_ledger_fireball5() {
    // **标签从 golden 取**（MG-11 起新增 `monster` 支；同 `verify_skill_tail_ledger` 口径）：
    // 金标准有而台账缺 ⇒ `real_ledger_verify_labels` 的防空转 panic 点名。
    let golden = golden_file("fireball-5.json");
    let labels: Vec<String> = golden["cases"]
        .as_array()
        .map(|cs| {
            cs.iter()
                .filter_map(|c| c["Label"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    assert!(!labels.is_empty(), "[fireball5] 金标准无 case（防空转）");
    let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
    real_ledger_verify_labels("fireball5", &golden, &refs);
}

// ---------------------------------------------------------------------------
// B16 Arm A（非零值魔表对照臂）：per-stem 绝对值表 + 三支 comparator。
// 期望表只对新增 `-nonzero` stem 生效；值来自 `sql/mir2_data.sql` 的生效行
// （`LoadMagicDB` 对重复 MagID 取最先读到的行），防止 golden 重生成时静默换行。
// ---------------------------------------------------------------------------
fn nonzero_fixture_expect(stem: &str, golden: &Value) {
    let exp: [(&str, u64); 5] = match stem {
        "fireball-nonzero" => [
            ("magic_power", 8),
            ("magic_max_power", 8),
            ("magic_def_power", 2),
            ("magic_def_max_power", 2),
            ("magic_train_lv", 3),
        ],
        "lightning11-nonzero" => [
            ("magic_power", 14),
            ("magic_max_power", 28),
            ("magic_def_power", 10),
            ("magic_def_max_power", 10),
            ("magic_train_lv", 3),
        ],
        "lightflower-nonzero" => [
            ("magic_power", 10),
            ("magic_max_power", 30),
            ("magic_def_power", 10),
            ("magic_def_max_power", 30),
            ("magic_train_lv", 3),
        ],
        _ => panic!("nonzero_fixture_expect 只用于 -nonzero 支：{stem}"),
    };
    for c in golden["cases"].as_array().expect("cases 必须是数组") {
        let label = c["Label"].as_str().unwrap_or("?");
        for (k, v) in exp {
            assert_eq!(
                c["Fixture"][k].as_u64().unwrap_or(0),
                v,
                "[{stem}/{label}] Fixture.{k} 与 sql 真值 {v} 不符"
            );
        }
    }
}

#[test]
#[ignore = "需先跑 game-svr 驱动（tests::fireball_nonzero_parity_*）生成真实台账"]
fn real_rust_ledger_fireball_nonzero() {
    let g = golden_file("fireball-nonzero.json");
    nonzero_fixture_expect("fireball-nonzero", &g);
    real_ledger_verify_labels("fireball-nonzero", &g, &["miss", "edge", "hit"]);
}

#[test]
#[ignore = "需先跑 game-svr 驱动（tests::lightning11_nonzero_parity_*）生成真实台账"]
fn real_rust_ledger_lightning11_nonzero() {
    let g = golden_file("lightning-11-nonzero.json");
    nonzero_fixture_expect("lightning11-nonzero", &g);
    real_ledger_verify_labels("lightning11-nonzero", &g, &["miss", "edge", "hit"]);
}

#[test]
#[ignore = "需先跑 game-svr 驱动（tests::lightflower_nonzero_parity_*）生成真实台账"]
fn real_rust_ledger_lightflower_nonzero() {
    let g = golden_file("lightflower-nonzero.json");
    nonzero_fixture_expect("lightflower-nonzero", &g);
    real_ledger_verify_labels("lightflower-nonzero", &g, &["s1", "s5", "s13"]);
}

// ---------------------------------------------------------------------------
// 比较器自检：金标准逐支喂回（identity）+ 三条篡改必须红 + 防空转必须红。
// ---------------------------------------------------------------------------
fn golden_file(name: &str) -> Value {
    assert!(
        !name.starts_with("legacy/"),
        "`legacy/` 目录的 golden 只作历史留档，**不作 verified 依据**（卡 §7/§8.3）：{name}"
    );
    let path = format!("{}/golden/magic/{name}", env!("CARGO_MANIFEST_DIR"));
    let text = std::fs::read_to_string(path).expect("金标准必须存在（缺文件即红）");
    serde_json::from_str(&text).expect("金标准必须是合法 JSON")
}

fn golden() -> Value {
    golden_file("fireball.json")
}

/// 把金标准改造成"Rust 台账"同构输入（identity）：只为验证比较器本身，不代表驱动已跑通。
fn ledger_from(g: &Value) -> Value {
    let mut v = g.clone();
    let cases = v.get_mut("cases").and_then(Value::as_array_mut).unwrap();
    for c in cases.iter_mut() {
        // 金标准里没有 RngSites / RngSeqs：identity 台账补上合法值（驱动落地后由真实数据替换）。
        let n = c
            .get("Rng")
            .and_then(Value::as_array)
            .map(|a| a.len())
            .unwrap_or(0);
        let sites = Value::Array(
            (0..n)
                .map(|_| Value::String("crates/world-bevy/src/magic.rs".into()))
                .collect(),
        );
        let seqs = Value::Array((0..n).map(|i| Value::from(i as i64 + 1)).collect());
        let obj = c.as_object_mut().unwrap();
        obj.insert("RngSites".into(), sites);
        obj.insert("RngSeqs".into(), seqs);
    }
    v
}

#[test]
fn vacuous_guard_empty_cases_red() {
    let g = serde_json::json!({"cases": []});
    let r = ledger_from(&g);
    assert!(verify(&g, &r).is_err(), "零用例必须红");
}

#[test]
fn vacuous_guard_missing_cases_red() {
    let g = serde_json::json!({});
    let r = serde_json::json!({});
    assert!(verify(&g, &r).is_err(), "缺字段必须红");
}

#[test]
fn identity_accepts_golden() {
    let g = golden();
    let r = ledger_from(&g);
    assert_eq!(
        verify(&g, &r),
        Ok(()),
        "identity 必须绿（比较器自身不假红）"
    );
}

#[test]
fn tooth_caster_sm_spell_present_red() {
    let g = golden();
    let mut tampered = g.clone();
    let cases = tampered
        .get_mut("cases")
        .and_then(Value::as_array_mut)
        .unwrap();
    let recv = cases[0]
        .get_mut("Receivers")
        .and_then(Value::as_array_mut)
        .unwrap();
    let caster = recv
        .iter_mut()
        .find(|r| r.get("Who").and_then(Value::as_str) == Some("caster"))
        .unwrap();
    caster
        .get_mut("Sent")
        .and_then(Value::as_array_mut)
        .unwrap()
        .push(serde_json::json!({"ident":17,"param":0,"tag":0,"series":1,"tail":"1"}));
    let err = verify(&tampered, &ledger_from(&tampered)).unwrap_err();
    assert!(
        err.iter().any(|e| e.contains("SM_SPELL(17)")),
        "必须红在负向牙①：{err:?}"
    );
}

#[test]
fn tooth_foreign_site_red() {
    let g = golden();
    let mut r = ledger_from(&g);
    let cases = r.get_mut("cases").and_then(Value::as_array_mut).unwrap();
    let sites = cases[0]
        .get_mut("RngSites")
        .and_then(Value::as_array_mut)
        .unwrap();
    sites[0] = Value::String("crates/world-bevy/src/mongen.rs".into());
    let err = verify(&g, &r).unwrap_err();
    assert!(
        err.iter().any(|e| e.contains("不在允许清单")),
        "必须红在 site 清单：{err:?}"
    );
}

// **已删除：`tooth_rm_late_changed_red`（2026-11-26）**。
// 为什么删：该牙咬的是 RM 的 `late`（以及同维的 `priority`）列，而这两个维度**不是产品产出**
// ——`PendingMagStruck` 没有该字段，台账值是驱动按 oracle 语义重建的常量（卡
// `DEV-RM-PRIORITY-UNOBSERVED`，见 `docs/issues/golden-harness-blind-spots.md` B26）。
// 拿驱动常量与 golden 比 = 循环自证，不证明产品，故该维度移出比较面、牙随维度移除。
// **这不是"为让红变绿而删牙"**：删除前它咬的是驱动自己的常量；恢复判定必须先让产品真的产出
// 并使用 `priority/late`（不是为测试建模），届时按 `rm_fields` 注释恢复六/七字段并重建牙。

/// 口径 B 的正向牙：**只调换 RM 顺序**必须仍绿（多重集语义；若哪天有人把 RM 改回按序比较，本用例红）。
#[test]
fn rm_multiset_ignores_order_accepts() {
    let g = golden();
    let mut r = ledger_from(&g);
    let cases = r.get_mut("cases").and_then(Value::as_array_mut).unwrap();
    let caster = cases[0]
        .get_mut("Receivers")
        .and_then(Value::as_array_mut)
        .unwrap()
        .iter_mut()
        .find(|x| x.get("Who").and_then(Value::as_str) == Some("caster"))
        .unwrap();
    let rm = caster.get_mut("Rm").and_then(Value::as_array_mut).unwrap();
    assert!(rm.len() >= 2, "前提：caster 的 RM 至少 2 条");
    rm.swap(0, 1);
    assert_eq!(verify(&g, &r), Ok(()), "RM 换序必须仍绿（多重集）");
}

/// 口径 B 的负向牙：RM **字段**变了必须红（多重集不等于不比内容）。
#[test]
fn tooth_rm_field_changed_red() {
    let g = golden();
    let mut r = ledger_from(&g);
    let cases = r.get_mut("cases").and_then(Value::as_array_mut).unwrap();
    let caster = cases[0]
        .get_mut("Receivers")
        .and_then(Value::as_array_mut)
        .unwrap()
        .iter_mut()
        .find(|x| x.get("Who").and_then(Value::as_str) == Some("caster"))
        .unwrap();
    let rm = caster.get_mut("Rm").and_then(Value::as_array_mut).unwrap();
    let old = rm[0].get("p1").and_then(Value::as_i64).unwrap_or(0);
    rm[0]["p1"] = Value::from(old + 1);
    let err = verify(&g, &r).unwrap_err();
    assert!(
        err.iter().any(|e| e.contains("Rm 多重集不等")),
        "必须红在 RM 多重集：{err:?}"
    );
}

#[test]
fn tooth_missing_rng_line_red() {
    let g = golden();
    let mut r = ledger_from(&g);
    let cases = r.get_mut("cases").and_then(Value::as_array_mut).unwrap();
    let rng = cases[0]
        .get_mut("Rng")
        .and_then(Value::as_array_mut)
        .unwrap();
    rng.pop();
    let err = verify(&g, &r).unwrap_err();
    assert!(
        err.iter().any(|e| e.contains("RNG 行数不等")),
        "必须红在总量：{err:?}"
    );
}

/// **施毒系（6/48，MG-13/14/15/50/64/65）**：非退化 golden 的真实台账判定面。
/// 三旋钮（护身符 `Shape` / 目标 `AntiPoison` / `AmyOunsulPoint`）全部经 `Fixture` 传给驱动；
/// 期望只从 golden 来（`Rng`/`RM_POISON(8037)` 多重集/帧面/`TargetHp`）。
/// 驱动：`tests::amyounsul_parity_*`（6 Shape=1）、`tests::amyounsul2_parity_*`（6 Shape=2）、
/// `tests::groupamyounsul_parity_*`（48 Shape=1）、`tests::groupamyounsul2_parity_*`（48 Shape=2）。
#[test]
#[ignore = "需先跑 game-svr 驱动（amyounsul_parity_*）生成真实台账"]
fn real_rust_ledger_amyounsul() {
    // **MG-13/14 位号面（2026-11-28）**：`prebuff_bit2` = 单目标臂跨 buff 位号。
    real_ledger_verify_labels(
        "amyounsul",
        &golden_file("amyounsul.json"),
        &["s1", "s5", "s13", "prebuff_bit2"],
    );
}

#[test]
#[ignore = "需先跑 game-svr 驱动（amyounsul2_parity_*）生成真实台账"]
fn real_rust_ledger_amyounsul2() {
    // **MG-15 ②（2026-11-28）**：`prebuff_red` = 红毒 0x40000000 位路径。
    real_ledger_verify_labels(
        "amyounsul2",
        &golden_file("amyounsul2.json"),
        &["s1", "s5", "s13", "prebuff_red"],
    );
}

/// **`MG-15 ①`（2026-11-28）**：单目标臂 Shape=0 stem 比较器（fall-through：不投毒、
/// 护身符照扣、训练尾 640 在）。
#[test]
#[ignore = "需先跑 game-svr 驱动（amyounsul_shape0_parity_*）生成真实台账"]
fn real_rust_ledger_amyounsul_shape0() {
    real_ledger_verify_labels(
        "amyounsul-shape0",
        &golden_file("amyounsul-shape0.json"),
        &["s1", "s5", "s13"],
    );
}

#[test]
#[ignore = "需先跑 game-svr 驱动（groupamyounsul2_parity_*）生成真实台账"]
fn real_rust_ledger_groupamyounsul2() {
    real_ledger_verify_labels(
        "groupamyounsul2",
        &golden_file("groupamyounsul2.json"),
        &["s1", "s5", "s13"],
    );
}

// ============================================================================
// 施毒系（6/48，MG-13/14/15/50/64/65）毒口径牙
// ============================================================================

/// **毒口径 L1 牙**：`frame_order_scope = multi_victim_same_tick` 分支（施毒件）的
/// `640` late 帧 `due_ms_rel` 判据（|Δ|≤32ms）与 chained 帧的批次类判据。
///
/// 子断言（每条都对应一个"删掉判据就变绿"的失效模式）：
/// ① 基准（golden 1000 / rust 1000）⇒ 绿；
/// ② golden 1000 vs rust **600** ⇒ 红（400ms 远大于 32ms 容差）；
/// ③ golden 1000 vs rust **0** ⇒ 红（1000ms 远大于 32ms）；
/// ④ golden 1000 vs rust **1016** ⇒ **绿**（2× `TickCount` 15.6ms 粒度，落在容差内）；
/// ⑤ chained 帧 `657` 的 `due` 从 1000 改 **0** ⇒ 红（交付批 vs 施法批的批次类仍在判）。
#[test]
fn tooth_poison_late_due_tolerance() {
    let label = "s1";
    let frame = |ident: i64, due: i64| {
        serde_json::json!({
            "ident": ident, "recog": 1002, "param": 0, "tag": 32768,
            "series": 7, "due_ms_rel": due, "tail": "",
        })
    };
    let receivers =
        |frames: Vec<Value>| serde_json::json!([{ "Who": "target", "Rm": [], "Sent": frames }]);
    let mk_g = || {
        let mut g = synth_poison_golden(label, serde_json::json!([1, 2]));
        g["cases"][0]["Fixture"]["frame_order_scope"] = Value::from("multi_victim_same_tick");
        g["cases"][0]["Fixture"]["amulet_shape"] = Value::from(1);
        g["cases"][0]["Receivers"] = receivers(vec![frame(657, 1000), frame(640, 1000)]);
        g
    };
    let mk_r = |tail_due: i64, chained_due: i64| {
        let mut r = synth_poison_rust(label, serde_json::json!([1, 2]));
        r["cases"][0]["Receivers"] = receivers(vec![frame(657, chained_due), frame(640, tail_due)]);
        r
    };
    let g = mk_g();
    assert!(
        verify_union(&g, &mk_r(1000, 1000)).is_ok(),
        "基准（640 1000/1000）必须绿：{:?}",
        verify_union(&g, &mk_r(1000, 1000))
    );
    // ② 640: 1000 → 600
    let err = verify_union(&g, &mk_r(600, 1000)).unwrap_err();
    assert!(
        err.iter().any(|e| e.contains("640 late due")),
        "640 的 due 1000→600 必须红在容差判据：{err:?}"
    );
    // ②b 640: 1000 → 950（|Δ|=50 > 32ms）必须红——容差不是"随便放宽"
    let err = verify_union(&g, &mk_r(950, 1000)).unwrap_err();
    assert!(
        err.iter().any(|e| e.contains("640 late due")),
        "640 的 due 1000→950（|Δ|=50 > 32）必须红在容差判据：{err:?}"
    );
    // ③ 640: 1000 → 0
    let err = verify_union(&g, &mk_r(0, 1000)).unwrap_err();
    assert!(
        err.iter().any(|e| e.contains("640 late due")),
        "640 的 due 1000→0 必须红在容差判据：{err:?}"
    );
    // ④ 640: 1000 → 1016（粒度内）必须绿
    let ok = verify_union(&g, &mk_r(1016, 1000));
    assert!(
        ok.is_ok(),
        "640 的 due 1000→1016（|Δ|=16 ≤ 32）必须绿：{ok:?}"
    );
    // ⑤ chained 657: 1000 → 0（批次类变化）必须红
    let err = verify_union(&g, &mk_r(1000, 0)).unwrap_err();
    assert!(
        err.iter().any(|e| e.contains("多重集")),
        "chained 帧 657 的 due 1000→0（交付批→施法批）必须红：{err:?}"
    );
}

/// **`10139`（`RM_CHARSTATUSCHANGED`）的 `p1` 必须逐值比**。
///
/// 期望值**从 golden 读**（`amyounsul.json` 的 10139.p1；本夹具 = 绿毒 `i=0` ⇒ `0x80000000`
/// 即 `i32::MIN`），**不硬写常量**；牙 = 把台账的 `p1` 改一个值 ⇒ 比较器在 `Rm` 多重集上红。
/// （红毒在本夹具也是 `i32::MIN`：位号是 buff 插入序，不是 PoisonState——见交接单口径。）
#[test]
fn tooth_poison_10139_status_p1_compared_red() {
    let label = "s1";
    let g_file = golden_file("amyounsul.json");
    let p1 = g_file["cases"]
        .as_array()
        .expect("cases")
        .iter()
        .flat_map(|c| c["Receivers"].as_array().expect("Receivers"))
        .flat_map(|r| r["Rm"].as_array().expect("Rm"))
        .find(|m| m["ident"].as_i64() == Some(10139))
        .map(|m| m["p1"].as_i64().expect("10139.p1 是整数"))
        .expect("amyounsul golden 必须含 10139");
    let receivers = |rm_p1: i64| {
        serde_json::json!([{
            "Who": "target", "Sent": [],
            "Rm": [{ "ident": 10139, "wParam": 0, "p1": rm_p1, "p2": 0, "p3": 0,
                     "priority": 155, "late": false }],
        }])
    };
    let mut g = synth_golden(label, serde_json::json!([1, 2]), Some("cover_drain"));
    g["cases"][0]["Receivers"] = receivers(p1);
    let mk_r = |v: i64| {
        let mut r = synth_rust(
            label,
            serde_json::json!([1, 2]),
            Some(serde_json::json!([])),
        );
        r["cases"][0]["Receivers"] = receivers(v);
        r
    };
    assert!(
        verify_union(&g, &mk_r(p1)).is_ok(),
        "基准（10139.p1 两侧都用 golden 值 {p1}）必须绿：{:?}",
        verify_union(&g, &mk_r(p1))
    );
    let mutated = if p1 == i64::MIN { 0 } else { p1 + 1 };
    let err = verify_union(&g, &mk_r(mutated)).unwrap_err();
    assert!(
        err.iter().any(|e| e.contains("Rm 多重集")),
        "10139.p1 {p1}→{mutated} 必须在 Rm 多重集上红：{err:?}"
    );
}

/// **`657`（`SM_CHARSTATUSCHANGED`）的 `series` 必须 = 接收者 `HitSpeed`，且在比较面里**。
///
/// 期望值从 golden 读：`Fixture.hit_speed` 与各收件人 657 帧的 `series` 必须一致；
/// 牙 = 把台账 657 的 `series` 改成 0（"未点亮"的旧值）⇒ 毒口径多重集红。
#[test]
fn tooth_poison_657_series_compared_red() {
    let label = "s1";
    let gf = golden_file("amyounsul.json");
    let case = gf["cases"]
        .as_array()
        .expect("cases")
        .iter()
        .find(|c| c["Label"].as_str() == Some(label))
        .expect("amyounsul 必须有 s1");
    let hit_speed = case["Fixture"]["hit_speed"].as_i64().expect("hit_speed");
    let series = case["Receivers"]
        .as_array()
        .expect("Receivers")
        .iter()
        .flat_map(|r| r["Sent"].as_array().expect("Sent"))
        .find(|f| f["ident"].as_i64() == Some(657))
        .map(|f| f["series"].as_i64().expect("657.series"))
        .expect("amyounsul golden 必须含 657");
    assert_eq!(
        series, hit_speed,
        "657.series({series}) 必须 = 收件人 HitSpeed({hit_speed})——否则 golden 自身不自洽"
    );
    assert_ne!(hit_speed, 0, "HitSpeed 必须非退化（0 与缺省不可区分）");
    let receivers = |v: i64| {
        serde_json::json!([{
            "Who": "target", "Rm": [],
            "Sent": [{ "ident": 657, "recog": 1002, "param": 0, "tag": 32768,
                       "series": v, "due_ms_rel": 1000, "tail": "" }],
        }])
    };
    let mut g = synth_poison_golden(label, serde_json::json!([1, 2]));
    g["cases"][0]["Fixture"]["frame_order_scope"] = Value::from("multi_victim_same_tick");
    g["cases"][0]["Fixture"]["amulet_shape"] = Value::from(1);
    g["cases"][0]["Receivers"] = receivers(series);
    let mk_r = |v: i64| {
        let mut r = synth_poison_rust(label, serde_json::json!([1, 2]));
        r["cases"][0]["Receivers"] = receivers(v);
        r
    };
    assert!(
        verify_union(&g, &mk_r(series)).is_ok(),
        "基准（657.series 两侧 = {series}）必须绿：{:?}",
        verify_union(&g, &mk_r(series))
    );
    let err = verify_union(&g, &mk_r(0)).unwrap_err();
    assert!(
        err.iter().any(|e| e.contains("多重集")),
        "657.series {series}→0 必须红：{err:?}"
    );
}

/// **`SM_SYSMESSAGE(100)` 的 `tail`（`[String] YouPoisoned` 的 GBK 编码）必须逐字节比**。
///
/// 期望值从 golden 读（`amyounsul.json` 的 100 帧 `tail`，由部署 `String.conf:31` 的
/// 「您中毒了!!!」编码而来）；牙 = 台账 `tail` 改**一个字符** ⇒ 毒口径多重集红。这条把
/// 「① 三键 fail-closed」的文本链路钉在比较面上：驱动若回退成 `b""`，这里必然红。
#[test]
fn tooth_poison_sysmessage_tail_compared_red() {
    let label = "s1";
    let gf = golden_file("amyounsul.json");
    let case = gf["cases"]
        .as_array()
        .expect("cases")
        .iter()
        .find(|c| c["Label"].as_str() == Some(label))
        .expect("amyounsul 必须有 s1");
    let tail = case["Receivers"]
        .as_array()
        .expect("Receivers")
        .iter()
        .flat_map(|r| r["Sent"].as_array().expect("Sent"))
        .find(|f| f["ident"].as_i64() == Some(100))
        .map(|f| f["tail"].as_str().expect("100.tail").to_string())
        .expect("amyounsul golden 必须含 100");
    assert!(
        !tail.is_empty(),
        "100 帧 tail 不得为空（否则文本链退化成 b\"\"）"
    );
    // 改**一个字符**（末位换成一个不同的 ASCII 字符）。
    let mut chars: Vec<char> = tail.chars().collect();
    let last = chars.len() - 1;
    chars[last] = if chars[last] == 'A' { 'B' } else { 'A' };
    let mutated: String = chars.into_iter().collect();
    assert_ne!(mutated, tail);
    let receivers = |t: &str| {
        serde_json::json!([{
            "Who": "target", "Rm": [],
            "Sent": [{ "ident": 100, "recog": 1002, "param": 14591, "tag": 0,
                       "series": 1, "due_ms_rel": 1000, "tail": t }],
        }])
    };
    let mut g = synth_poison_golden(label, serde_json::json!([1, 2]));
    g["cases"][0]["Fixture"]["frame_order_scope"] = Value::from("multi_victim_same_tick");
    g["cases"][0]["Fixture"]["amulet_shape"] = Value::from(1);
    g["cases"][0]["Receivers"] = receivers(&tail);
    let mk_r = |t: &str| {
        let mut r = synth_poison_rust(label, serde_json::json!([1, 2]));
        r["cases"][0]["Receivers"] = receivers(t);
        r
    };
    assert!(
        verify_union(&g, &mk_r(&tail)).is_ok(),
        "基准（100.tail 两侧同值）必须绿：{:?}",
        verify_union(&g, &mk_r(&tail))
    );
    let err = verify_union(&g, &mk_r(&mutated)).unwrap_err();
    assert!(
        err.iter().any(|e| e.contains("多重集")),
        "100.tail 改一个字符必须红：{err:?}（golden={tail:?} rust={mutated:?}）"
    );
}

/// **⑨ marker 白名单负例牙（D）**：把 `frame_order_scope = multi_victim_same_tick`
/// 塞进**非白名单** stem ⇒ 比较器入口**必须 panic**（放宽通道不能靠"没人写错"守）。
///
/// 白名单自检在读取台账**之前**执行，所以这里不需要任何 ledger 文件：
/// `real_ledger_verify_labels_opts("fireball", …)` 应在第一道断言处直接 panic，错误串点名白名单。
#[test]
fn tooth_poison_marker_whitelist_rejects_foreign_stem() {
    let mut g = synth_golden("s1", serde_json::json!([1, 2]), Some("cover_drain"));
    g["cases"][0]["Fixture"]["frame_order_scope"] = Value::from("multi_victim_same_tick");
    g["cases"][0]["Fixture"]["amulet_shape"] = Value::from(1);
    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        real_ledger_verify_labels_opts("sinsu", &g, &["s1"], false);
    }));
    let payload = res.expect_err("非白名单 stem 带顺序放宽 marker 必须 panic");
    let msg = payload
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| payload.downcast_ref::<&str>().map(|s| (*s).to_string()))
        .unwrap_or_default();
    assert!(
        msg.contains("只允许 fire / skill10"),
        "必须 panic 在白名单自检：{msg}"
    );
}

/// **MG-12/40 `tick_cap`（2026-11-26）**：一次施法 >300 ⇒ `RM_MAGHEALING` 累计 else 支置 300。
#[test]
#[ignore = "需先跑 game-svr 驱动（tests::healing_nonzero_parity_cap）生成真实台账"]
fn real_rust_ledger_healing_nonzero() {
    let golden = golden_file("healing-nonzero.json");
    let labels = golden_labels(&golden, "healing-nonzero");
    real_ledger_verify_labels("healing-nonzero", &golden, &labels);
}

// ---------------------------------------------------------------------------
// MG-44（32 圣言术）/ MG-48（50 魔爆）：ServerConf 十键批次
// ---------------------------------------------------------------------------

/// **`MG-44`（32 圣言术）**：三档目标等级（43/44/45，部署阈值 44 的严格小于口径）。
/// 金标准 `golden/magic/turnundead.json`；驱动 `tests::turnundead_parity_*`。
#[test]
#[ignore = "需先跑 game-svr 驱动（tests::turnundead_parity_*）生成真实台账"]
fn real_rust_ledger_turnundead() {
    // **标签从 golden 取**（MG-44 起含六臂；缺台账即防空转红）。
    let golden = golden_file("turnundead.json");
    let labels: Vec<String> = golden["cases"]
        .as_array()
        .map(|cs| {
            cs.iter()
                .filter_map(|c| c["Label"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    assert!(!labels.is_empty(), "[turnundead] 金标准无 case（防空转）");
    let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
    real_ledger_verify_labels("turnundead", &golden, &refs);
}

/// **`MG-48`（50 魔爆）**：玩家目标（`Race == Play` ⇒ STONE 分支可达）。
/// 金标准 `golden/magic/mabmabe.json`；驱动 `tests::mabmabe_parity_*`。
#[test]
#[ignore = "需先跑 game-svr 驱动（tests::mabmabe_parity_*）生成真实台账"]
fn real_rust_ledger_mabmabe() {
    let golden = golden_file("mabmabe.json");
    let labels = golden_labels(&golden, "mabmabe");
    real_ledger_verify_labels("mabmabe", &golden, &labels);
}

/// **MG-48 配置变体**（`rand0/rand1/time10`）：标签从各自 golden 取。
fn ledger_variant_stem(stem: &str, file: &str) {
    let golden = golden_file(file);
    let labels = golden_labels(&golden, stem);
    real_ledger_verify_labels(stem, &golden, &labels);
}

#[test]
#[ignore = "需先跑 game-svr 驱动（tests::mabmabe_rand0_parity_*）生成真实台账"]
fn real_rust_ledger_mabmabe_rand0() {
    ledger_variant_stem("mabmabe-rand0", "mabmabe-rand0.json");
}

#[test]
#[ignore = "需先跑 game-svr 驱动（tests::mabmabe_sr30_parity_*）生成真实台账"]
fn real_rust_ledger_mabmabe_sr30() {
    // **MG-48 follow-up（2026-11-28，① 档）**：`MabMabeHitSucessRate=30` 非缺省副本——
    // 行为面点亮（命中骰模数由配置来）。
    ledger_variant_stem("mabmabe-sr30", "mabmabe-sr30.json");
}

#[test]
#[ignore = "需先跑 game-svr 驱动（tests::mabmabe_rand1_parity_*）生成真实台账"]
fn real_rust_ledger_mabmabe_rand1() {
    ledger_variant_stem("mabmabe-rand1", "mabmabe-rand1.json");
}

/// **MG-47（35 `SKILL_WINDTEBO`）**：`GetPoseCreate` 前格 target 被推 ⇒ 逐格 `SM_BACKSTEP(9)`。
/// golden `golden/magic/windtebo.json`（s1/s5/s13）；驱动 `tests::windtebo_parity_*`。
#[test]
#[ignore = "需先跑 game-svr 驱动（windtebo_parity_*）生成真实台账"]
fn real_rust_ledger_windtebo() {
    real_ledger_verify_labels(
        "windtebo",
        &golden_file("windtebo.json"),
        &["s1", "s5", "s13"],
    );
}

/// AntiMagic 等号边界覆盖守卫的共用体（MG-10 / MG-11 各一份 golden）。
///
/// oracle（`MagicManager.cs:180`）是 `AntiMagic <= Random(10)`，Rust `anti_magic_roll` 是
/// `random_below(10) >= anti_magic` —— **等号通过**。断言 golden 三支的首抽与阈值关系：
/// `miss`：`Rng[0] < anti_magic` 且 HP 不变；`edge`：`Rng[0] == anti_magic`（等号）且 HP 下降；
/// `hit`：`Rng[0] > anti_magic` 且 HP 下降。
///
/// 为什么需要：等号边界是「`>=` 写成 `>`」这类变异的唯一判据；golden 重出时若丢了这个边界
/// （三支全 `>`），产品侧把等号改坏的变异就再也咬不住。
fn assert_antimagic_boundary(g: &Value, stem: &str) {
    let cases = as_array(g, "cases").expect("cases");
    let mut checked = 0;
    for case in cases {
        let label = case.get("Label").and_then(Value::as_str).unwrap_or("?");
        let anti = case
            .pointer("/Fixture/anti_magic")
            .and_then(Value::as_i64)
            .unwrap_or_else(|| panic!("[{stem}/{label}] 缺 Fixture.anti_magic"));
        let draw = as_array(case, "Rng")
            .expect("Rng")
            .first()
            .and_then(Value::as_i64)
            .unwrap_or_else(|| panic!("[{stem}/{label}] Rng 空：首抽必须是 AntiMagic 骰"));
        let before = case
            .get("TargetHpBefore")
            .and_then(Value::as_i64)
            .unwrap_or_else(|| panic!("[{stem}/{label}] 缺 TargetHpBefore"));
        let after = case
            .get("TargetHp")
            .and_then(Value::as_i64)
            .unwrap_or_else(|| panic!("[{stem}/{label}] 缺 TargetHp"));
        match label {
            "miss" => {
                assert!(
                    draw < anti,
                    "[{stem}/{label}] 首抽 {draw} 必须 < anti_magic {anti}"
                );
                assert_eq!(before, after, "[{stem}/{label}] HP 必须不变");
            }
            "edge" => {
                assert_eq!(
                    draw, anti,
                    "[{stem}/{label}] 首抽必须恰好等于 anti_magic（等号边界）"
                );
                assert!(after < before, "[{stem}/{label}] 过了等号边界必须掉血");
            }
            "hit" => {
                assert!(
                    draw > anti,
                    "[{stem}/{label}] 首抽 {draw} 必须 > anti_magic {anti}"
                );
                assert!(after < before, "[{stem}/{label}] 必须掉血");
            }
            // **MG-11 / 批 B（2026-11-26）**：非边界额外支的**显式白名单**——这两支不参与
            // AntiMagic 等号边界判定；本守卫仍要求**恰 3 个**边界支（miss/edge/hit），
            // 未知 label 仍 panic（将来加第 4 个边界支会被这里挡住）。
            "monster" | "monster_undead" => {}
            other => {
                panic!(
                    "[{stem}/{label}] 未知 case label {other:?}（本守卫只认 miss/edge/hit 与非边界白名单 monster/monster_undead）"
                )
            }
        }
        if !matches!(label, "monster" | "monster_undead") {
            checked += 1;
        }
    }
    assert_eq!(checked, 3, "[{stem}] 三支（miss/edge/hit）必须齐全");
}

/// **MG-10 边界覆盖守卫（2026-11-25）**：`fireball.json`（id=1）三支压在 AntiMagic 等号边界上。
#[test]
fn fireball_golden_antimagic_boundary_is_covered() {
    assert_antimagic_boundary(&golden(), "fireball");
}

/// **MG-11 边界覆盖守卫（2026-11-25）**：`fireball-5.json`（id=5，与 id=1 共用实现臂）同口径。
#[test]
fn fireball5_golden_antimagic_boundary_is_covered() {
    assert_antimagic_boundary(&golden_file("fireball-5.json"), "fireball-5");
}

/// **MG-19 边界覆盖守卫（2026-11-26）**：`lightning-11.json`（魔号 11）三支压在 AntiMagic 等号边界上。
#[test]
fn lightning11_golden_antimagic_boundary_is_covered() {
    assert_antimagic_boundary(&golden_file("lightning-11.json"), "lightning-11");
}

/// **MG-18 补验（2026-11-26）**：`frame_order_scope` 白名单负例——
/// 标记落在别的 stem 必须被点名拒绝；允许的 stem+magic_id 组合放行；stem 对但 magic_id 错同样拒绝。
#[test]
fn frame_order_scope_marker_is_refused_outside_the_whitelist() {
    let mk = |mid: i64| {
        serde_json::json!({
            "magic_id": mid,
            "cases": [{ "Fixture": { "frame_order_scope": "multi_victim_same_tick" } }]
        })
    };
    let err =
        check_frame_order_scope(&mk(10), "fireball").expect_err("fireball 不在白名单 ⇒ 必须拒绝");
    assert!(err.contains("只允许"), "{err}");
    assert!(
        check_frame_order_scope(&mk(10), "skill10").is_ok(),
        "skill10+10 应放行"
    );
    let err = check_frame_order_scope(&mk(10), "fire").expect_err("fire 的 magic_id 必须是 9");
    assert!(err.contains("magic_id 不一致"), "{err}");
}

/// **MG-18 P3（2026-11-26）**：仓内 **committed golden 全量扫**——`frame_order_scope =
/// multi_victim_same_tick` 只许出现在白名单 stem 且 `magic_id` 对得上。
///
/// 与 `tooth_poison_marker_whitelist_rejects_foreign_stem`（合成输入 ⇒ 必须 panic）互补：
/// 合成牙证明「塞错会红」；本扫把**仓里实际提交的 golden**放进同一判据，并在**默认门**跑
/// （不是只有那个 stem 的 ignored 比较器才查）。防空转：必须扫到 ≥2 支带标记的 stem。
#[test]
fn committed_goldens_obey_frame_order_scope_whitelist() {
    let dir = format!("{}/golden/magic", env!("CARGO_MANIFEST_DIR"));
    let mut marked: Vec<String> = Vec::new();
    for entry in std::fs::read_dir(&dir).expect("golden 目录必须存在") {
        let path = entry.expect("目录项可读").path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .expect("文件名必须是 UTF-8")
            .to_string();
        let text = std::fs::read_to_string(&path).expect("读 golden");
        let g: Value = serde_json::from_str(&text).expect("golden 必须是合法 JSON");
        let has_marker = g["cases"].as_array().is_some_and(|cs| {
            cs.iter().any(|c| {
                c["Fixture"]["frame_order_scope"].as_str() == Some("multi_victim_same_tick")
            })
        });
        if has_marker {
            check_frame_order_scope(&g, &stem).unwrap_or_else(|e| panic!("[{stem}] {e}"));
            marked.push(stem);
        }
    }
    assert!(
        marked.len() >= 2,
        "防空转：带 `multi_victim_same_tick` 的 golden 只扫到 {marked:?}（全量扫没扫到东西？）"
    );
}

/// **MG-27 边界覆盖守卫（2026-11-26）**：`firecharm.json`（魔号 13）三支压在 AntiMagic 等号边界上
/// （oracle `MagicManager.cs:334` 与 1/5/11 同式 `Random(10) >= AntiMagic`）。
#[test]
fn firecharm_golden_antimagic_boundary_is_covered() {
    assert_antimagic_boundary(&golden_file("firecharm.json"), "firecharm");
}
