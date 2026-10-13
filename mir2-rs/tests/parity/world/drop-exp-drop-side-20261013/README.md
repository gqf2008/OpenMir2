# M3④-D 掉落侧：oracle 锚定 + 「落地 vs 不落地」冲突裁决请求

> 卡：`openmir2-m3-drop-exp-wiring`（issue `58825bcc`，M3④-实现 D）——本片是**掉落侧分片**的
> **oracle 锚定（DoD①）与归属裁决请求**，**不擅改**生产语义（见 §5「为何本片零 .rs 改动」）。
> base = `refs/heads/master` **`90fab5c0`**（交付前已 `git merge origin/master`）；head = 分支 `drop-exp-host` **`2f403751`**
> （D① `c059c086` + D② `4139c2f0` + 本报告 + merge master）。
> 工作目录 `<worktree>/mir2-rs`；无栈、无端口（判据全离线）。

## 0. 一句话

D① 登记的那条 finding（「oracle 死亡掉落恒不落地、Rust 会落地」）在本片被**钉死到行**，
并新发现问题不在 `src/`，而在**本项目自己的 C# oracle 探针**：
`tests/parity/csharp/WorldGolden/Program.cs:344-366` 的死亡结算**绕开了 `Envir.AddItemToMap`**，
直接用桩 `AddMapObject(...)` 造地面物 ⇒ `golden/combat.txt` / `golden/session.txt` 编码的是
**探针意图**（落地），不是**冻结 oracle `src/` 的真实行为**（不落地）。
⇒ 「要不要把 Rust 落地改回不落地」是一次**跨卡裁决**（动 `WorldGolden/**` + `world/golden/**`，
不在本卡写集），本片给两条路 + 可一键套用的 diff，请 owner 路由。

## 1. oracle 真实行为（冻结 `src/`，逐行）

| 步骤 | 位置 | 事实 |
| --- | --- | --- |
| `MapItem.ItemId` 唯一的赋值点 | `src/Modules/SystemModule/Data/MapItem.cs:41` | `// this.ItemId = M2Share.ActorMgr.GetNextIdentity();` —— **被注释**（`rg 'ItemId\s*=' src` 其余 4 处全是 `== 0` 守卫） |
| 加地图首行门 | `src/M2Server/Maps/Envirnoment.cs:88-93` | `AddItemToMap` 第一句 `if (mapItem.ItemId == 0) return false;` |
| 死亡掉落物品 | `src/M2Server/Actor/BaseObject.cs:522-580`（`DropItemDown`） | 造 `MapItem{UserItem,Name,Looks}`（**无 ItemId**）→ StdMode 40/45 处理 → `GetDropPosition` → `AddItemToMap`；成功支才 `SendRefMsg(RM_ITEMSHOW, …)`（`:570`） |
| 死亡掉落金币 | `src/M2Server/Actor/BaseObject.cs:730-768`（`DropGoldDown`） | 同形；成功支 `:752` |
| 死亡分发 | `src/M2Server/Actor/BaseObject.cs:2450-2464`（`DieDropItems`）+ `BaseObject.Base.cs:766-767`（`RM_DIEDROPITEM`）| `DropUseItems` / `ScatterBagItems`（`BaseObject.Base.cs:501-550`，**倒序**遍历 `ItemList`）/ `ScatterGolds`（`:1711`） |
| `RM_ITEMSHOW` → 客户端 | `src/M2Server/Player/PlayObject.Message.cs:1744-1745` | `SendDefMessage(SM_ITEMSHOW, …)`——**只有 `AddItemToMap` 成功才走得到** |

> `GetDropPosition`（`BaseObject.cs:459-524`）是**纯格子扫描、不消费 RNG**；`GetRandomLook`
> （`M2Share.cs:524`，StdMode 45）才消费 RNG，且也在 `AddItemToMap` **之前**。
> ⇒ 真实 oracle 的死亡掉落：**RNG 面几乎为零（除 StdMode 45）**，但**落地面恒空**、
> `SM_ITEMSHOW(610)` **恒不发**。

⇒ 与既有白名单 **T-19**（`whitelist.md:148`「oracle 缺陷复刻：正常 `CM_DROPITEM` 恒失败，
**Rust 1:1 复刻该既有行为**」）与 **T-25**（`:154`「oracle 侧地面物不可达 ⇒ **world 写分片不批**；
无地面物是唯一可达形态」，owner 裁决 `073f0dac`）**同根同结论**。

## 2. Rust 现状

- `crates/world/src/world.rs::settle_deaths` → `place_floor_item` → `MapGrid::add_floor_item`：
  **会把 `e.drop_items` / `Gold` 写进地图**（`deaths[].1 = placed > 0`）。
- `bins/game-svr/src/ground.rs:592-622`（玩家 `CM_DROPITEM` 路径）**已经**按 T-19 复刻门：
  造 `item_id: 0` + `GroundMap::put_item` 保留 `item_id == 0 ⇒ false`。
  ⇒ **同一根因，两条路径口径不一致**（玩家丢弃已复刻、死亡掉落未复刻）。

## 3. 决定性新证据：golden 不是 oracle，是「探针意图」

`tests/parity/csharp/WorldGolden/Program.cs:344-366`（C# 侧 golden 探针的死亡结算）：

```csharp
foreach (ParityGolden.UserItem item in a.ItemList)
{
    if (ActorStub.GetDropPosition(envirs[mapId], a.CurrX, a.CurrY, 3, out int px, out int py))
    {
        envirs[mapId].AddMapObject(px, py, CellType.Item, item.Index, a); // ← 桩，绕过 AddItemToMap
        placed++;
    }
}
```

- 它调的是 **桩** `AddMapObject`（`Stubs.cs:376`），**没有** `Envir.AddItemToMap` 的
  `MapItem.ItemId == 0` 门，也没有 `MapItem`。
- 于是 golden 里出现落地物：`golden/combat.txt:36` `death 2 1 192 1 100` + `:37 floor 0 13 11 2`；
  `golden/session.txt:18` `death 2 2 192 -1 0` + 两条 `floor`。
- 这两份 golden 是 `tests/parity/tests/world_combat_parity.rs::combat_attacks_and_exp_match_csharp`
  与 `world_session_parity.rs::session_and_drops_match_csharp` 的判据。

**实测**（把 Rust 死亡路径按 §1 复刻成「不落地」后，四门禁里两条判据必红）：

```
rust  = death 2 0 192 1 100
csharp= death 2 1 192 1 100
```

⇒ 结论：`src/`（冻结 oracle）与「本项目 golden」在这条语义上**互相矛盾**；
二者必有一个是合同。这正是 AGENTS 硬约束③「行为等价优先于修 bug」与
「对拍基线即合同」在本项目里第一次正面冲突的点，**需 owner 裁定**。

## 4. 两条路（含影响面）

- **方案 A（对齐冻结 oracle / T-19 / T-25：死亡掉落不落地）**
  - 改：`crates/world/src/world.rs`（`settle_deaths` 不再落格、`place_floor_item` 删除；
    两条测试按不落地重写）——**本卡写集内**。
  - 还必须改（**不在本卡写集**）：`tests/parity/csharp/WorldGolden/Program.cs:344-366`
    按 §1 建模门（跳过 `AddMapObject`）→ 重新生成 `tests/parity/world/golden/{combat,session}.txt`。
  - 可一键套用：见本目录 `optionA-no-landing-align.patch`（我只做了写集内那半，
    在 `4139c2f0` 上 `git apply` 后只有两条 parity 判据因 golden 未同步而红）。
- **方案 B（保留 Rust 落地 = 探针意图；登记为偏差）**
  - 生产码零改动；新增白名单条目（**需 owner 签字**）= 「Rust 建模探针意图（落地），
    真实 `src/` 因 `MapItem.ItemId` 被注释而恒不落地 ⇒ 有意偏离，理由：world golden 合同 + `SM_ITEMSHOW` 可观测」。
  - 本卡随后可做 `SM_ITEMSHOW(610)` 下发（卡标题点名），但它**在真实 oracle 里不可达**、
    无 oracle golden 可比，只能按帧编码 + 探针逐字节自证。
- **无论 A/B**：`SM_ITEMSHOW(610)` 在 `src/` 里**不可达**（门在前），「实现它」都不是 1:1 复刻。

## 5. 为何本片零 `.rs` 改动（纪律）

- 这是**跨卡裁决**：方案 A 需同时改 `WorldGolden/**` 与 `world/golden/**`（不在本卡写集
  `crates/world/** + bins/game-svr/** + crates/protocol/**`）；单改 Rust 会留下两条 RED 判据，
  违反「四门禁全绿」。
- 单方面选 B 也是**新增偏差**（需 owner 签字的 whitelist 条目）。
⇒ 按卡 DoD①（「与盘点片冲突以盘点为准并**由 owner 修订本卡**」）**只交锚定 + 裁决请求**，
不擅自改语义。

## 6. 复现

```bash
cd <worktree>/mir2-rs
export CARGO_TARGET_DIR=E:/tmp/m2svr-dropside-target

# ① 证明 oracle 侧无 ItemId 赋值点（只读 src/）
grep -rn "ItemId" ../../src --include=*.cs | grep -v "== 0"

# ② 证明 golden 编码的是「落地」（探针桩绕过 AddItemToMap）
grep -n "^death\|^floor" tests/parity/world/golden/combat.txt | head
sed -n '344,366p' tests/parity/csharp/WorldGolden/Program.cs

# ③ 方案 A 的判据读数（先 apply optionA-no-landing-align.patch）
cargo test -p mir2-world --lib --offline
cargo test -p mir2-parity-tests --test world_combat_parity --offline   # 预期 RED（golden 未同步）
cargo test -p game-svr --offline
```

## 7. 残留物清单

- 本目录 7 件（README/01-04 + optionA patch + SHA256SUMS）。
- `E:/tmp/m2svr-dropside/optionA/no-landing-align.patch`（与仓库内同名文件同一内容，临时 staging）。
- `E:/tmp/m2svr-d2-target`（复用 D② 的隔离 target）。
- **未**改任何生产 `.rs`（工作树零 `.rs` 改动）；未起栈、未占端口；未改 `WorldGolden/**`、`golden/**`。
- 未 merge、未自签 review。
