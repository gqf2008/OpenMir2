# 「绿 ≠ 已验」：三类实例与统一口径（2026-11-20）

> 起因：同一批（MG-58/60/61/62/63）里同时出现三种「比较器绿了、但某一面其实没验」的情形。
> 本文把判据归并成一条口径 + 一份可复跑的机判，供后续收 green 时逐条过。
> 只在文档层面立规，**未改任何比较器/产品代码**。

## 0. 一句话口径

比较器为绿，只说明「**它真的比过的那些字段**在两侧相等」。一个面要算**已验**，必须同时满足：

1. **两侧都有该键**——缺键默认不纳入比较（当前只有 `recog` 在比较器里有「全有或全无」断言，
   `due_ms_rel` **没有**这一层保护，缺就一直不比）；
2. **该面没被 `keep` / allowlist / 掩码剪掉**——剪掉的理由要写在比较器注释里并登记；
3. **期望值来自 oracle 的正确行为**——不是旧世代 golden、也不是「产品与 golden 同向错」的产物；
4. **到点窗口跨过**——落在 `NotDue` 里的帧不算交付，`NotDue` 本身也不进比较面。

任一条不满足 ⇒ 绿只证明了其它面，**不能**把它写成「该 leaf 已验」。

## 1. 实例 A：keep 过滤把 640 整帧剪掉（`sinsu`）

- **事实**：`golden/magic/sinsu.json` 逐帧带 `due_ms_rel`（16/16），但比较器 `real_rust_ledger_sinsu`
  用 `keep = {17, 638, 642}` 取帧 ⇒ **640 的载荷与时序都不比**。
- **理由（已登记在比较器注释）**：离线替身 `RegenMonsterByName` 造怪 **0 抽**，而 Rust
  `create_monster_oracle` 真抽 ⇒ RNG 流偏移，640 的 `tag`（训练 `Random(3)+1`）实测 golden=2 / Rust=3。
  长度与逐值都不可比。
- **结论**：sinsu 的 scope 必须写「640 不比」；绿**不覆盖**训练帧。这是「剪掉一面换绿」的合法形态，
  但必须在 scope 里写明，不能只写在比较器注释里。

## 2. 实例 B：旧世代 golden 缺 `due_ms_rel` 键（34/41 份）

- **事实（机判，见 §5 脚本）**：`golden/magic/*.json` 共 41 份，逐帧带 `due_ms_rel` 的只有 **7 份**——
  `fire`、`sinsu`、`skill49/51/52/53/54`；其余 **34 份完全没有该键**（`skeleton`、`shield`、`lightflower`、
  `hangmajinbub`、`dejiwonho`、`snowwind`、`groupdeding`、`bigcloak`、`uenhancer`…）。
- **比较器侧**：`verify` 的 `sent_fields` 只在 golden 首帧带该键时才把 `due_ms_rel` 纳入比较 ⇒
  缺键 ⇒ **时序面整列不比**，其余字段（ident/param/tag/series/tail，带 recog 时还有 recog）照比。
- **结论**：这 34 份的绿**不证明时序**。例如 `skeleton` 的 640 只证了载荷，没证「1000ms 后」；
  写 scope 时不能写成「帧面逐字段已比」。
- **补充风险**：`due_ms_rel` 没有「全有或全无」断言，理论上可以出现**部分帧带 due、部分不带**的
  golden 而无人察觉（当前实测是全有或全无，属"暂时安全"）。

## 3. 实例 C：产品缺陷未修 + golden 无该面 ⇒ 两个绿 = 一处未验

- **事实**：`shield`(31) / `lightflower`(24) / `hangmajinbub`(14) / `dejiwonho`(15) 的 640 在 Rust 是
  **立即发**（`finish_spell_tail(…, None)` 四处，属既有全局登记），oracle 是 `SendSelfDelayMsg(…, 1000)`。
- 这四份 golden **没有 `due_ms_rel` 键** ⇒ 640 的 due 不进比较面 ⇒ 现在**是绿的**。
  绿的原因不是"产品对了"，而是"这一面根本没比"。
- **怎样把它变成可观测（建议做法）**：在**修产品之前**，先按新 schema 重出这四份 golden（带 due）
  ⇒ 比较器会立刻红（golden 1000 vs Rust 0）——这就是"先把失效态造出来再修"。
  修好后再重出 ⇒ 绿，此时绿才代表该面已验。对应 `da` 的四处修复与收口卡
  `openmir2-magic-640-delay-sweep.md`。
- **反过来的坑**：只改产品、不重出 golden，比较器**既不会红、也不会变得更可信**——缺键仍是不比。

## 4. 正例（同批新增的五号，可作模板）

- `skill49/51/52/53/54.json`：**18/18 帧**带 `due_ms_rel`；比较器对四个收件人断**完整**
  `(ident, due_ms_rel)` 序列、**无 keep**；caster 另有负向断言「恰 0 条 17」。
- 因此 640 的 `due_ms_rel = 1000` 是**被判**的（牙 C「去 640」、牙 A「去 638」都能咬住）；
  同时 Train 块与 `Rng` 长度提供**第二条独立观测**（牙 B 同时动 640 与抽数）。

## 5. 收 green 时的五问（逐条回答，答不上来就别写「已验」）

1. 这一面**两侧都有键**吗？（`recog` / `due_ms_rel` / `tail` / 自定义列；注意只有 `recog` 有全有或全无断言）
2. 有 **keep / allowlist / 掩码**吗？在谁那里、理由是什么、有没有登记进 scope？
3. 期望值来自 oracle 的**正确行为**吗？（旧世代 golden？产品缺陷与 golden 同向？）
4. **到点窗口**跨过了吗？（`NotDue` 里的帧不算交付；`NotDue` 不给"没发"作证）
5. 有没有**第二条独立观测**？（本批 = 帧面 + `Train` 块 + `Rng` 长度）

## 6. 机判脚本（只读，随时可复跑）

统计每份 golden 的 `due_ms_rel` 覆盖率，以及各 stem 的 caster 640 及其 due：

```bash
python - <<'PY'
import json, glob, os
full = none = partial = 0
for f in sorted(glob.glob("mir2-rs/tests/parity/golden/magic/*.json")):
    g = json.load(open(f, encoding="utf-8"))
    n = t = 0; c640 = []
    for c in g.get("cases", []):
        for r in c.get("Receivers", []):
            for s in r.get("Sent", []):
                t += 1; n += ("due_ms_rel" in s)
                if s.get("ident") == 640 and r.get("Who") == "caster":
                    c640.append((c.get("Label"), s.get("due_ms_rel", "<NO-KEY>")))
    if t == 0:
        continue
    if n == t: full += 1
    elif n == 0: none += 1
    else: partial += 1
    # **每份都打一行**（不按条件过滤）：否则「全带 due 但无 caster 640」的件不会出现，
    # 读者就无法从输出反推 FULL 集合是否完整（见下面不变量）。
    print(f"{os.path.basename(f)}: due {n}/{t}; caster640={c640}")
print(f"SUMMARY total={full+none+partial} FULL={full} NONE={none} PARTIAL={partial}")
PY
```

判读：`due n/t` 中 `n < t` ⇒ 该份的时序面只比了一部分（或整列不比）；`<NO-KEY>` ⇒ 该帧的 due 未验。

**不变量（读者可自检，不需要看别的文件）**：
- 末行 `FULL=<k>` **必须等于**输出里 `n==t` 的行数；`NONE=` 等于 `n==0` 的行数；`PARTIAL=` 等于 `0<n<t` 的行数；
- 三者之和必须等于 `total=`；`total=` 一般等于 `golden/magic/*.json` 的文件数——**若有零帧件会被 `t == 0` 跳过**（当前无零帧件，41 份全部计入），届时以 `total=` 与文件数的差额解释；
- 也就是说：**FULL 集合可以从输出逐行反推**，不存在"漏打印因而数不到"的情形。
  本条是 `3b` 对 §6 的 P3 修正：原脚本只在「有 caster 640 或 `n != t`」时打印，
  若将来出现「全带 due 但无 caster 640」的件，它会静默不出现。
- 实测（2026-11-20，41 份）：`SUMMARY total=41 FULL=7 NONE=34 PARTIAL=0`。

## 7. 挂账（都不是本批范围）

- **加一条 `due_ms_rel` 全有或全无断言**（与 `recog` 同款），放在 `real_ledger_verify_labels_opts`
  里；成本约十行，能消掉 §2 的"暂时安全"。
- **34 份旧 golden 是否重出**：按 `docs/issues/openmir2-golden-fixture-drift.md` 的世代规则处理，
  不要在与该 leaf 无关的批次里顺手重出（重出会改期望值，需自带牙与对照）。
- **实例 C 的四处**：归 `da` 的 `openmir2-magic-640-delay-sweep.md`；完工判据应包括「golden 带 due 键
  且比较器对该帧 due 判绿」。
