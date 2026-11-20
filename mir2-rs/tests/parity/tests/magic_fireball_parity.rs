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
//! 3. **RM 多重集**：RM(`ident/wParam/p1/p2/p3/priority/late`) **排序后按多重集**相等
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
        for k in ["TargetHpBefore", "TargetHp"] {
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
            // 2) 负向牙②：obs_out 的 RM 与 SM 均必须为空
            if who == "obs_out" {
                for k in ["Rm", "Sent"] {
                    let len = as_array(gr, k).map(|a| a.len()).unwrap_or(0);
                    if len != 0 {
                        errs.push(format!(
                            "[{label}] 负向牙②：obs_out 的 {k} 有 {len} 条（应为 0）"
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
            // 比较面仍是**全部七个字段**，只对顺序免疫；缺/多条目逐条报出。
            let rm_fields = ["ident", "wParam", "p1", "p2", "p3", "priority", "late"];
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
                    "[{label}/{who}] Rm 多重集不等（ident,wParam,p1,p2,p3,priority,late）：                     golden 有 rust 无={missing:?} rust 有 golden 无={extra:?}"
                ));
            }

            // 3b) SM：**严格顺序** + 逐字段 + 尾串（玩家可见的真实发送顺序）。
            let sent_fields = ["ident", "param", "tag", "series"];
            let ga = as_array(gr, "Sent").cloned().unwrap_or_default();
            let ra = as_array(rr, "Sent").cloned().unwrap_or_default();
            if ga.len() != ra.len() {
                // 逐帧报缺/多（不打印整表；逐字段比较留给等长分支）。
                let sent_key = |v: &Value| -> (i64, i64, i64, i64, String) {
                    (
                        num(v, "ident"),
                        num(v, "param"),
                        num(v, "tag"),
                        num(v, "series"),
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
                    for f in sent_fields {
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

        // 5) RNG：数量 / 逐次 / site 清单 / seq 连续
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
    // 有伤害时：`SM_STRUCK(31)` 的 `param` 是扣血后的 HP、`series` 是本次伤害
    // ⇒ `param + series` 必须回到施法前值（= `Fixture.target_hp`）。
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
        for f in r
            .get("Sent")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if f.get("ident").and_then(Value::as_i64) != Some(31) {
                continue;
            }
            let hp_after = f.get("param").and_then(Value::as_i64).unwrap_or(i64::MIN);
            let dmg = f.get("series").and_then(Value::as_i64).unwrap_or(0);
            let pre = hp_after.saturating_add(dmg);
            if pre != target_hp {
                return Err(format!(
                    "[{label}] 31 帧 `HP_after({hp_after})+damage({dmg})` = {pre} ≠ `Fixture.target_hp` {target_hp}——`target_hp` 必须是**施法前**输入（§8.3d）；**疑似输入/输出混淆**（施法后值当输入，帧面会整体错位且伪装成真分歧）"
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

fn real_ledger_verify_labels(stem: &str, golden: &Value, labels: &[&str]) {
    let dir = ledger_dir();
    let magic_id = golden.get("magic_id").cloned().unwrap_or(Value::from(0));
    // **不**把金标准顶层 `anti_magic` 搬进比较对象：该字段是构造参数（`fireball` 顶层仍是 3），
    // 生效值在**逐 case** 的 `Fixture.anti_magic`（登录序 1），已由 `check_golden_contract` 断言。
    // 搬过来会把金标准与自身比较（恒等），制造"查过"的假象。
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
    if let Err(errs) = verify(golden, &rust) {
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
/// 判定面（标签 s1/s5/s13；金标准 `golden/magic/lightflower.json`，C# oracle 生成）。
#[test]
#[ignore = "需先跑 game-svr 三支驱动（tests::lightflower_parity_*）生成真实台账"]
fn real_rust_ledger_lightflower() {
    real_ledger_verify_labels(
        "lightflower",
        &golden_file("lightflower.json"),
        &["s1", "s5", "s13"],
    );
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
    real_ledger_verify_labels(
        "bighealing",
        &golden_file("bighealing.json"),
        &["heal", "full", "both"],
    );
}

/// **`18 隐身术`（`SKILL_CLOAK`）**真实台账 → 判定面（标签 s1/s5/s13）。
#[test]
#[ignore = "需先跑 game-svr 三支驱动（tests::cloak_parity_*）生成真实台账"]
fn real_rust_ledger_cloak() {
    real_ledger_verify_labels("cloak", &golden_file("cloak.json"), &["s1", "s5", "s13"]);
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
    real_ledger_verify("hbfireball", &golden_file("hbfireball.json"));
}

/// **`13 火符`**真实台账 → 判定面（金标准 `golden/magic/firecharm.json`）。
#[test]
#[ignore = "需先跑 game-svr 三支驱动（tests::firecharm_parity_*）生成真实台账"]
fn real_rust_ledger_firecharm() {
    real_ledger_verify("firecharm", &golden_file("firecharm.json"));
}

/// **`2 治愈`**真实台账 → 判定面（标签 = `heal` / `full` / `self`；本支无 AntiMagic 骰）；
/// 金标准 `golden/magic/healing.json`。
#[test]
#[ignore = "需先跑 game-svr 三支驱动（tests::healing_parity_*）生成真实台账"]
fn real_rust_ledger_healing() {
    real_ledger_verify_labels(
        "healing",
        &golden_file("healing.json"),
        &["heal", "full", "self"],
    );
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
    real_ledger_verify("lightning11", &golden_file("lightning-11.json"));
}

#[test]
#[ignore = "需先跑 game-svr 三支驱动（tests::lightning45_parity_*）生成真实台账"]
fn real_rust_ledger_lightning45() {
    real_ledger_verify("lightning45", &golden_file("lightning-45.json"));
}

/// **id=5（`SKILL_FIREBALL2`）真实台账 → 判定面**；金标准 `golden/magic/fireball-5.json`。
#[test]
#[ignore = "需先跑 game-svr 三支驱动（tests::fireball5_parity_*）生成真实台账"]
fn real_rust_ledger_fireball5() {
    real_ledger_verify("fireball5", &golden_file("fireball-5.json"));
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

/// `late` 列（RM 的 `LateDelivery`）必须真的进比较面：把一条 RM 的 `late` 翻转 ⇒ 红。
#[test]
fn tooth_rm_late_changed_red() {
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
    let old = rm[0].get("late").and_then(Value::as_bool).unwrap_or(false);
    rm[0]["late"] = Value::Bool(!old);
    let err = verify(&g, &r).unwrap_err();
    assert!(
        err.iter().any(|e| e.contains("Rm 多重集不等")),
        "必须红在 RM 多重集（late 列）：{err:?}"
    );
}

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
