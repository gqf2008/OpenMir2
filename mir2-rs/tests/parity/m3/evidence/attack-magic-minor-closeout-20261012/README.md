# `openmir2-m3-attack-magic-wiring`（**Minor 收口：M-1 + F-1/F-2**）证据

- 卡片：`openmir2-m3-attack-magic-wiring`（issue `3132299100e88f1f461e8515c4a715cf2942aa3f`）
- 本片：把审查者遗留的三条 Minor 收口 ——
  1. **M-1**（reviewer-5 `ebfd15a8`，攻击片）：`tests::combat_downlink_sends_struck_and_nowdeath`
     的 doc 声称覆盖 `SM_STRUCK` 16B `MessageBodyWL` 尾、实测只断言 12B 头 ⇒ 补**逐字节尾断言**；
  2. **F-1**（reviewer-5 `1de8fca5`，魔法片）：`switch(MagIdx)` 的 `default` 首句行号被引成 `:1256`、
     实为 `:1269-1270` ⇒ 订正注释与 `magic-wiring` README；
  3. **F-2**（reviewer-5 `1de8fca5`，魔法片）：Rust 对**所有**魔号都套用 `default` 首句
     `Dir = GetNextDirection(...)`，而 oracle 的 `switch` 有 8 个独立 `case` 不执行该句 ⇒ 按
     **case 集合**门住（只有非这 8 个魔号才改展示方向）+ 单测/进程级用例/红检。
- 工作树：`E:\Users\gxh\Documents\GitHub\OpenMir2\.claude\worktrees\m2-svr`（分支 `worktree-m2-gamesvr-host`）
- base：`refs/heads/master` = **`bbf63fb0`**（`9753c4d0` + `bbf63fb0` 均已 merge 进本分支，merge 提交
  `e9ce5e89` / `14f7c46b`；被审 head 见卡上 patch/status）
- 身份：`openmir2-worker-11`。**不 merge、不自签 review。**

## 0. 与同卡其它片的关系

| 片 | 内容 | 状态 |
| --- | --- | --- |
| 第一片 | `CM_*HIT` 9 条派发 + `+GD` ack + `SM_HIT` 族/`SM_STRUCK`/`SM_NOWDEATH` 回帧（`3824fd96`） | 已 approve（reviewer-5 `ebfd15a8`）并合入 master `29516d79`；其 M-1 由本片收口 |
| 第二片 | 魔法派发 `CM_SPELL`/`CM_MAGICKEYCHANGE`（`c1fd01a1` / patch `38f4ddff`，证据 `evidence/magic-wiring-20261012/`） | 已 approve（reviewer-5 `1de8fca5`）；其 F-1/F-2 由本片收口 |
| **本片** | 上述三条 Minor 收口（`magicwire.rs` + `main.rs` 用例 + 本目录） | 本目录 |

⇒ 交付 patch `--base refs/heads/master --head refs/heads/worktree-m2-gamesvr-host` 的差量 =
**第二片（魔法派发）+ 第一片 M-1 + 第二片 F-1/F-2 收口**；`src/`、`sql/`、`MirClient`、`crates/**` 均未改。

## 1. 本片改了什么（写集 `mir2-rs/bins/game-svr/src/{magicwire,main}.rs` + 本片证据）

- **M-1**：`combat_downlink_sends_struck_and_nowdeath` 在第一帧 `SM_STRUCK` 的 12B 头断言之后，
  新增 `struck[12..] == mir2_protocol::edcode::encode(&wl)` 的**逐字节尾断言**，其中
  `wl = MessageBodyWL{ Param1 = GetFeature() = 111, Param2 = CharStatus = 0, Tag1 = 命中者 ActorId, Tag2 = 0 }`
  （本用例的 `ActorDisplay::new(dir=4, light=0, feature=111, ..., status=0, ...)`、`mag=false`）。
  oracle 尾形状：`PlayObject.Message.cs:1562-1628`（`RM_STRUCK` 支的 `EDCode.EncodePacket(MessageBodyWL{...})`）
  + `src/OpenMir2/Packets/ServerPackets/MessageBodyWL.cs`（4×`int` = 16B，顺序 `Param1, Param2, Tag1, Tag2`）。
- **F-1**：`main.rs` 与 `evidence/magic-wiring-20261012/README.md` 里 `default` 首句行号
  `PlayObject.Attack.cs:1256` → **`:1269-1270`**（`default:` 在 `:1269`、`Dir = ...` 在 `:1270`；
  `:1256` 是 `SKILL_43` case 内的 `if`）。自核：`src/M2Server/Player/PlayObject.Attack.cs`。
- **F-2**：`magicwire.rs` 新增 `EXPLICIT_CASE_MAG_IDX: [i32; 8] = [12, 25, 26, 27, 34, 38, 43, 56]`
  （= `SKILL_ERGUM/SKILL_BANWOL/SKILL_FIRESWORD/SKILL_MOOTEBO/SKILL_CROSSMOON/SKILL_TWINBLADE/SKILL_43/SKILL_REDBANWOL`，
  `MagicConst.cs:52-197`；`case` 行号 `PlayObject.Attack.cs:1120/1136/1152/1168/1187/1218/1234/1253`）
  + `takes_default_dir()`；`handle_spell_cmd` 只在 **不在这 8 个内**的魔号上执行
  `Dir = GetNextDirection`（`:1269-1270`）。效果体整段仍未接（归 m3cs），本片只对齐「走哪条分支」。

## 2. 门禁

**隔离 target = 私有目录 `E:/tmp/openmir2-m2svr-amw-target`（全 `--offline`）**。⚠️ 原口径的共享目录
`E:/tmp/openmir2-m2svr-target` 本轮被**并发 lane** 写入（实测其 `.fingerprint/**` 里混入
另一工作树 `E:\tmp\revamw` 的路径，`cargo test -p game-svr` 因夹具路径指向已不存在的
`E:\tmp\revamw\...` 假红 31 条）⇒ 按仓库既有教训（`brief-m2svr-n1.md`：「该目录被污染过一次」）
改用**私有 target** 复跑；私有目录 `.fingerprint` 内**零** `revamw` 引用、全部指向本工作树。

| 门禁 | 结果 | 证据 |
| --- | --- | --- |
| `cargo fmt --all -- --check` | rc=0 | `gates-fmt.txt` |
| `cargo clippy --workspace --all-targets -- -D warnings` | rc=0 | `gates-clippy.txt` |
| `cargo test --workspace` | **75 suites / 615 passed / 0 failed / 16 ignored**（rc=0） | `gates-test-workspace.txt` |
| `cargo run -p msgcodegen -- --check` | rc=0，604 条 | `gates-msgcodegen.txt` |
| `cargo test -p game-svr` ×3 | 各 **230 passed**（rc=0，可重复） | `gamesvr-3run-1..3.txt` |

> 计数：本片 +1 单测（`magicwire::tests::default_dir_only_for_non_explicit_case_magics`）⇒
> `-p game-svr` 由上一片 base 的 229 变为 230；`--workspace` 由 `74/609/0/16`（第一片 base）经
> master `9753c4d0`（m3cs 第八片 +1 suite/+5 passed）与本片 +1 变为 `75/615/0/16`。

## 3. 牙齿（改坏 ⇒ 必红；原始输出见 `red-*.txt`，改后均 `cp` 还原、`git status` 干净）

| # | 改坏点 | 结果 | 文件 |
| --- | --- | --- | --- |
| 5 | `smframe::sm_struck` 尾 `wl[0..4]`/`wl[4..8]` 互换（M-1） | `combat_downlink_sends_struck_and_nowdeath` **FAILED**：尾断言 left `[100,100,...]`/right `[111,100,...]` | `red-5-struck-tail-feature-status-swap.txt` |
| 6 | `EXPLICIT_CASE_MAG_IDX[0]` `12 → 120`（F-2） | ① `magicwire::tests::default_dir_only_for_non_explicit_case_magics` **FAILED**（精确集合断言）；② 进程级 `spell_cmd_dispatches_guard_ack_and_magic_key_change` **FAILED**（`mag_idx=12` 的方向被写成右、断言「保持左」失败） | `red-6-f2-explicit-case-set.txt` |

攻击片原有的 red-1..4、魔法片的 red-1..2 分别见 `evidence/attack-wiring-20261012/`、
`evidence/magic-wiring-20261012/`。

## 4. 端口 / 残留

- **本片不起栈、不占端口**：`ports-preflight.txt` 实拍 = 仅共享 MySQL `3306` LISTENING，
  5000/5100/5500/5600/5700/6000/7000/7100/7200 全 FREE；未起客户端、未动 `E:\MirServer`。
- **本轮让窗**：T-28 已合入 master ⇒ `m3bag` 的 T-14 两相待跑；本片按 brief 顶部 owner 08:50
  「优先离线/单测」走离线，不争端口。
- 未改 `crates/**`、`src/**`、`MirClient`、表结构/脚本语法/数据格式、`tests/parity/m3/**` 夹具、`whitelist.md`。

## 5. 登记（沿用第一/二片，本片未新增）

- 技能效果面（`SM_SPELL`/限速/战士技切换/`DoSpell`）归 `openmir2-m3-combat-skills-align`（m3cs）；
  限速/超速、`!boLateDelivery`、`RM_MOVEFAIL` 编码器、`STONE`、`GetHitMode` 技能态、目标筛选、
  `HealthTick`/`SpellTick` 等见 `attackwire.rs`/`magicwire.rs` 模块头与两片 README。
- F-2 收口后，这 8 个魔号的 `Dir` **不再被改动**（与 oracle 的 `case` 分支一致）；其效果体本身
  仍由 m3cs 的接口接上。

## 6. 复现

见 `commands.txt`；哈希清单 `SHA256SUMS.txt` 按「LF 归一 / 仓库字节」口径，
复核：`python -I tools/evidence-hash/verify_sha256sums.py --check-extra tests/parity/m3/evidence/attack-magic-minor-closeout-20261012`。
