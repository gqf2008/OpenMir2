# spell_cast 残余真差异分诊：ident 53（SM_HEALTHSPELLCHANGED）×3 — 交付报告

卡：`openmir2-compare-ident53-diff-triage` · **tier=R（判不准按 R）** ·
owner：openmir2-worker-74 · 分支：`card/compare-ident53-diff-triage`
（base = walgit/master `fbd2fa306`）· 2026-10-24
分诊结论已先签入卡 comment（oid `fa6e9de712eb416f508c5c1b1a5cd3b36715ae5e`）。

## 0. 结论摘要

| # | frame_index | ident | kind/field | oracle | rust | 分诊 |
|---|---|---|---|---|---|---|
| 1 | 30 | 53 | body_len | 0 | 16 | **D-9 故意偏离 + 窗口 pin 丢失**（运行环境缺口，非 src 缺陷） |
| 2 | 32 | 53 | head tag | 13 | 15 | **MP 数值/时序分歧**（tier=R，窗口复跑定位） |
| 3 | 32 | 53 | body_len | 0 | 16 | 同 #1 |

处置：**不动实现、不碰 `rules.json`**（53 条目 worker-58 起草 `action=compare` fail-closed，
该红仍红——本卡没有也不许把它掩掉/ignore）；差异表归档本报告 + 卡 comment；
`selftest_impl.py` 新增 ⑦ 断言把 3 处 diff 的 anatomy 逐处钉死（任何漂移/掩码化必红）。
`src/**`、C#、场景 JSON、`rules.json`、`card_window.py` 零改动。

## 1. ① 取证

### 1.1 常量与语义

**`SM_HEALTHSPELLCHANGED = 53`**（`src/OpenMir2/Messages.cs:211`）。
头部口径：`recog`=发送者 ActorId（全局掩码）、**`param`=HP、`tag`=MP、`series`=MaxHP**
（全部 u16 逐字节比）、**body = 0B**（oracle `SendSocket(ClientMsg)` 无尾）。

### 1.2 oracle（C#）发射链

```text
CM_SPELL(3017) → PlayObject.Message.cs:1339-1341 → ClientSpellXY（PlayObject.Attack.cs:1054）
  → PlayObject.DoSpell（PlayObject.Base.cs:1505-1531，default 支、非战士技）
     GetSpellPoint（:1369-1372）= Round(Spell/4×(Level+1)) + DefSpell = Round(4/4×1)+1 = 2
     :1514  MP < 2 ⇒ return false（蓝门，本场景过）
     :1518  DamageSpell(2)（BaseObject.cs:1383-1405，直写 WAbil.MP）
     :1519  HealthSpellChanged()（BaseObject.cs:700-709；Race==Play ⇒ SendUpdateMsg
            入自己队列，BaseObject.Message.cs:283-305 先去重同 ident 再入队）
  → MagicManager.DoSpell（成功支 ：648-658 RM_MAGICFIRE / 失败支距离门 Attack.cs:1278-1281
     RM_MAGICFIREFAIL）
出队（PlayObject.Message.cs:1736-1739）：
  MakeMessage(SM_HEALTHSPELLCHANGED, processMsg.ActorId, WAbil.HP, WAbil.MP, WAbil.MaxHP)
  + SendSocket(ClientMsg)   // 无 body；出队时才读 WAbil
```

### 1.3 rust 发射点

- **施法 wrapper**：`mir2-rs/bins/game-svr/src/lib.rs:10407-10451`——`:10415` 蓝门早退、
  `:10426` 先扣 `sg.state.wabil.mp`（saturating_sub）、`:10429-10440` 后发
  `sm_healthspellchanged_frame(...)`（magic 1 不属 6/48/9/10/22/36 延后集 ⇒ 立即发）；
  该 wrapper 18c7ffe56（2026-10-17）接线，run head `a7fbe4a33` 与现 master 逐字同。
- **帧布局**：`smframe.rs:262-266` `sm_healthspellchanged` = oracle header-only；
  `:325-333` `sm_healthspellchanged_client` = D-9 冻结客户端布局（尾
  `EDCode(12B TCharDesc{Feature=HP, Status=MP, StatusEx=MaxHP})`，线缆 16 字符）；
  `:340-346` 生产选路 = `client_layout_enabled()`（`MIR2_CLIENT_ABILITY_LAYOUT`，未设 ⇒ 客户端布局）。
- **回复（stamina）**：`bins/game-svr/src/stamina.rs` 逐行复刻 `BaseObject.Run()`
  体力段（50ms 重放、`SpellTick += 2/Run`、`SpellFillTime=800`、MP += MaxMP/18+1）。

### 1.4 基线事实（判值锚点）

PlayerId 6228（winplay0）：**HP=16 / MP=17 / MaxHP=16 / MaxMP=18**——
双侧 dbsnap `characters_ablity.tsv` 一致（oracle 行 ModifyTime=2026-10-22 未回存；
rust 行 2026-10-24 已回存 MP=17），且 rust 入图日志
`E:/tmp/conformance/rust-logs/game-svr.out.log` 05:57:56.508
`WAbil[hp=16 mp=17 max_hp=16 max_mp=18]` 三方互证。
施法间隔（window-run-1）：oracle 两发 probe-t 4507→6820（2313ms）、rust 2241→4557（2316ms）。

### 1.5 3 处 diff 逐字节解剖

| # | frame_index（对） | oracle 字节/字段 | rust 字节/字段 | 语义 |
|---|---|---|---|---|
| 1 | 30（oracle seq32 ↔ rust seq31，cast 1） | `body_len=0`、`body_hex=""` | `body_len=16`、`body_hex=6864686b646768686464786b6464686b`，解码 = `10000000 0f000000 10000000` = TCharDesc{16,15,16} | oracle header-only vs rust D-9 客户端尾（内容恰为 HP/MP/MaxHP 三 ×i32） |
| 2 | 32（oracle seq35 ↔ rust seq34，cast 2） | `tag=13` | `tag=15` | **MP**：oracle 17→15→13（两发各扣 2，窗口内无回复）；rust 两帧均 15（body 尾解码同为 {16,15,16}） |
| 3 | 32 | `body_len=0` | `body_len=16` | 同 #1 |

（两侧帧数对齐：各 2 条 ident 53，按序配对；recog=会话 ActorId 已由全局掩码处理，不参与判定。）

## 2. ② 分诊与 tier 依据

### 2.1 body_len ×2（#1/#3）＝ D-9 已裁决偏离 + 窗口 pin 丢失

- D-9（`mir2-rs/tests/parity/whitelist.md` D 线）：冻结客户端 `HIGHHP=True`
  （`Grobal2.pas:39`）从**尾**解 53 的 `TCharDesc`，oracle header-only 在真机客户端上
  HUD 归零/拒跑 ⇒ **oracle 布局与冻结客户端不可同时满足**，owner 2026-10-15 裁决
  生产默认取客户端布局、对拍显式 pin `MIR2_CLIENT_ABILITY_LAYOUT=0`。
- 本窗口（window-run-1，`card-window.json`：git_head=`a7fbe4a33`、rust 二进制
  sha256=`dfc47b0e…` = 现 `target/release/mir2-server.exe`）**profile 确有**
  `sides.rust.env.MIR2_CLIENT_ABILITY_LAYOUT=0`，但起栈链把它丢了：
  `game-svr.out.log 05:57:06.620 [INF] CLIENT_ABILITY_LAYOUT layout=client-58（MIR2_CLIENT_ABILITY_LAYOUT=<unset>）`
  ——`start-rust-warmup.ps1` 用 WMI `Win32_Process.Create` 派生（不继承调用方 env），
  且 `start_mir2_server.ps1` 只注 `MIR2_RNG_SEED`/`MIR2_DB_NAME`。
- ⇒ **运行环境缺口，不是 src 缺陷**；不修 src（改生产默认 = 推翻 owner 的 D-9 裁决，
  且违反客户端冻结硬约束）。rules.json 53 条目按设计 fail-closed 该红仍红。
  修属窗口工具链（env 转发/WMI 适配），不在本卡写集，建议另开卡。

### 2.2 head tag ×1（#2）＝ MP 数值/时序分歧 ⇒ tier=R

- oracle 侧与静态模型**逐位吻合**：基线 MP=17 → cast1 扣 2 发 15 → cast2 扣 2 发 13；
  窗口内无回复可证：`SpellFillTime=800`（oracle Server.conf 实值）+ `SpellTick += 2/50ms Run`
  ⇒ 2.3s 间隙 tick ≈ 155 ≪ 800，一次回复都不会发生。
- rust 侧**第 1 帧吻合**（15 = 17−2），**第 2 帧不符**：按现 master 源码（wrapper 先扣后发 +
  stamina 模型同上）cast 2 应发 13，实发 15；且软关存档把 **MP=17（基线值）** 写回
  （`characters_ablity` rust 行 ModifyTime=2026-10-24）。三事实（15/15 帧 + 终库 17）
  与现源码静态推演（13/13 + 终库 13）矛盾，run1 二进制又确含 wrapper 代码
  （18c7ffe56 特征串「无地面会话」在 exe 内）⇒ **按现源码不可静态定位成因**
  （可能面：会话/地面双份 WAbil 同步、tick 时序、或窗口二进制与 head 的隐性偏差）。
- 触**数值（MP 扣减/回复/持久化）+ 时序** ⇒ 按判级政策属 R；机制判不准 ⇒ 「判不准按 R」。
- **处置**：不动实现；差异归档（本报告 §1.5 + 卡 comment）；窗口复跑定位
  （前置：修 pin 转发链、二进制与 head 对齐、复跑后核 53 序列与终库 MP）。
  该项 `window_required: true`。

## 3. ③ 牙（均实测、均还原）

新增 `selftest_impl.py` **控制帧 ⑦**：把残余 diffs 的 anatomy 钉成
`[(30,body_len,body_len,0,16), (32,body_len,body_len,0,16), (32,head,tag,13,15)]`
（值/序/处数任一漂移即红；掩码化 53 会先让 ⑤/⑥ 红）。恢复后
`compare_runs selftest 296/296 GREEN`（承前 295 ⇒ +1）。

| 牙 | 变异 | 实测 |
| --- | --- | --- |
| M-A（只有 ⑦ 能抓的值漂移） | rust seq34（cast 2 帧）`tag 15→14`：仍 3 处 diff、⑥ 仍绿 | ⑦ **RED**，anatomy 精确显示 `(32,'head','tag',13,14)` |
| M-B（结构性漂移） | rust seq31（cast 1 帧）`tag 15→13`：新增 1 处 ⇒ 4 处 | ⑥ + ⑦ **双 RED**，⑥ 列出 4 项 |

两轮均 `git checkout` 还原，`frames.ndjson` sha256 复测 = 库内 `0eafd0d76187a0f5…` 逐字节一致。

## 4. ④ 门禁（本卡 head 实跑）

| 门禁 | 结果 |
| --- | --- |
| `python -I -B tools/conformance/compare_runs.py selftest` | **GREEN 296/296，rc=0** |
| `python -I -B tools/conformance/card_window.py --selftest` | **GREEN 256/256，rc=0** |
| `python -I tools/hygiene/conflict_marker_gate.py` | OK（0 markers / 7488 tracked files），rc=0 |
| `python -I tools/hygiene/stale_pointer_gate.py` | OK，rc=0 |
| `python -I tools/hygiene/dynamic_count_stamp_gate.py` | OK（0 缺戳），rc=0 |
| `git diff --check` | 干净，rc=0 |
| Rust 门禁（fmt/clippy/test/msgcodegen/t_reservation_gate/whitelist 核查） | **不适用**：`git diff --stat` 仅 `tools/conformance/selftest_impl.py` + 本文档，零 `src/**`/`mir2-rs/**`/`rules.json` 触碰 |

## 5. 写集与残留物

- 改动：`tools/conformance/selftest_impl.py`（⑦ 注释 + 断言，+27 行）、本文档。
- 未碰：`rules.json`（53 条目保持 worker-58 草稿原样，`approved_by` 仍 PENDING）、
  `src/**`、C#、场景 JSON、`card_window.py`、fixture（只读 + 变异即还原）。
- 残留物：`E:/tmp/w74-triage-comment.json`（comment body 中转，可删）。
- 移交窗口项（非本卡）：① 起栈链 env 转发修复（WMI 适配/`start_mir2_server.ps1`）；
  ② spell_cast 复跑（pin 生效）后 53 应余 tag 1 处，再按其结果定位 MP 分歧成因；
  ③ rules.json 53 条目的 owner 复核（PENDING）。
