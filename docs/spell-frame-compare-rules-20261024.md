# `spell_cast` 的最后一块判据：给 `rules.json` 补 53 / 638 / 639 三条比较规则

- 卡片：`openmir2-spell-frame-compare-rules`（tier=L）
- 作者：`openmir2-worker-58`（worktree `wt-worker-58`，分支 `card/spell-frame-compare-rules`）
- base = master `65d6e7134`（已含前置卡 `openmir2-spell-cast-window-blockers` 的合并 `12d720a19`）
- 写集：`tools/conformance/rules.json`（纯追加 3 条）+ 本文。**未改** `src/**`、场景 JSON、
  `whitelist.md`、`docs/window-batch-plan-20261023.md`；未起栈、未连库。
- 文中所有 `file:line` 均在本 head 现取（2026-10-24）。

## 0. 结论摘要

| 项 | 结果 |
|---|---|
| 补规则前（同一份最小 frames） | `DIFFS 0` + `UNDECIDABLE 3` ⇒ **`VERDICT: UNDECIDABLE (exit 2)`** |
| 补规则后（同一输入） | `DIFFS 0` ⇒ **`VERDICT: PASS (exit 0)`** |
| `rules.json` 变化 | 42 → 45 条；**21 行纯新增、0 行删除/修改**（既有 ident 的 `mask_body_ranges`/`head_compare`/`approved_by` 一律未动） |
| 新三条的 `approved_by` | 均为 `PENDING owner/coordinator review (draft: openmir2-worker-58)`（与现有 42 条一致；owner 尚未裁定） |
| 回归 | `compare_runs selftest 262/262 GREEN`；`card_window --selftest 226/226 GREEN`；两个既有场景 `--dry-run` 均 `rc=0` |

**一句话**：`spell_cast` 在**比较规则**层面已经不再缺任何一条；它离「可跑清单」只差
①本条规则（本卡已补）、②profile 的 `magic` 夹具（前置卡已写明）、③**窗口 profile 必须按 D-9
pin `MIR2_CLIENT_ABILITY_LAYOUT=0`**（见 §3，这是既有全局口径，不是新缺口）。

## 1. 三条规则的字段口径（以 C# 源为准）

### 1.1 总表

| ident | 名称 | `recog` | `param` | `tag` | `series` | `body`（线缆） |
|---|---|---|---|---|---|---|
| 53 | `SM_HEALTHSPELLCHANGED` | 发送者 ActorId（全局掩） | **HP (u16)** 逐字节 | **MP (u16)** 逐字节 | **MaxHP (u16)** 逐字节 | oracle **0B**（header-only）；Rust 生产默认 D-9 客户端布局 = EDCode(12B TCharDesc)=**16 字符**，对拍须 pin |
| 638 | `SM_MAGICFIRE` | 施法者 ActorId（全局掩） | **nTargetX** 逐字节 | **nTargetY** 逐字节 | **MakeWord(EffectType, Effect)** 逐字节 | **单次 EDCode(4B LE targetActorId) = 6 字符**（**全部 4 字节**；spell_cast 中 target=0 ⇒ `6464686b643f`） |
| 639 | `SM_MAGICFIRE_FAIL` | 施法者 ActorId（全局掩） | **0** 逐字节 | **0** 逐字节 | **0** 逐字节 | **0B**（header-only） |

- `ident` 本身永远参与比较（`compare_runs.py` 判据 1）。
- `recog` 三帧都是**会话作用域 ActorId**（C# `HUtil32.Sequence()` 随机正数 vs Rust
  `EcsStore::alloc_id` 自增）⇒ 由 `rules.json` 的**全局** `mask_fields:["recog"]` 处理，
  三条规则都**不取回**（与 31/52 的既有口径一致：会话变量允许不同，比较器掩 `recog`）。
- **三次 EDCode 拼接？没有**：53/639 无 body；638 是**一次** `EDCode.EncodeBuffer(...)`。

### 1.2 53 `SM_HEALTHSPELLCHANGED`（`src/OpenMir2/Messages.cs:211`）

- oracle 发端：`BaseObject.HealthSpellChanged()`（`src/M2Server/Actor/BaseObject.cs:700-709`）
  对 `Race == ActorRace.Play` 先 `SendUpdateMsg(RM_HEALTHSPELLCHANGED, 0,0,0,0, "")` 发自己
  （`ShowHp` 时再 `SendRefMsg` 广播给视野内玩家）。
- oracle 收端：`src/M2Server/Player/PlayObject.Message.cs:1736-1739`
  ```text
  MakeMessage(SM_HEALTHSPELLCHANGED, processMsg.ActorId, WAbil.HP, WAbil.MP, WAbil.MaxHP)
  SendSocket(ClientMsg)          // 无尾；PlayObject.Socket.cs:77-96 PackLength=CommandMessage.Size=12
  ```
- body：**没有多次 EDCode 拼接，oracle 就是空 body**。
- Rust：`smframe::sm_healthspellchanged`（`mir2-rs/bins/game-svr/src/smframe.rs:262-266`）是
  oracle header-only 布局；生产选路 `sm_healthspellchanged_frame`（`:340-346`）默认走 **D-9
  冻结客户端布局**（尾 `EDCode(12B TCharDesc{HP:i32,MP:i32,MaxHP:i32})`，`:325-337`，
  线缆 **16 字符**）。
- **对拍前提**：跨侧逐字节对拍必须 pin `MIR2_CLIENT_ABILITY_LAYOUT=0`
  （`mir2-rs/tests/parity/whitelist.md:32` 的 D-9 全局口径）。未 pin 时两侧 body **0B vs 16B**
  ⇒ 本规则按 `body_len` 差异 **fail-closed**（不会被掩码洗绿、也不会静默通过）。
- 本规则**不放宽任何字段**（HP/MP/MaxHP 全比）；`spell_cast` 的真机窗口本身仍是 R 级判据。

### 1.3 638 `SM_MAGICFIRE`（`src/OpenMir2/Messages.cs:300`）

- oracle 发点：`MagicManager.DoSpell` 共享尾（`src/M2Server/Magic/MagicManager.cs:648-658`）
  ```text
  SendRefMsg(RM_MAGICFIRE, 0, MakeWord(EffectType, Effect), MakeLong(nTargetX, nTargetY),
             targetActorId /* 无目标 = 0 */, "")
  ```
- oracle 收端：`src/M2Server/Player/PlayObject.Message.cs:1788-1793`
  ```text
  MakeMessage(SM_MAGICFIRE, processMsg.ActorId, LoWord(nParam2) /*x*/, HiWord(nParam2) /*y*/, nParam1 /*MakeWord(EffectType,Effect)*/)
  sSendStr = EDCode.EncodeBuffer(BitConverter.GetBytes(processMsg.nParam3), 4)   // nParam3 = targetActorId
  SendSocket(ClientMsg, sSendStr)
  ```
- body = **单次** EDCode 编码的 **4B LE `targetActorId`**。`EncryptUtil.Encode` 对 4B 输出
  **6 字符**（`encoded_len(4)=4+ceil(4/3)=6`），**覆盖全部 4 字节**。
  > 订正：`tools/probe/scenarios/spell_cast.json` 的 note 写「低 3 字节」不准确；编码按 4 字节整段。
  > 本卡不改场景 JSON（红线），此处只做登记；该文件的措辞可由其 owner 后续顺手订正。
- Rust 同形：`smframe::sm_magicfire`（`smframe.rs:795-811`）；金标准用例
  `spell_and_magicfire_field_layout`（`:3470-3496`）以 `sm_magicfire(9, 0x1112, 0x1314, 5, 6, 0x21)`
  钉住「尾 = `Encode(4B LE targetActorId)`」。
- **可比性边界**：`spell_cast` 两发 `CM_SPELL(3017)` 的 `param=series=0` ⇒ `target ActorId=0`
  （oracle `ActorMgr.Get(0)==null`；Rust `cetin_near_xy` 的 `cell_obj_id > 0` 门）⇒ 两侧 body
  都是 0 的 4B 编码 `6464686b643f`（已用 `EncryptUtil.Encode` 同式反解回 `00000000` 核验）。
  **本规则故意不掩 body**（保住 target=0 路径的编码正确性判据；风格同 654/657 的「先 compare」）。
  **若将来出现 target≠0 的场景**：`targetActorId` 跨栈不可比（同 `recog` 口径），届时须在评审中
  新增窄的 `mask_body_ranges: [[0,6]]`（或 decode-aware 判定），**不得整帧 ignore**。

### 1.4 639 `SM_MAGICFIRE_FAIL`（`src/OpenMir2/Messages.cs:301`）

- oracle 发点：`ClientSpellXY` 的 `default` 支在 `MagicManager.DoSpell` 返回 false
  （含距离门 `MagicManager.cs:155-157`）时
  `SendRefMsg(RM_MAGICFIREFAIL, 0,0,0,0, "")`（`src/M2Server/Player/PlayObject.Attack.cs:1278-1281`）。
- oracle 收端：`src/M2Server/Player/PlayObject.Message.cs:1794-1796`
  `SendDefMessage(SM_MAGICFIRE_FAIL, processMsg.ActorId, 0, 0, 0)` ⇒ `PlayObject.Socket.cs:17-26`
  的 5 参 `SendDefMessage` = `MakeMessage` + `SendSocket(ClientMsg)`（**无尾**）。
- Rust 同形：`smframe_ext.rs:297-302` `sm_magicfire_fail(actor_id)`；金标准
  `header_only_frames_self_computed_golden`（`:600`）钉 12B 头 `c80000007f02000000000000`。
- body：**0B，无 EDCode 尾、无拼接**。

## 2. 两侧对照（构造级，非真机）

### 2.1 夹具

`E:/tmp/w58-frcmp/{csharp,rust}/frames.ndjson`，每侧三帧（53/638/639 各一条），字段取真实语义：

```text
53 : recog=1000(csharp)/275(rust)  param=18(HP) tag=15(MP) series=19(MaxHP)  body=""            (0B)
638: recog=1000(csharp)/275(rust)  param=29(x) tag=28(y) series=257           body="6464686b643f" (6B, target=0)
639: recog=1000(csharp)/275(rust)  param=0 tag=0 series=0                     body=""            (0B)
```

两侧 `recog` / `t_ms` / `conn` / `seq` 故意不同，用来证明全局掩码仍在生效。

### 2.2 补规则前（`rules.json` 42 条，sha256 `193aa7b7c010828b`）

```text
$ python -I -B tools/conformance/compare_runs.py \
    --oracle E:/tmp/w58-frcmp/csharp --rust E:/tmp/w58-frcmp/rust \
    --out E:/tmp/w58-frcmp/report-before.json --rules tools/conformance/rules.json
== compare_runs ==
rules: ...\tools\conformance\rules.json (sha256=193aa7b7c010828b, ident 规则 42 条，PENDING 复核)
frames: oracle=3 rust=3 compared=3
idents: [53, 638, 639]
DIFFS: 0
whitelisted: 9 条（masked_head conn/recog/t_ms × 3 帧）
UNDECIDABLE: 3
  coverage/unclassified_ident: ident 53 在产物里出现但规则表没有条目（UNCLASSIFIED）
  coverage/unclassified_ident: ident 638 在产物里出现但规则表没有条目（UNCLASSIFIED）
  coverage/unclassified_ident: ident 639 在产物里出现但规则表没有条目（UNCLASSIFIED）
VERDICT: UNDECIDABLE (exit 2)          # rc=2
```

### 2.3 补规则后（`rules.json` 45 条，sha256 `4bfde36357e9b68c`）

```text
$ ... 同一命令，--out E:/tmp/w58-frcmp/report-after.json ...
== compare_runs ==
rules: ...\tools\conformance\rules.json (sha256=4bfde36357e9b68c, ident 规则 45 条，PENDING 复核)
frames: oracle=3 rust=3 compared=3
idents: [53, 638, 639]
DIFFS: 0
whitelisted: 9 条（masked_head conn/recog/t_ms × 3 帧）
VERDICT: PASS (exit 0)                 # rc=0
```

### 2.4 「改坏必红」（证明三条规则真在比，不是被掩绿）

在 rust 侧产物上各改一处，再跑同一比较器：

```text
53_param              rc=1 DIFFS: 1 | VERDICT: FAIL (exit 1)     # param 18 -> 17
638_series            rc=1 DIFFS: 1 | VERDICT: FAIL (exit 1)     # series 257 -> 258
638_body_target0x21   rc=1 DIFFS: 1 | VERDICT: FAIL (exit 1)     # body 换成 Encode(0x21)
639_series            rc=1 DIFFS: 1 | VERDICT: FAIL (exit 1)     # series 0 -> 1
```

⇒ 638 的 **body 仍在判**（不是掩码）；53/639 的头字段也在判。

## 3. `spell_cast` 进「可跑清单」的清单（本卡不改 batch-plan）

`docs/window-batch-plan-20261023.md` 属 drift-sweep 领域，本卡**只给建议**：

1. **规则（已由本卡完成）**：`rules.json` 有 53/638/639 三条 `action=compare`。
2. **夹具（前置卡已写明）**：窗口 profile 必须有
   `magic: [{"magic_id": 1, "level": 0, "use_key": "1", ...}]`，否则只有 `SM_MOVEFAIL(28)`。
3. **D-9 起栈口径（既有强制项，不是新缺口）**：跨侧逐字节对拍必须让 Rust 侧以
   `MIR2_CLIENT_ABILITY_LAYOUT=0` 起栈。**当前 `E:/tmp/spellcast-profile.json` 没有这一段**；
   窗口前须在 profile 的 `sides.rust.env` 加 `"MIR2_CLIENT_ABILITY_LAYOUT": "0"`
   （`card_window.py` 会把 profile 的 `sides.*.env` 原样传给起栈命令；`start_mir2_server.ps1`
   只注入 `MIR2_RNG_SEED`/`MIR2_DB_NAME`，不注入这一项）。
   缺这条 ⇒ 53 两侧 `body_len` 0 vs 16 ⇒ **FAIL**（fail-closed，本卡不能也不应把它掩掉）。
4. 建议 drift-sweep owner 把 §7.3/§7.4 的 `spell_cast` 判据改为：
   「**比较规则已具备（53/638/639，`rules.json`）；跑前确认 D-9 pin + magic 夹具**」，
   即把「不可跑」降级为「待窗口 / 跑前条件清单」。

## 4. 门禁与回归（本 head 现跑）

```text
$ python -I -B tools/conformance/compare_runs.py selftest
GREEN: compare_runs selftest 262/262                                  # rc=0

$ python -I -B tools/conformance/card_window.py --selftest
GREEN: card_window.py selftest 226/226                                # rc=0

$ python -I -B tools/conformance/card_window.py --dry-run \
    --scenario tools/probe/scenarios/login_only.json --out E:/tmp/w58-frcmp/dry-login
[compare] compare: ... --rules ...\tools\conformance\rules.json       # rc=0

$ python -I -B tools/conformance/card_window.py --dry-run \
    --scenario tools/probe/scenarios/move_walk.json --out E:/tmp/w58-frcmp/dry-move
[compare] compare: ... --rules ...\tools\conformance\rules.json       # rc=0

$ python -I -B tools/whitelist/t_reservation_gate.py check
T_RESERVATION_GATE GREEN（预约表 255 行 / whitelist 255 号）           # rc=0

$ git grep -n -E '^(<{7}|={7}|>{7})( |$)' -- . ':!*.md'              # rc=1 = 无匹配（无冲突标记）
$ git merge-tree $(git merge-base HEAD walgit/master) HEAD walgit/master ; echo $?
0                                                                     # merge-tree 干净
```

## 5. 写集核对与残留物

- `git diff --numstat`：`tools/conformance/rules.json` **21 行新增、0 行删除**；本文件新增。
  其它文件未动（`git status --porcelain` 只列这两项）。
- `rules.json` 三条新规则均为 `approved_by = PENDING ...`；既有 42 条的
  `mask_body_ranges`/`head_compare`/`approved_by` 未被触碰（纯追加在数组末尾可直接验证）。
- 临时物（本卡收尾删除）：`E:/tmp/w58-frcmp/`（两侧 frames 夹具、`report-before.json`、
  `report-after.json`、4 个 mutation 目录、`rules_insert.py`）。`--dry-run` 不创建 out 目录。
- **未起栈、未连库**；未改 `src/**`、场景 JSON、`whitelist.md`、batch-plan。
