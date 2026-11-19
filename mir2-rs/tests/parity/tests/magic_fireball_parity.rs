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
//! 3. **逐接收者逐字段**：RM(`ident/wParam/p1/p2/p3/priority/late`) 与 SM(`ident/param/tag/series/tail`)
//!    按序逐项相等；
//! 4. **HP 一级断言**：`TargetHpBefore/TargetHp` 两侧相等（miss 100→100、edge/hit 100→85 由金标准决定）；
//! 5. **RNG**：行数相等、逐次取值相等、`seq` 连续无缺号、**每一行 `site` ∈ 允许清单**（§16b）。

use serde_json::Value;

/// `site` 允许清单（§16b 表格的代码形态）：`(前缀, 理由)`。
/// **扩充必须在此逐条写理由并进评审 diff**——不允许"能过就加"。
pub const SITE_ALLOWLIST: &[(&str, &str)] = &[
    ("crates/world-bevy/src/magic.rs", "原版 MagicManager 的威力/骰门/推挤纯函数取数点"),
    ("crates/world-bevy/src/magic_effect.rs", "魔法效果计算（AntiMagic 骰门、GetPower*）"),
    ("crates/world-bevy/src/world.rs", "cast_* 施法效果体，对应 DoSpell 的 case 体"),
    ("crates/world-bevy/src/slave.rs", "deliver_delay_magic_impl：RM_DELAYMAGIC 到期结算"),
    ("crates/world-bevy/src/combat.rs", "GetAttackPower 的随机区间"),
    ("bins/game-svr/src/lib.rs", "宿主侧施法接线（帧参数/训练尾）"),
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
    v.get(k).and_then(Value::as_i64).unwrap_or(i64::MIN)
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

            // 3) 逐字段
            let rm_fields = ["ident", "wParam", "p1", "p2", "p3", "priority", "late"];
            let sent_fields = ["ident", "param", "tag", "series"];
            for (k, fields) in [("Rm", &rm_fields[..]), ("Sent", &sent_fields[..])] {
                let ga = as_array(gr, k).cloned().unwrap_or_default();
                let ra = as_array(rr, k).cloned().unwrap_or_default();
                if ga.len() != ra.len() {
                    errs.push(format!(
                        "[{label}/{who}] {k} 条数不等：golden={} rust={}",
                        ga.len(),
                        ra.len()
                    ));
                    continue;
                }
                for (i, (g, r)) in ga.iter().zip(ra.iter()).enumerate() {
                    for f in fields {
                        if num(g, f) != num(r, f) {
                            errs.push(format!(
                                "[{label}/{who}] {k}[{i}].{f} 不等：golden={} rust={}",
                                num(g, f),
                                num(r, f)
                            ));
                        }
                    }
                    if k == "Sent" && text(g, "tail") != text(r, "tail") {
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
// 比较器自检：金标准逐支喂回（identity）+ 三条篡改必须红 + 防空转必须红。
// ---------------------------------------------------------------------------
fn golden() -> Value {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/golden/magic/fireball.json");
    let text = std::fs::read_to_string(path).expect("金标准必须存在（缺文件即红）");
    serde_json::from_str(&text).expect("金标准必须是合法 JSON")
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
    assert_eq!(verify(&g, &r), Ok(()), "identity 必须绿（比较器自身不假红）");
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

#[test]
fn tooth_missing_rng_line_red() {
    let g = golden();
    let mut r = ledger_from(&g);
    let cases = r.get_mut("cases").and_then(Value::as_array_mut).unwrap();
    let rng = cases[0].get_mut("Rng").and_then(Value::as_array_mut).unwrap();
    rng.pop();
    let err = verify(&g, &r).unwrap_err();
    assert!(
        err.iter().any(|e| e.contains("RNG 行数不等")),
        "必须红在总量：{err:?}"
    );
}
