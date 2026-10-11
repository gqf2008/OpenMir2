# 卡 `openmir2-m3-gamesvr-save` 证据（Rust game-svr「进世界存档」缺失）

**patch**：待挂（`patch --base refs/heads/master --head refs/heads/worktree-m2-gamesvr-host`）
**base**：`refs/heads/master`（交付时 = `0a59fa48`，本分支已 merge）
**head 提交**：`ff0ff63c`（本片源码 + 证据）→ merge master `f982940a`
**worktree**：`E:\Users\gxh\Documents\GitHub\OpenMir2\.claude\worktrees\m2-svr`

## 1. 交付内容（DoD ①③）

oracle 路径（已逐行核）：

```text
WorldServer.SaveHumanRcd            src/GameSrv/Word/WorldServer.cs:1077-1088
  └ MakeSaveRcd                     WorldServer.cs:1231-1360  （原始记录 + 实时增量 → CharacterDataInfo）
     └ M2Share.FrontEngine.AddToSaveRcdList → PlayerDataService.SavePlayerRcd
        └ DB_SAVEHUMANRCD(101)      src/DBSrv/Services/Impl/DataService.cs:207
           └ PlayDataStorage.Save.cs（characters / characters_ablity / characters_item /
              characters_bagitem / characters_storageitem / characters_item_attr）
```

触发点：`MakeGhost()`（退出世界）——oracle 在 `RunNotice` 的 `BoSoftClose`/`BoKickFlag` 支与
`CloseUser` 断线支都会走到 `SaveHumanRcd`。

Rust 侧新增（写集 = `mir2-rs/bins/game-svr/**`）：

| 文件 | 内容 |
| --- | --- |
| `src/record.rs` | `encode_save_character_data(account, chr_name, &PlayerRecord)`：`SaveCharacterData{Account,ChrName,CharacterDataInfo}` 的 MemoryPack 编码（用本文件既有 `Writer`，与 DBSvr 侧 `crates/storage::SaveCharacterData` **同布局**） |
| `src/dbclient.rs` | `save_human_rcd_request(query_id, packet)`：与取档同一个 `ServerRequestData` 信封，`Message.Ident = DB_SAVEHUMANRCD(101)` |
| `src/main.rs` | `SavedHuman{account, record, enter_map}`（进世界时留下原始记录）+ `build_save_record`（`MakeSaveRcd` 的实时增量：`CurMap/CX/CY/Gold/GameGold/IncHealth/IncSpell/HungerStatus/Abil/HumItems/BagItems`）+ `save_human_rcd`（发 101、登记 `save_outstanding`）；在 `Closed`（断线）、`drain_kicks`（重复进世界踢旧）、`drain_soft_close`（账号级顶号）三处 **`world.disconnect` 之前**调用；`use_items_from` 补 oracle 的 `HumItems[Helmet]/[Necklace]` 交换（`WorldServer.cs:1163-1176`，与 `MakeSaveRcd` 的 `:1302-1303` 互逆） |
| `src/homemap.rs` | `new_human_spawn_with_home`：同一条 3 抽路径，同时返回出生点原坐标（`MakeSaveRcd` 要写 `HomeMap/HomeX/HomeY`） |
| `src/main.rs`（新角色分支） | 补上 issue 登记过的缺口：`WorldServer.cs:262-275` 的新角色 `Abil` 初始化（`Level=1 / DC=MC=SC=(ushort)MakeLong(1,2)=1 / HP=MP=MaxHP=MaxMP=15 / MaxExp=100 / MaxWeight=30`）+ `HomeMap/HomeX/HomeY/CurMap/CurrX/CurrY` 落记录 |
| `src/main.rs`（启动） | 载入 `Exps.conf` 的 `NeedExps` 供存档 `Abil.MaxExp = GetLevelExp(Abil.Level)`（`PlayObject.cs:1310`）——oracle 在登录后这么覆盖，`MakeSaveRcd` 用之 |

`MakeSaveRcd` 的三处「照抄不修」：
1. 存档 `Abil` 从**新建** `Ability` 开始，只拷 `Level/HP/MP/MaxHP/MaxMP/Exp/MaxExp/Weight/MaxWeight/
   WearWeight/MaxWearWeight/HandWeight/MaxHandWeight`，`AC/MAC/DC/MC/SC` 留 0（`WorldServer.cs:1239-1275`）。
2. `HumItems[Helmet] = UseItems[Necklace]`、`HumItems[Necklace] = UseItems[Helmet]`（`:1302-1303`）。
3. 空槽不是 `null` 而是 `HUtil32.DelfautItem.ToServerItem()`（`Desc = new byte[14]`，Index/MakeIndex/Dura=0）。

## 2. 用例与红检（DoD ③）

- `record::tests::save_character_data_roundtrips_through_dbsrv_type`：`encode_save_character_data`
  的输出必须被 **DBSvr 侧同一类型** `mir2_storage::SaveCharacterData` 解出，且重编码回**同一字节**
  （两侧同 MemoryPack 布局的机械证明）。
- `dbclient::tests::save_request_carries_db_savehumanrcd_and_packet`：用收侧解析器解回请求帧，
  `Message.Ident == DB_SAVEHUMANRCD(101)`、`Packet == 传入的 SaveCharacterData 编码`。
- `tests::save_record_reflects_live_state_and_item_layout`：底稿 = oracle 全字段向量 `stage6`
  （`tests/fixtures/player_data_full_oracle.txt`）；断言实时坐标（世界实体 11/22）、`Gold=649`、
  `GameGold=77`、`IncHealth/IncSpell/HungerStatus`、`BagItems` 46 槽补位、`HumItems` 13 槽 +
  Helmet/Necklace 交换。

**红检（改坏 ⇒ 必红；原始件见 `red-*.txt`）**：

| 红检 | 改法 | 结果 |
| --- | --- | --- |
| `red-gold.txt` | `rec.data.gold = sg.state.gold` → `= 0`（漏写 Gold 列） | `save_record_...` FAILED（exit 101） |
| `red-bag.txt` | `bag_items_for_save` 不写物品（全默认物品） | 同上 FAILED（exit 101） |
| `red-ident.txt` | `save_human_rcd_request` 的 ident `101` → `100` | `save_request_...` FAILED（exit 101） |

三次改坏后均**还原**，`git status` 只剩本次三处源码 + 证据目录（无半路残留）。

## 3. 门禁与 T-27 preflight（DoD ①⑤，隔离 `CARGO_TARGET_DIR=E:\tmp\openmir2-m2svr-target`，全 `--offline`）

**合并 master `0a59fa48` 后的树**：

| 门禁 | 命令 | 结果 |
| --- | --- | --- |
| fmt | `cargo fmt --all -- --check` | rc=0（`gates-fmt.txt`，空 = 无 diff） |
| clippy | `cargo clippy --workspace --all-targets --offline -- -D warnings` | rc=0（`gates-clippy.txt`） |
| test | `cargo test --workspace --offline` | **73 suites / 560 passed / 0 failed / 16 ignored**（`gates-test-workspace.txt`） |
| msgcodegen | `cargo run -p msgcodegen --offline -- --check` | rc=0，604 条（`gates-msgcodegen.txt`） |

新增/改动用例 **3 连跑**：`gamesvr-3run-1..3.txt`，每次 `123 passed / 0 failed`（unit）+
`13 passed / 0 failed`（relay）。

**T-27 断线存档机判护栏**（`openmir2-m3-bag-equip-alignment` 第十三格，已随 master `0a59fa48` 合入）：

- 数据面：`rg -n DB_SAVEHUMANRCD mir2-rs/bins/game-svr` 现在**命中**（`dbclient.rs:145` 等；
  改动前 0 命中）——见 `t27-grep.txt`。
- 夹具自检（不占端口/不起栈）：`run_bag_equip_pair.ps1 -SelfCheckNoStack` rc=0，
  9 条全绿，SELFTEST ⑧ 即 T-27 preflight 三态机判（空目录 false / `DB_SAVEHUMANRCD` true /
  `DBR_` 回包不算）——见 `t27-selfcheck.txt`。

## 4. 真机（DoD ②）——**本片未跑端到端，如实登记**

- 未起栈。开工前后实拍端口：5000/5100/5500/5600/5700/6000/7000/7100/7200 **全空**（未占窗口）；
  3306 = 共享 MySQL（pid 4540）——见 `ports-preflight.txt`。未 `/Stop`（无栈可停，**窗口已释放**）。
- **为什么未跑端到端**：T-14 的夹具口径片已随 master `0a59fa48` 合入（关挂机 + 同口径 peer 护栏 +
  两相 `-PinStartPose` + T-27 机判），**但 owner 明示「接线前不开 M3 真机窗口」**
  （m3bag 卡 `f139fb75` / `af689dab`：端到端等 `openmir2-m3-gamesvr-save` 合入 + owner 发窗口）。
  本片把该前置（存档）做出来了；端到端同序列对拍留给 owner 发窗口后的 T-14 复跑。
- **跑起来会差在哪（预先如实列，不静默）**：
  1. `characters_ablity.Hp/MAxHP`：oracle 基线是 `19/19`，而本宿主 `RecalcAbilitys` 的 `cur_hp`
     取记录 `Abil.HP`（新角色 init = 15）⇒ 若 oracle 的 19 来自本卡未复刻的某个写入点，该列会差；
     **需要夹具跑一遍定案**（登记）。
  2. `GetUpgradeStdItem` 的 Desc 升级差量、buff/技能对能力的加成都未移植（沿用既有登记）。
  3. 周期存档 `SaveHumanRcdTime`（`WorldServer.cs:698-702`，默认 10 分钟）**未做**（登记）；
     本片只做 `MakeGhost`/断线那一条（DoD ① 的「至少」）。
  4. `FrontEngine.InSaveRcdList` 的「保存中」生命周期未建模（`begin_save/end_save` 仍是死代码）⇒
     保存期间的并发重登不会被 requeue（登记）。

## 5. 边界与残留物

- 未动 `src/**`（C# oracle）、`crates/**`、`MirClient`、表结构/脚本语法/数据格式；未动别人的卡状态。
- 唯一跨出 `bins/game-svr` 的改动是**读** `crates/data`（`ExpsConfig`/`MAX_CHANGE_LEVEL`）与
  `mir2_formula`/`mir2_storage`（既有依赖），无 `crates/**` 文件改动。
- 证据入库路径：`mir2-rs/tests/parity/m3/evidence/gamesvr-save-20261012/`（本卡写集含
  `mir2-rs/tests/parity/m3/**`）；`SHA256SUMS.txt` 用仓库口径（LF 归一 = git blob），
  `tools/evidence-hash/verify_sha256sums.py --check-extra` 复核 GREEN。
