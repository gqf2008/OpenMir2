# 窗口收口台账（2026-10-25）

- 卡：`openmir2-window-closure-ledger`（tier **L**，owner `openmir2-tester-1`，worktree `.worktrees/tester-1`，branch `card/window-closure-ledger`）。
- 定位：**条件② 的窗口执行/关卡依据**。本台账只登记「已合并、R 面待真机窗口」的卡；**登记 ≠ 已有真机证据**，
  每行按窗口实跑结果关闭（`window_required` 卡保持 `in-progress` 直到该行证据入卡）。
- 快照锚：`rcard-evidence-plan` 快照 **`87dc9847b`**；首版覆盖区间 **`87dc9847b..a3bf5340f`**（6 行）；**追加区间 `a3bf5340f..97eb5365d`**（+3 行：castle-official `02f83a1a2` / castle-official-actions `a62faeb1c` / msgqueue `4b9ce64ca`）。
- 口径来源：`docs/rcard-evidence-plan-20261025.md` §5（W1/W2/W3 轮次与证据形态）、各卡自己的 R 级窗口项章节。
- 红线：不改 `rcard-evidence-plan` 既有行（只引用）；不把未合并写成已合并；不把「登记型 PASS」写成「已有真机证据」。

## 1. 逐卡登记（首版 6 行 + 2026-10-25 追加 3 行；合计 **9 行**）

| # | 卡 id | merge oid | W 项（本台账命名，来源=各卡报告窗口章） | 场景 / 夹具 | 证据形态 | 关卡命令 | 窗口状态 | 计划轮次 / 漂移 |
|---:|---|---|---|---|---|---|---|---|
| 1 | `openmir2-castle-merchant-tax-wiring` | `99156c2c6`（approve reviewer-21 `e523aacee1`；reviewer-20 approve） | **W-TAX-1** 四路径双栈（买/卖/修/升级）；**W-TAX-2** `CastleGetAllNpcTax=1` 窗口副本（税基恒 `UpgradeWeaponPrice`，多城升序）；**W-TAX-3** 前置（`CastleMerchant=1` 不在攻城区 ⇒ 零税；非城主仍收税）；**W-TAX-4** 日上限/金库上限/`SaveTick` 10min 门 | 现有场景**不覆盖**城堡商人税；需窗口专用配置副本（内容冻结，不改 `E:/MirServer`）+ 手工四操作，或新 castle 税场景 | 协议帧 diff=0 + `Castle/0/SabukW.txt` 的 `TotalGold`/`TodayIncome`/`SaveTick` 字段 diff=0（需 ⑧-1 C# 时钟注入，同 tick）+ DB 后态快照 | 无现成 `run_batch` 场景；窗口手工 + `gate_shadow_proxy` 抓帧 + 文件 sha256/字段比对（模板见 `docs/castle-merchant-tax-wiring-20261025.md` §6） | window_required（in-progress） | 原计划 **W2** castle fixture（§5 的 castle fixture 桶）；漂移：fixture 尚未建，故暂不能进 run-3 的 17 场景 |
| 2 | `openmir2-loader-fields` | `9a7b8c798`（approve reviewer-21 `c9089db97f`） | **W-LF-1** 出生点 11 键取值与落点（map/x/y/朝向；朝向来自 `humData.Dir`，不经 StartPoint）；**W-LF-2** 5 个新列的消费（`MakeSafePkZone`/`InSafeZone` 后续片） | `login_only` 是**既有角色**登录（alt profile `char_name=winplay0`，`card_window.py:476`），**不触发新建角色出生点** | 协议首包地图/坐标/朝向 diff=0（需新角色路径）；W-LF-2 待接线后按安全区判定行为对拍 | `login_only` 只覆盖「既有角色进图」腿；W-LF-1 需 probe 加 `newchr` 步骤（`CM_NEWCHR=101` 只是协议常量，tools/probe、tools/window、card_window.py 均 0 命中）或窗口手工建号 | window_required（in-progress） | **部分/缺（不进 W1）**；W-LF-2 无消费点 ⇒ 不列窗口 |
| 3 | `openmir2-script-playdice-openybdeal` | `bff927e11`（approve reviewer-21 `54eeabc3c4`；reviewer-20 approve） | **W-PD-1** GAMEPOINT 真机落库 + `SM_GAMEGOLDNAME(5008)` 字节 + 重连读值；**W-PD-2** 回退标签排水帧序——**部署件按字面 `@back` 非目标**：`grep -r "@back>" E:/MirServer/M2GameSvr/Envir` = **0**，`@back2>` = **2**（`真爱-G002`/`真爱666-G002`）；待 dev 澄清是否指 `@back2`。W-PD-3/4（构造级已钉，非窗口项） | GAMEPOINT：赌博/功能 NPC（如 `真爱-G002`）——**现有 17 场景不含**；`@back`：部署无该标签（`npc_script_chain` 明确不用） | DB 后态 `characters.GamePoint` diff=0 + 5008 `Param/Tag` 逐字节 + 重连后 `$GAMEPOINT`；`@back` 若按字面 ⇒ 构造级/非目标 | GAMEPOINT 需新场景或 `gate_shadow_proxy` 手工；`@back2` 需新场景（点 `真爱-G002` 后选 `@back2`） | window_required（in-progress） | **W2**（单卡新场景桶，GAMEPOINT）；`@back` 不列窗口（构造级/非目标）；⑩① 面板标签+GAMEGOLD 组合部署 0 处 ⇒ 不列窗口项 |
| 4 | `openmir2-clearmapmon-impl` | `f08496192`（approve reviewer-21 `ea4c42d4f8`） | **W-CM-1** T118 清场（≥2 只同名 + 宠物/召唤物）+ MONGENEX 刷怪；**W-CM-2** Q001（`骷髅精灵5`、(10,10)、`TimeRecall 5` 回收时序）；**W-CM-3** RNG 流水位置（部署当前不可观测，观察项）；**W-CM-4** `regen` 失败门/玩家切图相对序/`Invisible` 语义 | 部署地图 T118 / Q001 的脚本路径（手工触发 `CLEARMAPMON`/`MONGENEX`） | `SM_DISAPPEAR(30)` 帧序 + 实体集合/AOI 差集逐字节 + 刷怪位置（±2 格）/数量 diff=0；抽数动作在后的场景加 `MIR2_RNG_LOG` site/order 账本 | 模板（`docs/clearmapmon-impl-20261025.md` §4）：`python -I -B tools/gw-capture/gate_shadow_proxy.py --out E:/tmp/clmon/sel --map 7101=127.0.0.1:7100 --gate sel --direction s2c`，跑完 `--verify`；C# 栈同命令重跑按帧 sha256 对拍 | window_required（in-progress） | 原计划把 CLEARMAPMON 列为「需实现」审计（L 卡 `ff8c0dc9a`），实现后升 R ⇒ 归 **W2**（GM/脚本新场景）；漂移：W2 轮次编号来自计划 §5，本卡不在原 49 行 W1 清单内 |
| 5 | `openmir2-m3-castle-siege-state`（T-127 残余片） | `263e77d7e`（approve reviewer-21 `da06ee4945`） | **W-T127-1** 开战链帧序（`SM_SYSMESSAGE`/`SM_DIGDOWN`/`SM_USERNAME` 及相对序）；**W-T127-2** 结束预告/超时停战两帧；**W-T127-3** N1 丢弃（攻城区观察者收不到 `SM_USERNAME`）；**W-T127-4** 多城广播次序 `[0,1]`；**W-T127-5** `SS_204`/`SS_212` planes 零副作用 | 城堡战语境（`castle_zone` / castle fixture）；20 点整点或 `StartCastlewarTime` 注入到当前小时 | 全服 `SM_SYSMESSAGE` 文本/颜色/收件人 diff=0；`SM_DIGDOWN`/`SM_USERNAME` 帧与相对序 diff=0；可注入时钟（`CastleNow`）压到分钟级 | 无现成 `run_batch` 场景；需 castle fixture（未建），窗口内以 `gate_shadow_proxy` 抓帧 + 时钟注入 | window_required（in-progress） | 计划 **W3**（`castle_zone`）；漂移：`castle_zone` 未解封（rcard plan §5 W3 的 15 行桶），故不进 run-3 首轮 |
| 6 | `openmir2-inbound-dispatch-red-batch`（R 片 `card/inbound-dropgold-rng`） | `a3bf5340f`（approve reviewer-21 `e26812b48`） | **W-DG-1** `CM_DROPGOLD` 零抽数复拍（≥`CanDropGold` 且 `< Gold`）；**W-DG-2** `CM_BUTCH` 两抽流位（16/201，`SM_SKELETON(33)`→`SM_BUTCH(637)`） | `drop` 场景（`CM_DROPGOLD` / `CM_BUTCH`；部署需动物尸体） | 653 一条、`Recog` 相同、地面无金币、后续 RNG 流水账本逐位；Butch 帧序 + 尸体 `BodyLeathery/MeatQuality/Skeleton` 落值 + 同 seed 流水账本 | 模板（`docs/dropgold-butcheffect-rng-20261025.md` §5）：`gate_shadow_proxy --out E:/tmp/dropgold/sel --map 7101=127.0.0.1:7100 --gate sel --direction s2c`，跑完 `--verify`；C# 栈同命令重放 | window_required（in-progress） | 计划 **W3**（drop 场景/夹具桶）；漂移：R 面经 review `e26812b48` 定性为「**窗口验证**」而非「实现修复」（零抽数属 oracle 事实） |

| 7 | `openmir2-castle-official-design` | `02f83a1a2`（approve reviewer-20 `b5e7bb43`） | **W-CO-1** `@main` 页逐字节（mask `$DATETIME`）；**W-CO-2** `@treatdoor` 0 帧 + 2 行日志；**W-CO-3** `@repairdoor`/`@repairwalls`/`@hirearchers`/`@commandorder`/`@day`/`@aboutwar`/`@listwar` 文本与价格；**W-CO-4** Click（普通玩家、有城）零帧；**W-CO-5** 动作段不可达确认；**W-CO-6** 注入城主/`CanJmpScriptLableMap` 后复拍 open/close/repair 帧与 60s 门（**需 owner 批准测试专用注入**） | 固定 NPC `沙巴克城堡管理`（`Npcs.txt:4`，0150）；`老人-3`/`比奇国王-0122` 的说词页 | `SM_MERCHANTSAY` 文本 diff=0（mask `$DATETIME`）+ 服务端日志行数 + `SM_SYSMESSAGE` 红字；W-CO-6 另需 `SM_DIGUP/DIGDOWN` 与 60s 严格 `>` 时序 | 模板：`card_window.py` 起双栈后 `probe --scenario`（无现成场景，手工 `dlg_select`）+ `gate_shadow_proxy` 抓帧 | window_required（in-progress） | 计划 **W3**（castle fixture / `castle_zone` 未解封）；漂移：W-CO-6「测试专用城主注入」需 owner 授权，不解决则该项跑不了（见 §2 关联） |
| 8 | `openmir2-castle-official-actions` | `a62faeb1c`（approve reviewer-20 `67ba3049`） | **W-CO-6**（同上：注入城主后复拍 open/close/repair 的帧与 60s 门）+ 帧体口径 **Light/Feature 取展示层**（`Series=MakeWord(Dir,Light)`、尾 `Param1=GetFeature`） | 同 #7；构造级夹具 `castle_official_action_*`（3+1 条）只作回归，不当真机证据 | 同上 + `SM_DIGUP/DIGDOWN` 帧体逐字节（Feature/Light 非零） | 同 #7 | window_required（in-progress） | **缺口（未实现，不阻塞构造级）**：`@@castlename` 缺 SabukW 写侧、`@@sendmsg` 广播支需 `BoSendMsgFlag`；动作段未接 1011 漏斗（R-0 下为死代码）⇒ 窗口只能验证「不可达确认」，不能验证真动作 |
| 9 | `openmir2-script-handler-msgqueue`（R 片） | `4b9ce64ca`（approve reviewer-21 `f9d5f10f`） | **W-MQ-1** 面板 + 脚本帧同 handler 的序（`SM_MERCHANTSAY(643) → SM_MENU_OK(767) → SM_WINEXP(44) → SM_SENDGOODSLIST(645) → SM_GAMEGOLDNAME(5008)`）；**W-MQ-2** 同标签 3 条同 ident 脚本帧的出队 `[0,2,1]`（四叉堆序，非入队序）；**W-MQ-3** 发送者先于收件人（`SENDMSG 0` 下发送者 `SM_SYSMESSAGE` 先于收件人）；**W-MQ-4** `@getback` ≥51 件 ⇒ 多页 `SM_SAVEITEMLIST` 按页序连发（不被同优先级重排） | `getbackupgnow` 链（`Market_Def` 修理商脚本）；`@dealybme` 开窗帧 | 帧序 diff=0（逐帧 ident + 尾文本）+ `getback` 多页顺序 | 模板：`card_window.py` 起双栈 → `probe --scenario <getback 链>` → `compare_runs.py` 帧序对拍；`@dealybme` 需 selloff 场景 | window_required（in-progress） | 计划 **W3**（帧序桶）；**缺口**：`@dealybme` 开窗帧仍在 selloff 自有 emitter（未并入统一排水）⇒ 该子项跑不出统一序，需后续片 |

> W-MQ 编号与源报告 `docs/script-handler-msgqueue-20261025.md` §5 **逐字一致**（台账不自造编号；如需台账自有命名另起 `L-MQ-*` 并写映射）。

> 证据形态词汇沿用 `rcard-evidence-plan` §5：协议 `diff=0`、`db_snapshot`（九表）、`MIR2_RNG_LOG` site/order、
> 文件 sha256、构造级断言。**「构造级断言已绿」不等于真机证据**，第 3/4/6 行的构造用例不替代窗口行。

## 2. W 项与计划轮次对齐（acceptance ②）

- **W1（run-3 的 17 场景）覆盖结论**：本 6 张**没有一张能被 W1 完整覆盖**——#2 的 `login_only` 是既有角色登录（W-LF-1 需 `CM_NEWCHR`/手工建号）；#3 的 GAMEPOINT 需新场景、`@back` 为部署非目标。W1 只提供「既有角色进图」腿（#2 的部分）与 `npc_script_chain` 语境（#3 的相邻腿，不含 `@back`）。
- **W2（新场景/夹具第一批）**：#1 商税（castle 税场景）、#3 GAMEPOINT（单卡新场景）、#4 CLEARMAPMON（GM/脚本路径）。
- **W3（夹具/不可达第二批）**：#5 T-127（`castle_zone`）、#6 DROPGOLD/BUTCH（drop 场景）。
- **追加 3 行的轮次**：#7/#8 归 **W3**（castle fixture / 城主注入未解封）；#9 归 **W3**（帧序桶）。
- **与 `docs/window-kit-coverage-20261025.md` §2 的 P0/P1 关联（哪条不解决就跑不了）**：
  - #7/#8 的 **W-CO-6** 依赖「测试专用城主注入」（owner 授权）+ P1「无 castle fixture」；不解决 ⇒ 只能验「不可达确认」（W-CO-4/5），open/close/repair 帧跑不出来。
  - #7 的 `$DATETIME` 比对需 mask（P0「C# 无 `MIR2_CLOCK_*`」相关）。
  - #9 的 `@dealybme` 子项依赖自身缺口（selloff 自有 emitter），与 kit 的 P0 无直接关系，但帧序对拍需 `-RngLog`/抓帧模板（P0「私栈未接 `-RngLog`」影响 RNG 面观察）。
- **漂移登记**：原计划 §5 的 W1 是 49 行卡面清单，本台账 6 张卡中只有 2 张的 W 项落在 W1；
  其余 4 张是「今日新合并的 R 片」或「原计划外审计升 R」，轮次按 §5 的桶归位，不新增轮次编号。
  `pickup_item` 已封存（T-19/T-25），本台账不含该场景。

## 3. 复算命令（acceptance ③）

```sh
# 1) 快照之后的 tier=R 合并集合（与下表 6 行对账；--first-parent 避免把分支内部提交算进来）
git log --first-parent --merges 87dc9847b..a3bf5340f --format='%h %s' | grep 'tier=R'
# 首版期望 6 行：99156c2c6 / 9a7b8c798 / bff927e11 / f08496192 / 263e77d7e / a3bf5340f
# 追加 3 行（不同区间，单独跑）：git log --first-parent --merges a3bf5340f..97eb5365d --format='%h %s' | grep 'tier=R'
# 期望：02f83a1a2（castle-official）/ a62faeb1c（castle-official-actions）/ 4b9ce64ca（msgqueue）

# 2) goal_gate 条件② 的 R 卡分桶（快速模式，只读）
python -I tools/goal/goal_gate.py --json
```

实测（`a3bf5340f` 上的快速模式，快照 `87dc9847b`）：R 卡总数 **213**；桶分布
`closed 12 / window_required 145（另未合并 2）/ whitelist 48 / adjudicated 3 /
merged_not_closed 需人工 1（G11）/ 其它需人工 0`。
**213 是全部 R 卡的状态桶，不是合并数**；上列桶和 = 211，另 2 张为 `not_merged`（`openmir2-castle-official-actions`、`openmir2-script-handler-msgqueue`）。本台账只登记其中「已合并、待窗口」的 6 张增量卡。

## 4. 后续合并追加

新 R 卡合并后在本节追加一行（格式同 §1），并在 §1 表尾追加：

| 卡 id | merge oid | W 项 | 场景/夹具 | 证据形态 | 关卡命令 | 窗口状态 | 轮次 |
|---|---|---|---|---|---|---|---|
| _（待追加）_ | | | | | | | |

已知候选（合并后按实际 W 项补登）：`openmir2-castle-official-design`——**设计片已合 `e0bbcde61`；R 实现片已合 `02f83a1a2`**（tier R、window_required；含片内提交 `12a853fba`），W 项待补登本台账。

## 5. 已知项（不阻塞窗口，登记备查）

- **preflight 失败项**：`preflight_private_stack.py` 6 pass / 2 fail（缺 release 二进制、`start-rust.ps1` 指 dev-2），
  `preflight.py --profile alt|window-magic` 各 1 fail（`Debug-Fallback`）——全部由 `run_batch.sh` 首步
  `rebuild-server.ps1 -SyncStartRust` 覆盖，**开窗前不必单独重建**。
- **5700 的一次性观测**：2026-10-25 04:00 快照曾见 `5700` 未监听；reviewer 04:20 实测十端口全在听（DBSrv pid 6808 在跑）⇒ 该格为**瞬时观测**，非稳定偏差；窗口仍走私有栈，不依赖它。
- **故障期门禁口径**：`stale_pointer_gate` 必须 `--ref master`（默认 `refs/remotes/walgit/master` 在故障期是冻结值）。
- **补推清单**：`card/window-run2-castle-list-caliber`（head `fa24260b0`，runbook §0.6）待 walgit 恢复后补推。

## 6. 门禁（本卡：只新增本文档）

7 项卫生门禁（`stale_pointer` 用 `--ref master`）+ `git diff --check` + `merge_tree_helper` 结果写进 patch body；
`dynamic_count_stamp` 对本文件实测 rc=0（§3 的计数行带快照锚 `87dc9847b`）。
