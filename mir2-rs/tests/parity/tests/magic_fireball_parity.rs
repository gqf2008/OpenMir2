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
/// 前置（三支必须**各自独立进程**，`rng_trace` 是进程级 `OnceLock`，见卡 §16c）：
/// ```text
/// cargo test -p game-svr --lib tests::fireball_parity_miss -- --exact --ignored --test-threads=1
/// cargo test -p game-svr --lib tests::fireball_parity_edge -- --exact --ignored --test-threads=1
/// cargo test -p game-svr --lib tests::fireball_parity_hit  -- --exact --ignored --test-threads=1
/// cargo test -p mir2-parity-tests --test magic_fireball_parity -- --ignored --exact real_rust_ledger_parity
/// ```
///
/// 防空转：任一份台账缺失/不是合法 JSON/不是恰 1 个 case/`Label` 不符 ⇒ 红（不静默跳过）。
/// **已知红（卡 `openmir2-magstruck-name-color` 未修前）**：obs_in 的
/// `SM_CHANGENAMECOLOR(656)`（Sent）与 `RM_CHANGENAMECOLOR(8090)`（Rm）——Rust
/// `deliver_mag_struck` 对玩家的 `name_color` 恒 false。修好后本用例应全绿。
#[test]
#[ignore = "需先跑 game-svr 三支驱动生成真实台账（命令见本用例文档注释/卡面 §22）"]
fn real_rust_ledger_parity() {
    let dir = ledger_dir();
    let mut cases = Vec::new();
    for label in ["miss", "edge", "hit"] {
        let path = dir.join(format!("fireball_{label}.json"));
        let text = std::fs::read_to_string(&path).unwrap_or_else(|e| {
            panic!(
                "防空转：缺真实台账 {}（{e}）。先跑 game-svr 三支驱动（命令见文档注释）",
                path.display()
            )
        });
        let v: Value = serde_json::from_str(&text).expect("防空转：台账必须是合法 JSON");
        let arr = v.as_array().expect("防空转：台账必须是 [case] 数组");
        assert_eq!(arr.len(), 1, "[{label}] 每支台账恰 1 个 case");
        assert_eq!(
            arr[0].get("Label").and_then(Value::as_str),
            Some(label),
            "[{label}] 台账 Label 不符"
        );
        cases.push(arr[0].clone());
    }
    let rust = serde_json::json!({"magic_id": 1, "anti_magic": 3, "cases": cases});
    if let Err(errs) = verify(&golden(), &rust) {
        panic!(
            "真实台账与金标准不等（{} 条）：\n{}",
            errs.len(),
            errs.join("\n")
        );
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
