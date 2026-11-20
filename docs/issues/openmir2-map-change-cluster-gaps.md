# openmir2-map-change-cluster-gaps

> 状态：**已实现（2026-11-20，`card/map-change-cluster`；第二方待签）** —— 见 §5。
> 立卡来源 = IT-09 回城卷对拍时的实测/源码核对。
> 两条缺口都挂在**共用换图路径** `bins/game-svr/src/lib.rs` 的 `drain_map_change`
> （及其帧构造 `bins/game-svr/src/sessionwire.rs` 的 `map_change_frames`）上——**不是**任何单条物品私有：
> 走路换图、脚本 `MAPMOVE`、魔号 21/传送、IT-09/10/11/13 的吃物品传送，全都经过它。

## 0. 证据来源（可重放）

`mir2-rs/tests/parity/m3/move-eat-probe` + golden
`mir2-rs/tests/parity/m3/fixtures/move_eat_vectors.json`：真 `Envirnoment`（合成 `.map`）、真
`ClientUseItems`、固定种子，排空 `MsgQueue` 后按出队序 pump `8062/10115/8097/8098/8100`。
oracle 实际泵出的帧序（两例 IT-09 一致）：

```text
635(SM_EAT_OK) → 800(SM_SPACEMOVE_HIDE) → 622(SM_WEIGHTCHANGED)
→ 633(SM_CLEAROBJECTS) → 634(SM_CHANGEMAP) → 766(SM_AREASTATE) → 54(SM_MAPDESCRIPTION) → 801(SM_SPACEMOVE_SHOW)
```

本仓 `drain_map_change` 产出的是 `633 → 634 → 801`（635/800/622 属吃物品动作本身）。
IT-09 的用例把「golden `pumped` 里剔除的 ident 恰等于 `[766, 54]`」写成断言（等式，不是豁免）。

## 1. 缺口 A：`[名|文件]` 覆盖时 `SM_CHANGEMAP` 尾应为**取图文件主干**

- **oracle 取值**：`src/GameSrv/Maps/MapManager.cs:66-96` 的 `AddMapInfo`——`MapFileName = sMapFileName`，
  其中 `sMapFileName` 是 `[逻辑名|文件主干](...)` 的 `|` 右半、或 `名字<文件主干>` 的 `<...>`，
  缺省 = 逻辑名；物理文件路径才是 `sMapFileName + ".map"`。随后 `BaseObject.SpaceMove`
  （`src/M2Server/Actor/BaseObject.cs:1134`）`SendMsg(RM_CHANGEMAP, 0,0,0,0, MapFileName)`，
  处理器 `src/M2Server/Player/PlayObject.Message.cs:1774-1779`
  `SendDefMessage(SM_CHANGEMAP, ActorId, CurrX, CurrY, DayBright(), processMsg.Msg)`（尾 = `EDCode(MapFileName)`）。
- **宿主位置**：`drain_map_change` 里 `let map_file = registry.name_of(ev.map_id)`（**逻辑名**）；
  `sessionwire::map_change_frames` 把它 `EDCode` 编码进 634 尾。
- **现网可达（不是理论）**：`E:/MirServer/M2GameSvr/Envir/MapInfo.txt` 有 10 条 `|` 形态，例如
  `[M100A|M100 …] NORECONNECT(G002) NORANDOMMOVE NORECALL NOPOSITIONMOVE DAY` ⇒ oracle 的
  `MapFileName = "M100"`、宿主发 `"M100A"`。
- **复现**：给 `move-eat-probe` 的注册表造一条 `map_name != map_file_name` 的条目（例如逻辑名
  `alias_map`、`map_file_name = move_map`），或在真栈上把玩家传送进 `M100A` 后比 `SM_CHANGEMAP` 尾。
- **客户端影响**：`SM_CHANGEMAP` 的尾是客户端用来定位/加载地图文件的字符串；发逻辑名而非文件主干
  ⇒ 客户端按 `M100A` 而非 `M100` 解析（是否命中取决于客户端是否二次查表/加后缀，**未在冻结客户端上实测**——
  这正是本条要收口的理由）。
- **为什么不在 IT-09 改**：改 `drain_map_change` 影响所有换图卡；IT-09 写集只到 `eat_move.rs` + 钩子。

## 2. 缺口 B：换图后缺 `766 SM_AREASTATE` / `54 SM_MAPDESCRIPTION` / `11029 SM_SERVERCONFIG`

- **oracle 连带帧**（`PlayObject.Message.cs:1774-1779` 的 `RM_CHANGEMAP` 处理器，未在本仓建模）：
  1. `SendDefMessage(SM_CHANGEMAP, …)`；
  2. `RefUserState()`（`src/M2Server/Player/PlayObject.Base.cs:270-286`）⇒
     `SendDefMessage(SM_AREASTATE=766, n8, 0,0,0)`，`n8 = FightZone|1 / SafeArea|2 / InGuildWarArea|4`；
  3. `SendMapDescription()`（`:887-895`）⇒ `SM_MAPDESCRIPTION=54`（`nMusicid` 来自 `MapDesc` 解析）；
  4. `SendServerConfig()`（`:941-973`）⇒ `SM_SERVERCONFIG=11029`（`Recog/Param/Tag/Series=0/0/0/0`，
     尾 = `EDCode(Config.ClientConf)`）；**门**：`SoftVersionDateEx == 0` 直接 return（探针的
     `PlayObject.SoftVersionDateEx = 0` ⇒ golden 里没有 11029；生产非 0 ⇒ 会发）。
- **宿主位置**：`map_change_frames` 只出 633/634/801。
- **客户端影响**：换图后区域状态（安全区/战斗区位）与地图描述/背景音乐不刷新；生产（`SoftVersionDateEx≠0`）
  还少一份服务器运行配置下发。
- **为什么不在 IT-09 改**：同缺口 A（共用路径）；且 `SM_SERVERCONFIG` 的尾是 `Config.ClientConf`
  全文，属配置面，需要单独定比较面。

## 3. 验收（修复卡 Definition of Done 草案）

- 两条缺口各自的 oracle 锚在**同一张换图卡**内实现：634 尾改用**取图文件主干**（registry 需提供
  `map_file_name_of` 或等价访问器）；`map_change_frames` 追出 766/54（+ 受 `SoftVersionDateEx` 门的 11029）。
- 对拍：扩 `move-eat-probe` 的 pump 面到全 8 帧；加一例 `[名|文件]` 覆盖（逻辑名≠主干）证明 634 尾；
  加一例 `SoftVersionDateEx≠0` 证明 11029 门。
- 回归：走路换图/`MAPMOVE`/魔号 21 既有 `drain_map_change` 用例全绿；把 IT-09 §9.12 的
  「剔除集 == [766,54]」等式改为「剔除集 == []」。

## 4. 边界

- 本卡**不改** `src/` 下 C# 参照实现；不动 `useitem.rs`/`eat_move.rs` 的吃物品语义。
- 冻结客户端实际如何消费 `SM_CHANGEMAP` 尾**未实测**（见 §1 影响）；修复时以「与 oracle 逐字节一致」为准，
  不以猜测的客户端行为为准。

## 5. 实现（2026-11-20，`card/map-change-cluster`）

**状态**：两条缺口（A 634 尾、B 766/54）均已实现并通过本地门；第二方 QA 待签。

### 5.1 改动
1. `crates/envir/src/registry.rs`：新增 `MapRegistry::map_file_name_of(map_id)`（取 `MapInfoEntry.map_file_name`）。
   **数据本就随 `loaded` 装载、只缺访问器**——"缺代码不缺数据"（`2d` 的离线数据源陷阱不适用）。
2. `bins/game-svr/src/lib.rs::drain_map_change`：634 尾改用 `map_file_name_of`；按 `flag_of`/`map_desc_of`
   组装 `sessionwire::MapVisual` 传进帧装配；缺输入 ⇒ `WRN` 并跳过 766/54（不编造）。
3. `bins/game-svr/src/sessionwire.rs::map_change_frames`：新增 `MapVisual{flag,desc}` 参数；
   在 634 之后、801 之前 push `SM_AREASTATE(766)`（`enter::area_state`）与 `SM_MAPDESCRIPTION(54)`
   （`enter::music_id` + `enter::gbk(desc)`），路由 `SelfOnly`（oracle `SendDefMessage`）。
4. **11029 `SM_SERVERCONFIG` 不在本卡**：probe 的 `SoftVersionDateEx=0` ⇒ golden 本无该帧、四张卡的
   `pumped` 也不含；其尾是 `ClientConf` 配置面，需单独定比较面与 conf 透传（另开单）。

**634 尾的输入类（重要）**：只对 **alias 图**（`[名|文件]`）变化；无 alias 时 `map_file_name == map_name`，
**逐字节不变**（既有 golden 全部属这一类，夹具未改）。

### 5.2 既有断言改写（逐处）
| 处 | 改前 | 改后 | 依据 |
|---|---|---|---|
| `eat_move.rs` 两处对拍器（`run_move_parity` / IT-13） | `modeled=[800,622,633,634,801]`、`registered_gap=[766,54]`，剔除集须 `==[766,54]` | `modeled=[800,622,633,634,766,54,801]`、`registered_gap=[]`，剔除集须为空 | 766/54 已接线，golden 里的两条现在逐帧比 |
| `lib.rs::t279_dress_eff_tests::npc_script_mapmove_bai_teleports_and_persists` | `[633,634,801]` | `[633,634,766,54,801]` | 脚本 MAPMOVE 走同一 drain，oracle 同处理器发连带帧 |
| `lib.rs::p22_move_to_home_broadcast_order_and_tick_phase` | `[800,653,633,634,801]` | `[800,653,633,634,766,54,801]` | 同上（P22 `MoveToHome`） |
| `lib.rs::drain_map_change_*` 两例 | 不变 | 不变 | 传 `registry=None` ⇒ 按设计不发 766/54（缺输入登记） |

### 5.3 新增测试
- `sessionwire::tests::map_change_appends_areastate_and_mapdescription`：序 633→634→766→54→801；
  766 `Recog`=标志位、54 `Recog`=MusicId、尾=`EDCode(GBK(MapDesc))`、两帧 `SelfOnly`。
- `eat_move::tests::map_change_634_tail_uses_alias_file_stem`：alias 图（逻辑名 `alias` / 文件主干 `stem`）
  ⇒ 634 尾=`EDCode(GBK("stem"))`；**对照**无 alias 图 ⇒ 尾=逻辑名。

### 5.4 牙（§31：还原后 sha 相等、`grep MUTATION`=0）
- **T1 删 766** ⇒ 红：`sessionwire::map_change_appends…`（`[633,634,54,801]` vs `[…766,54,801]`）与 alias 例（`[633,634,54]` vs `[…766,54]`）。
- **T2 删 54** ⇒ 红：同上两例（`[633,634,766,801]` / `[633,634,766]`）。
- **T3 634 尾改回 `name_of`** ⇒ **alias 例红**（尾=`EDCode("alias")` vs 期望 `EDCode("stem")`）、**无 alias 的
  `it09_town_scroll_host_parity` 仍绿**——"该红该绿"两半都有。

### 5.5 本地结果
- `cargo test -p game-svr --lib -- map_change` → `running 8`，**8 passed / 0 failed**；
- `cargo test -p game-svr --lib -- eat_move` → `running 18`，**18 passed / 0 failed**；
- 全量 `cargo test -p game-svr --lib` → `running 2227`，**2154 passed / 0 failed / 73 ignored**；`cargo fmt --all -- --check` rc=0。

### 5.6 登记（本卡不改）：生产 drain 的时序差异
`drain_map_change` 的生产调用点只有两处（`handle_move_cmd` 的 walk 臂与 turn 臂）。吃物品（IT-09/10/11/13）、
魔号 21（`MagSaceMove`）、P22 `MoveToHome`、脚本 `MAPMOVE` 只 `base_object_move_player` 记一条 `MapChange`，
**没有第三方 tick 级 drain** ⇒ 这些路径的换图簇（含 766/54）要等该玩家下一次 walk/turn 才 flush
（测试里显式调 drain，故对拍面看不到该时序差）。**影响**：上述路径的帧**及时性**（不改变帧内容/顺序）；
**另开单，不在本卡修**。
