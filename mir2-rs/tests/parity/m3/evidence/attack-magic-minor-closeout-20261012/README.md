# `openmir2-m3-attack-magic-wiring`（**M-1 收口**）证据

- 卡片：`openmir2-m3-attack-magic-wiring`（issue `3132299100e88f1f461e8515c4a715cf2942aa3f`）
- 本片：攻击片 reviewer-5（`ebfd15a8`）遗留的 **M-1** 收口 —— 给进程级用例
  `tests::combat_downlink_sends_struck_and_nowdeath` 补 `SM_STRUCK` **16B `MessageBodyWL` 尾**的
  逐字节断言（原用例 doc 声称覆盖尾、实测只断言 12B 头，对尾 mutation 抓不到）。
- 工作树：`E:\Users\gxh\Documents\GitHub\OpenMir2\.claude\worktrees\m2-svr`（分支 `worktree-m2-gamesvr-host`）
- base：`refs/heads/master` = **`bbf63fb0`**（`9753c4d0` + `bbf63fb0` 均已 merge 进本分支，merge 提交 `e9ce5e89` / `14f7c46b`；被审 head 见卡上 patch/status）
- 身份：`openmir2-worker-11`。**不 merge、不自签 review。**

## 0. 与同卡其它片的关系

| 片 | 内容 | 状态 |
| --- | --- | --- |
| 第一片 | `CM_*HIT` 9 条派发 + `+GD` ack + `SM_HIT` 族/`SM_STRUCK`/`SM_NOWDEATH` 回帧（`3824fd96`） | 已 approve（reviewer-5 `ebfd15a8`）并合入 master `29516d79` |
| 第二片 | 魔法派发 `CM_SPELL`/`CM_MAGICKEYCHANGE`（`c1fd01a1` / patch `38f4ddff`，证据 `evidence/magic-wiring-20261012/`） | 已在待审 |
| **本片** | 第一片的 **M-1 收口**（`SM_STRUCK` 16B 尾逐字节断言 + 对应红检） | 本目录 |

⇒ 交付 patch `--base refs/heads/master --head refs/heads/worktree-m2-gamesvr-host` 的差量 =
**第二片（魔法派发）+ 本片（M-1 收口）**；`src/`、`sql/`、`MirClient`、`crates/**` 均未改。

## 1. 本片改了什么（写集 `mir2-rs/bins/game-svr/src/main.rs` 用例 + 本片证据）

- `tests::combat_downlink_sends_struck_and_nowdeath`：在第一帧 `SM_STRUCK` 的 12B 头断言之后，新增
  `struck[12..] == mir2_protocol::edcode::encode(&wl)` 的**逐字节尾断言**，其中
  `wl = MessageBodyWL{ Param1 = GetFeature() = 111, Param2 = CharStatus = 0, Tag1 = 命中者 ActorId, Tag2 = 0 }`
  （本用例的 `ActorDisplay::new(dir=4, light=0, feature=111, ..., status=0, ...)`、`mag=false`）。
- 订正 doc：红检清单补 **③**（尾 `feature`/`status` 互换 ⇒ 尾断言必红），并把「声称覆盖尾」的
  误导文案改成「现在真的逐字节断言」。
- **oracle 行号（尾的形状）**
  - `src/M2Server/Player/PlayObject.Message.cs:1562-1628` —— `RM_STRUCK` 支：
    头 `MakeMessage(SM_STRUCK, ActorId, WAbil.HP, WAbil.MaxHP, wParam /*=伤害*/)`，
    尾 `EDCode.EncodePacket(MessageBodyWL{ Param1 = GetFeature(), Param2 = CharStatus,
    Tag1 = nParam3 /*=命中者*/, Tag2 = (RM_STRUCK_MAG ? 1 : 0) })`。
  - `src/OpenMir2/Packets/ServerPackets/MessageBodyWL.cs` —— 4×`int` = 16B（顺序
    `Param1, Param2, Tag1, Tag2`），与 `smframe::sm_struck` 的 `wl[0..4]/[4..8]/[8..12]/[12..16]` 相同。
- 只动用例与注释；**未改生产路径**（`sm_struck` 实现不变）。**未动 `crates/**`**。

## 2. 门禁（隔离 `CARGO_TARGET_DIR=E:/tmp/openmir2-m2svr-target`，全 `--offline`）

| 门禁 | 结果 | 证据 |
| --- | --- | --- |
| `cargo fmt --all -- --check` | rc=0 | `gates-fmt.txt` |
| `cargo clippy --workspace --all-targets -- -D warnings` | rc=0 | `gates-clippy.txt` |
| `cargo test --workspace` | **75 suites / 614 passed / 0 failed / 16 ignored**（rc=0） | `gates-test-workspace.txt` |
| `cargo run -p msgcodegen -- --check` | rc=0，604 条 | `gates-msgcodegen.txt` |
| `cargo test -p game-svr` ×3 | 各 **229 passed**（rc=0，可重复） | `gamesvr-3run-1..3.txt` |

> 计数口径：本片只加断言、不加用例 ⇒ `-p game-svr` 仍 229。`--workspace` 从上一片（merge master 前）
> 的 `74/609/0/16` 变为 `75/614/0/16`，增量来自本轮 merge 进来的 master `9753c4d0`
> （m3cs 第八片 `crates/world/tests/attack_skill_vectors.rs`：+1 suite / +5 passed），非本片新增。
> 门禁均在 **merge `bbf63fb0` 之后**的树上复跑（该 merge 只动 `tests/parity/m3/**` 证据/夹具与
> `whitelist.md`，无 `.rs` ⇒ 计数不变）。

## 3. 牙齿（改坏 ⇒ 必红）

| # | 改坏点 | 结果 |
| --- | --- | --- |
| 5 | `smframe::sm_struck` 尾 `wl[0..4]` 与 `wl[4..8]` 互换（`status`/`feature` 错位） | `tests::combat_downlink_sends_struck_and_nowdeath` **FAILED**：尾断言 left `[100,100,...]` / right `[111,100,...]`（`red-5-struck-tail-feature-status-swap.txt`） |

改后 `cp` 还原（`smframe.rs` 与 HEAD 一致）、`git status` 干净（仅 `main.rs` 修改 + 本片证据目录为新增）。
攻击片原有的 red-1..4 见 `evidence/attack-wiring-20261012/`。

## 4. 端口 / 残留

- **本片不起栈、不占端口**：`ports-preflight.txt` 实拍 = 仅共享 MySQL `3306` LISTENING，
  5000/5100/5500/5600/5700/6000/7000/7100/7200 全 FREE；未起客户端、未动 `E:\MirServer`。
- **本轮让窗**：T-28 已合入 master ⇒ `m3bag` 的 T-14 两相待跑；本片按 brief 顶部 owner 08:50
  「优先离线/单测」走离线，不争端口。
- 未改 `crates/**`、`src/**`、`MirClient`、表结构/脚本语法/数据格式、`tests/parity/m3/**` 夹具、`whitelist.md`。

## 5. 登记（沿用第一/二片，本片未新增）

- 限速/超速（`CheckActionStatus`/`AttackTick`/…）、`!boLateDelivery` 支、`RM_MOVEFAIL` 编码器、
  `STONE` 状态位、`GetHitMode` 的技能/装备态、目标筛选（`IsProperTarget`/`InSafeZone`/PK）、
  `HealthTick`/`SpellTick` 体力面等 —— 见 `evidence/attack-wiring-20261012/README.md` 与
  `attackwire.rs`/`magicwire.rs` 模块头。
- 技能效果面（`SM_SPELL`/限速/战士技切换/`DoSpell`）归 `openmir2-m3-combat-skills-align`（m3cs）。

## 6. 复现

见 `commands.txt`；哈希清单 `SHA256SUMS.txt` 按「LF 归一 / 仓库字节」口径，
复核：`python -I tools/evidence-hash/verify_sha256sums.py --check-extra tests/parity/m3/evidence/attack-magic-minor-closeout-20261012`。
