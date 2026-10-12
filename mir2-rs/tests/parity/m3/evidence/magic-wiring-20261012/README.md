# `openmir2-m3-attack-magic-wiring`（第二片：魔法派发）证据

- 卡片：`openmir2-m3-attack-magic-wiring`（issue `31322991`）；本片 = 攻击片（第一片 `3824fd96`）之后的**魔法半**。
- 工作树：`E:\Users\gxh\Documents\GitHub\OpenMir2\.claude\worktrees\m2-svr`（分支 `worktree-m2-gamesvr-host`）
- base：`refs/heads/master` = **`5e23d484`**（已 merge 进本分支，merge 提交 `64e73ebc`；攻击片 `3824fd96` 未被改写）
- 身份：`openmir2-worker-11`。**不 merge、不自签 review。**

## 1. 本片做了什么（写集 `mir2-rs/bins/game-svr/**`，未动 `crates/**`）

| 文件 | 内容 |
| --- | --- |
| `bins/game-svr/src/magicwire.rs`（新增） | `CM_SPELL`/`CM_MAGICKEYCHANGE` 的纯函数接缝：入帧映射（`parse_spell`/`parse_magic_key`）、`find_magic`（= `GetMagicInfo(int)`）、`change_magic_key`（= `ClientChangeMagicKey`）、`spell_reject`（= `ClientSpellXY` 三守卫）；6 条单测 |
| `bins/game-svr/src/main.rs` | `handle_spell_cmd` / `handle_magic_key_cmd`；魔法表**不改 `SessionGround`**——直接用已有的 `SavedHuman.record.data.magic`（该结构就是“未在 SessionGround 建模的字段原样保留”的整条记录）⇒ `CM_MAGICKEYCHANGE` 改的 `Key` 会随存档落库；sim 循环两处派发；1 条进程级用例 |
| `mir2-rs/bins/` 其余 | 未动 |

`magicwire` 留在 bin（依赖 bin 私有的 `record::MagicRcd`）；lib 里的 `movewire`/`attackwire` 只依赖 `mir2_protocol`/`mir2_world`。

### oracle 逐条行号

- **入帧映射**
  - `CM_SPELL`：`src/GameSrv/Word/WorldServer.cs:783-790` ——
    `SendUpdateMsg(Ident, Tag, LoWord(Recog), HiWord(Recog), MakeLong(Param, Series), "")`
    （`SendUpdateMsg` 形参序同 `SendMsg`，`src/M2Server/Actor/BaseObject.Message.cs:283/303`）
    ⇒ `wParam(Tag)=魔法号`、`nParam1/nParam2 = 目标坐标`、`nParam3 = MakeLong(Param,Series)`。
  - `CM_MAGICKEYCHANGE` 走 `WorldServer` 的 `default` 支：`WorldServer.cs:857-859`
    `SendMsg(playObject, Ident, Series, Recog, Param, Tag, sMsg)` ⇒ `nParam1=Recog`、`nParam2=Param`。
- **处理器**
  - `CM_SPELL`：`src/M2Server/Player/PlayObject.Message.cs:1339-1340`
    `ClientSpellXY(wIdent, wParam, (short)nParam1, (short)nParam2, ActorMgr.Get(nParam3), LateDelivery, ref delayTime)`；
    成功 ⇒ `ActionTick = GetTickCount(); SendSocket(M2Share.GetGoodTick)`（`+GD`）；失败且 `delayTime==0`
    ⇒ `SendRefMsg(RM_MOVEFAIL,0,0,0,0,"")`。
  - `CM_MAGICKEYCHANGE`：`PlayObject.Message.cs:871-873` → `ClientChangeMagicKey((ushort)nParam1,(char)nParam2)`。
- **`ClientSpellXY` 守卫**：`src/M2Server/Player/PlayObject.Attack.cs:1054-1068`
  （`!IsCanSpell` → `Death||STONE` → `GetMagicInfo(nKey)==null`，任一 ⇒ `return false`）。
- **`GetMagicInfo(int)`**：`src/M2Server/Player/PlayObject.cs:4174-4185`（比 `userMagic.Magic.MagicId`，**不是按键**）。
- **`ClientChangeMagicKey`**：`src/M2Server/Player/PlayObject.Operate.cs:774-786`（命中 `MagicId==nSkillIdx` 置 `Key` 后 `break`，**无回帧**）。
- **`switch(MagIdx)` 的 `default` 首句**：`PlayObject.Attack.cs:1269-1270`
  `Dir = M2Share.GetNextDirection(CurrX, CurrY, nTargetX, nTargetY)`（复用 `movewire::next_direction`）。
  该句**只在 `default` 支执行**：`switch` 的 8 条独立 `case`（`:1120/1136/1152/1168/1187/1218/1234/1253`，
  魔号 12/25/26/27/34/38/43/56）**不执行**（reviewer-5 `1de8fca5` 的 F-1/F-2 收口，代码见 `magicwire::EXPLICIT_CASE_MAG_IDX`）。

## 2. 门禁（隔离 `CARGO_TARGET_DIR=E:/tmp/openmir2-m2svr-target`，全 `--offline`）

| 门禁 | 结果 | 证据 |
| --- | --- | --- |
| `cargo fmt --all -- --check` | rc=0 | `gates-fmt.txt` |
| `cargo clippy --workspace --all-targets -- -D warnings` | rc=0 | `gates-clippy.txt` |
| `cargo test --workspace` | **74 suites / 609 passed / 0 failed / 16 ignored**（rc=0） | `gates-test-workspace.txt` |
| `cargo run -p msgcodegen -- --check` | rc=0，604 条 | `gates-msgcodegen.txt` |
| `cargo test -p game-svr` ×3 | 各 **229 passed**（rc=0，可重复） | `gamesvr-3run-1..3.txt` |

（攻击片时为 `73/596/0/16`、`game-svr` 222 ⇒ 本片 +7 条用例：`magicwire` 6 单测 + 1 进程级。）

## 3. 新用例与红检

**新用例（7 条）**
- `magicwire::tests`：`change_magic_key_sets_only_matching_id` /
  `change_magic_key_ignores_when_absent` / `find_magic_matches_magic_id_and_skips_none` /
  `parse_spell_maps_world_server_fields` / `parse_magic_key_truncates_recog_to_u16` /
  `spell_reject_order_and_results`（6 条）；
- 进程级 `tests::spell_cmd_dispatches_guard_ack_and_magic_key_change`：① 未知魔法号 ⇒ 不回 ack；
  ② 有该魔法 + 目标在右 ⇒ 回 `+GD`、`Dir = GetNextDirection`（= 右）、本片不发任何魔法帧；
  ③ `Death` ⇒ 不回 ack；④ `CM_MAGICKEYCHANGE` 命中 `mag_idx` ⇒ 改 `Key` 且无帧，未命中 ⇒ 不改。

**可判红（2 条；改坏 ⇒ 必红，原始输出见 `red-*.txt`）**

| # | 改坏点 | 结果 |
| --- | --- | --- |
| 1 | `magicwire::change_magic_key` 改比 `magic_key == skill_idx`（而非 `mag_idx == skill_idx`） | 进程级用例 FAILED：left 109(`'m'`) / right 102(`'f'`)（`red-1-*`） |
| 2 | `magicwire::spell_reject` 加 `&& false` 不看 `has_magic` | 进程级用例 FAILED：「未知魔法号 ⇒ 不回 ack / 无帧」断言（`red-2-*`） |

改后均 `cp` 还原、`git status` 干净。

## 4. 端口 / 残留

- **本片不起栈、不占端口**：`ports-preflight.txt` 实拍 = 仅共享 MySQL `3306` LISTENING，
  5000/5100/5500/5600/5700/6000/7000/7100/7200 全 FREE。
- 未动 `crates/**`、`src/**`、`MirClient`、表结构/脚本语法/数据格式、`tests/parity/m3/**` 夹具、`whitelist.md`。

## 5. 登记（本片未接，不静默；完整清单见 `magicwire` 模块头）

- **`ClientSpellXY` 的技能效果体**（`PlayObject.Attack.cs:1071-1275`）整个 `switch (UserMagic.MagIdx)`：
  限速（`CheckActionStatus` + `MagicAttackTick`/`MagicAttackInterval`/`MagicAttackCount`/`OverSpeedCount`/`BoEmergencyClose`）、
  `SpellTick -= 450`、`GetSpellPoint`/`DamageSpell`、战士技切换与
  `+LNG/+ULNG/+WID/+UWID/+FIR/+CRS/+UCRS/+TWN/+CID/+UCID` 帧、`default` 支的
  `DoSpell`（`MagicManager.cs:145-…`）**均未接** ⇒ 本片**不产出 `SM_SPELL(17)`**、
  `RM_MAGICFIREFAIL ⇒ SM_MAGICFIRE_FAIL(639)` 亦未接。技能效果面的向量与用例在
  `openmir2-m3-combat-skills-align`（m3cs）的离线片，其接口就绪后再接这一层。
  - `DoSpell` 开头的 `IsWarrSkill` 门（`MagicManager.cs:151`）与 `MagicAttackRage` 门（`:155`，
    需 `crates/envir` 的 `GameSvrConf.MagicAttackRage`，本片未动 `crates/**`）**未接**。
- **`LateDelivery`**：Rust 入帧路径无延时重投（`command` 直投）⇒ 恒按 `false` 处理；oracle 此时
  `!boLateDelivery` 限速支**应生效**，本片按**直通**（等价 `boLateDelivery`）并登记
  （与 `attackwire`/`movewire` 同源登记）。
- **魔法表存放**：用 `SavedHuman.record.data.magic`（进世界时保位存下的 `CharacterData.magic`），
  **未改 `SessionGround`**（brief 顶部 owner 约束 #1 的三处保护面之一）；`CM_MAGICKEYCHANGE` 改的 `Key`
  随 `build_save_record`（克隆整条记录）落库，与 oracle `MakeSaveRcd` 的 `MagicList` 同源。
- **`IsCanSpell`**：oracle 默认 `true`（`PlayObject.cs:960` 初值；`Config.LockSpellAction` 与
  `ClientUseItems` 的置位面未移植）⇒ 本片恒 `true`（登记）。
- **`STONE`**（`StatusTimeArr[PoisonState.STONE]`）：状态位面未移植 ⇒ `Death` 已接、`STONE` 未接。
- **`RM_MOVEFAIL` 编码器**：未接 ⇒ 守卫拒绝支只记日志（不静默）。

## 6. 复现

见 `commands.txt`；哈希清单 `SHA256SUMS.txt` 按「LF 归一 / 仓库字节」口径，
复核：`python -I tools/evidence-hash/verify_sha256sums.py --check-extra tests/parity/m3/evidence/magic-wiring-20261012`。
