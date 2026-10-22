# 对拍批次 1：login_only 双栈真机 run（2026-10-23 05:03–05:05）

- 卡：`openmir2-dualrun-window-burndown`（owner `openmir2-worker-24`）
- 窗口：05:03:35 起 C# 栈 → 05:04:10 跑 probe → 04:04 停 → 05:04:31 起 Rust 栈 → 05:04:36 跑 probe → 05:05 停；两侧端口全释放后才动手，跑完 `Get-Process` 残留 = 0。
- 方法：同端口串行（`E:/tmp/conformance/` 下 `start-csharp3.ps1` / `start-rust.ps1`），同一账号 `winplay0`、同一服务器名 `热血传奇`、同一场景 `tools/probe/scenarios/login_only.json`。

## 1. 命令与产物

```sh
# C# oracle 侧
python -I -B tools/probe/mir2_probe.py --login-gate 127.0.0.1:17000 \
  --account winplay0 --password winplay0 --server-name 热血传奇 --char-name winplay0 \
  --scenario tools/probe/scenarios/login_only.json \
  --out E:/tmp/conformance/runs/w24-csharp-login --timeout-ms 20000
# → exit=0 frames=29

# Rust 侧（首跑同样显式带 --game-gate 17600）
python -I -B tools/probe/mir2_probe.py --login-gate 127.0.0.1:17000 --game-gate 127.0.0.1:17600 \
  --account winplay0 --password winplay0 --server-name 热血传奇 --char-name winplay0 \
  --scenario tools/probe/scenarios/login_only.json \
  --out E:/tmp/conformance/runs/w24-rust-login --timeout-ms 20000
# → exit=0 frames=30

python -I -B tools/conformance/compare_runs.py \
  --oracle E:/tmp/conformance/runs/w24-csharp-login \
  --rust   E:/tmp/conformance/runs/w24-rust-login \
  --out    E:/tmp/conformance/runs/w24-login-report.json
```

| 产物 | sha256 前缀 |
|---|---|
| `runs/w24-csharp-login/frames.ndjson` | `ad2b357be761219bd8fd0c49…` |
| `runs/w24-rust-login/frames.ndjson` | `f2f3f5bcae5ad9b5221aa69d…` |

## 2. 帧序列（ident 顺序）

```
C#  (29): 529 530 520 525 658 51 50 41 213 11029 42 766 54 5008 1106 52 752 811 46 10 41 654 657 657 42 100 100 100 100
Rust(30): 529 530 520 525 658 51 50 41 213 11029 42 766 54 5008 1106 52 752 811 46 [14] 10 41 654 657 657 42 100 100 100 100
```

## 3. 差异（按证据强度排序）

1. **Rust 多一条 `SM_HIT(14)`**（idx 19，紧跟在 46 之后、10 之前）。去掉这条后**两侧 ident 序列完全相同（29 = 29）**。
   对应卡 `openmir2-npc-hit-frames`（「固定 NPC 的 SM_HIT(14) 诊断——Rust 抓到、C# 未见」）：本批次用无头对拍复现了该现象。
2. **两条系统公告顺序颠倒**（ident 100）：
   | 位次 | C# | Rust |
   |---|---|---|
   | 26 | 「禁止使用任何外挂、加速、辅助、脱机…」 | 「唯一客服QQ：79311185 唯一担保QQ：79311185」 |
   | 27 | 「唯一客服QQ…」 | 「禁止使用任何外挂…」 |
   ⇒ 内容两边都在，**发送次序相反**（时序差，R 级口径）。
3. **`SM_ABILITY(52)` body 长度 54 vs 78**（C# 54 / Rust 78）：与首跑一致的布局对 (54,78)，
   规则表白名单仍是 `{40,58}` ⇒ 属规则缺口（`conformance-rules` 车道），不等于行为差异。

## 4. 比较器判定

- `compare_runs.py` 原始输出：`DIFFS: 9` / `VERDICT: UNDECIDABLE (exit 2)`，
  UNDECIDABLE 原因是 **ident 14 在规则表里没有条目（UNCLASSIFIED）**——规则缺口，不是"通过"。
- 9 条 diff 中 7 条是**位置错位**（idx 19 起整体后移一位），根因即差异 1；另 2 条是错位后错比的 100。

## 5. 后续

- `rules.json` 需补 ident 14（以及首跑已知的 100/520/525/529/530/654/657、52 的 (54,78)）——归 `conformance-rules` 车道，本卡只提供证据。
- 公告顺序差异需单独立卡或以 `comment` 挂到相关卡（R 级：时序）。
- 下一批：同窗口跑 `gm_time` / `gm_batch`（无需 `--var`），再按首次 run 的 `recog` 填 `--var` 跑 NPC/商人/背包族。

## 6. 追加（05:06–05:09 第二次窗口）：`gm_time` 两侧都收不到回包

同一窗口内改跑 `gm_time`（`CM_SAY(3030)` 发 `@时间`，等 `SM_SYSMESSAGE(100)`）：

| 侧 | 尝试 | 帧数 | 结果 |
|---|---|---:|---|
| C# | 默认（softclose） | 30 | `steps[1]: timeout waiting for ident(s) [100] after 3000ms` |
| C# | 前置 `sleep 1500` | 30 | 同上 |
| C# | `--finish close` | 30 | 同上 |
| Rust | `--finish close` | 31 | 同上 |
| Rust | 前置 `sleep 1500` + `--finish close` | 32 | 同上 |

- `CM_SAY` 确实发出（`run.json.chain.sends` 里 t≈529ms 有 ident=3030），两栈都**没有任何 100 回包**。
- C# GameSrv 日志在连跑之间反复出现 `角色数据保存中,稍后重新读取. Account:winplay0`，
  疑与 `CM_SOFTCLOSE` 的 BoReconnection 语义（玩家留在世界里等重连）叠加有关；改用 `--finish close` 未改变结果。
- 结论：**gm 族当前不可作对拍证据**，先要解决「进世界后 `CM_SAY` 何时/怎样才被处理」；
  首批应缩到 `login_only`（已跑通）与点击/背包族，gm 族另行排查。

## 7. 追加（05:09–05:12 第三次窗口）：点击族跑通，`SM_HIT(14)` 不可复现、`100` 顺序差可复现

**问题与解法**：C# 的 ActorId 每会话不同（本批 2123442500，前一会话见过别的值），跨 run 复用必然点空。
驱动器新增 `click_npc: "auto:last_turn"`：取进世界后**首个非自己的 `SM_TURN(10)`** 的 recog，
没有就 fail-closed。据此新增 `tools/probe/scenarios/npc_click_auto.json`（无需 `--var`）。

```sh
# C#
python -I -B tools/probe/mir2_probe.py --login-gate 127.0.0.1:17000 --account winplay0 --password winplay0 \
  --server-name 热血传奇 --char-name winplay0 --scenario tools/probe/scenarios/npc_click_auto.json \
  --out E:/tmp/conformance/runs/w24-csharp-npc_auto --timeout-ms 20000 --finish close      # exit=0 frames=30
# Rust（同参数 + --game-gate 127.0.0.1:17600）
python -I -B tools/probe/mir2_probe.py ... --out E:/tmp/conformance/runs/w24-rust-npc_auto    # exit=0 frames=30
```

| 项 | C# | Rust |
|---|---|---|
| 帧数 | 30 | 30 |
| `SM_MERCHANTSAY(643)` | 有 | 有 |
| `SM_HIT(14)` | 无 | **无**（本批两侧都没有） |

`compare_runs.py`：`frames 30/30`、**DIFFS 2**——
`idx=26/27 ident=100 body_len 72↔55`，即 §3 第 2 条的**系统公告顺序颠倒**再次出现（内容对调、长度互换）。
其余 `whitelisted: 46 条`（recog/t_ms 掩码、52 布局）。

**重要修正**：`SM_HIT(14)` 只出现在 05:04 的 `login_only` run（Rust 单侧多一条），
本批 05:11 的 `npc_click_auto` 两侧都没有 —— 说明它是**间歇性**的（与首卡「Rust 抓到、C# 未见」的描述一致），
不能当成每次登录必现的稳定差异；要定位需固定 RNG 种子（`MIR2_RNG_SEED`）后重复采样。

## 8. 追加（05:12–05:14 第四次窗口）：`SM_HIT(14)` 采样 3+3

同一窗口内两侧各连跑 3 次 `login_only`（`--finish close`，避免 softclose 的重连残留）：

| 侧 | 帧数 | `SM_HIT(14)` |
|---|---|---|
| C# r1/r2/r3 | 29 / 29 / 29 | **无 / 无 / 无** |
| Rust r1/r2/r3 | 29 / **30** / 29 | 无 / **有（r2：seq=20, recog=119, param=8, tag=10, series=4）** / 无 |

⇒ `SM_HIT(14)` 在 Rust 侧**间歇出现（本样本 1/3）**，固定 NPC 的 ActorId=119；C# 侧 3/3 无。
这与卡 `openmir2-npc-hit-frames` 的「Rust 抓到、C# 未见」一致；要定频需固定 RNG 种子后加大样本
（`start-rust.ps1` 已带 `-Seed 42`，但会话内 RNG 消费序可能随地图/AOI 变化）。

## 9. 追加（05:14–05:17 第五次窗口）：庄家对话（`@kaiz`）两侧同样无后续 643；`SM_HIT(14)` 再 +1

- 首个非自己 actor 解出来是**赌场庄家**（643 正文：`〖庄〗… ※<开-始-下-注/@kaiz>※ ※<筹码兑现/@chouma>※`），
  于是新增 `dice_chain_auto.json`（`auto:last_turn` + `@kaiz`/`@ya5`/`@ya52` 链）。
- 两侧都在 `steps[3]`（选 `@kaiz` 后等 643）超时：**C# 30 帧 / Rust 31 帧**，行为一致——
  选「开始下注」后服务端没有第二条 643，说明该面板不是用 643 开，等待目标需改（属 `openmir2-openybdeal-host-wiring` 的窗口项）。
- `compare`：`frames 30/31`、DIFFS 3 = 两条 `100` 顺序 + `frame_count`（Rust 多一条 **`SM_HIT(14)`**，seq=31，recog=119，出现在 643 之后）。
- `SM_HIT(14)` 累计样本：Rust **3/7**（login 05:04、login-r2、dice），C# **0/7**；值恒为 `recog=119 param=8 tag=10 series=4`，
  且总是紧跟该 NPC 的交互帧，像是 NPC 转身/受击的副作用帧。

## 10. 更正（05:18–05:21 第六次窗口）：公告 100 的顺序**不是**两侧差异，两侧都会变

`npc_click_auto` 两侧各跑 3 次，按解码后的正文判顺序（「禁止…外挂」/「唯一客服QQ」）：

| run | 帧数 | 公告顺序 | `SM_HIT(14)` |
|---|---:|---|---|
| C# r1 | 32 | **禁止 → 客服** | 无 |
| C# r2 | 30 | 客服 → 禁止 | 无 |
| C# r3 | 30 | 客服 → 禁止 | 无 |
| Rust r1 | 30 | 客服 → 禁止 | 无 |
| Rust r2 | 31 | 客服 → 禁止 | **有** |
| Rust r3 | 30 | 客服 → 禁止 | 无 |

⇒ **§3 第 2 条 / §7 的「Rust 顺序颠倒」结论作废**：C# 自己 3 次里就有 1 次是「禁止→客服」，
说明这两条公告**同 tick 内次序不固定**（两侧都这样），单次 run 的次序差不能当成实现差异，
比较时应对同一 tick 内的多条 100 做**多重集比较**或显式登记为规则项。

`SM_HIT(14)` 本批 C# 0/6、Rust 1/6，与 §8/§9 合并后累计 **Rust 4/13、C# 0/13**，仍是间歇性、值恒
`recog=119 param=8 tag=10 series=4`。

## 11. 离线汇总（06:2x 无端口分析）：6 对 run 的残差只有两类

对已有的 6 对产物做离线对齐（Rust 侧剔除间歇的 `SM_HIT(14)`；同 ident 的 100 按**多重集**比）：

| 对 | ident 序列 | body 差异 | 100 多重集 |
|---|---|---|---|
| npc r1 | 差一对 `10,41`（**C# 多**） | — | 相等 |
| npc r2 / r3 | 相同 | `52` 54↔78（布局） | 相等 |
| login r1 | 相同 | `52` 54↔78 + 100 顺序（72↔55） | 相等 |
| login r2 / r3 | 相同 | `52` 54↔78 | 相等 |

⇒ 按「剔除间歇多余帧 + 100 多重集 + 52 布局对」三条口径后，**6/6 对无其它差异**。
两个"多余帧"现象**两侧都有**：Rust 偶发多一条 `SM_HIT(14)`（4/13），C# 偶发多一对 `SM_TURN(10)+SM_FEATURECHANGED(41)`（1/3，npc r1）。

**对比较器/规则表的建议**（属 `tools/conformance` 与规则车道，本卡不擅自改）：
1. `52` 需登记 (54,78) 布局对（首跑已提，仍未入表）；
2. 同一批次内的多条 `100` 改按多重集比，或显式登记「同 tick 公告次序不定」；
3. NPC 交互附近的偶发 `14` / `10+41` 需要一条显式规则（容忍或按种子复现），否则单跑必 flaky。

## 12. 更正（05:24–05:27 第七次窗口）：`SM_HIT(14)` **两侧都会发**，差异在 `series`（C#=0 / Rust=4）

用 `@chouma`（筹码兑现）触发庄家二次交互，两侧各跑一次：

| 侧 | 帧数 | `SM_HIT(14)` |
|---|---:|---|
| C# | 33 | `seq=33 recog=<庄家> param=8 tag=10 **series=0** t=12164ms` |
| Rust | 31 | `seq=31 recog=119 param=8 tag=10 **series=4** t=4139ms` |

⇒ **§8/§9/§11 的「C# 0/13 不发 14」作废**：C# 也会发，只是触发点不同（这里是选 `@chouma` 之后）。
两侧共同点：`param=8`、`tag=10`、紧跟 643 之后；**唯一稳定差异是 `series`：C#=0、Rust=4**。

另外：`@kaiz` / `@chouma` 两侧都在等待窗口内没等到下一条 643（8s），但 drain 阶段都收到了
（C# 在 `t≈12s`、Rust 在 `t≈4s`）——说明**庄家的应答慢于 3–8s**，等待窗口需要放宽或等特定帧，
这属 `openmir2-openybdeal-host-wiring` 的窗口项（已挂卡）。

**§12 补记（05:27–05:30）**：把 `@chouma` 的等待放宽到 15s 后，C# 这一跑**又没有** 643/14——选完之后只有 `10,41` 与一条 `53`（t≈18.5s）。
⇒ 庄家 `@chouma` 支的**应答本身不稳定**（一次是 643+14，一次只有 53），在弄清 oracle 该支的真实应答前，
`@kaiz/@chouma` 都不适合当判据；`dice_chain_auto.json` 暂留作探索用场景，不进首批。

## 13. 批次内出现过的 ident 词表（`mir2-rs/crates/protocol/src/messages.rs`）

| ident | 名称 | 出现位置 |
|---:|---|---|
| 10 | `SM_TURN` | 进世界/NPC 出现 |
| 14 | `SM_HIT` | NPC 交互（间歇） |
| 41 | `SM_FEATURECHANGED` | 紧随 10 |
| 42 | `SM_USERNAME` | 进世界 |
| 46 | `SM_DAYCHANGING` | 进世界 |
| 50 | `SM_LOGON` | 进世界握手 |
| 51 | `SM_NEWMAP` | 登录 |
| 52 | `SM_ABILITY` | 登录（54/78 布局） |
| 53 | `SM_HEALTHSPELLCHANGED` | 吃 `@chouma` 后那一跑 |
| 54 | `SM_MAPDESCRIPTION` | 进世界 |
| 100 | `SM_SYSMESSAGE` | 公告/GM 回包 |
| 213 | `SM_ATTACKMODE` | 进世界 |
| 520 | `SM_QUERYCHR` | 选人 |
| 525 | `SM_STARTPLAY` | 选人（带 GameGate 地址） |
| 529 | `SM_PASSOK_SELECTSERVER` | 选服 |
| 530 | `SM_SELECTSERVER_OK` | 选服（带 SelGate 地址） |
| 643 | `SM_MERCHANTSAY` | NPC 对话 |
| 654 | `SM_CHANGELIGHT` | 进世界 |
| 657 | `SM_CHARSTATUSCHANGED` | 进世界 |
| 658 | `SM_SENDNOTICE` | 登录公告 |
| 752 | `SM_SUBABILITY` | 进世界 |
| 766 | `SM_AREASTATE` | 进世界 |
| 811 | `SM_ADJUST_BONUS` | 进世界 |
| 1106 | `SM_VERSION_FAIL` | 进世界（两侧一致出现） |
| 5008 | `SM_GAMEGOLDNAME` | 登录 |
| 11029 | `SM_SERVERCONFIG` | 登录 |

> 规则表当前只覆盖 23 个 ident，上表里 `14/42/213/213/1106/5008/11029/654` 等多条仍是 UNCLASSIFIED 或靠 `--allow-unclassified` 兜底。

## 14. 背包族当前不可达（05:30–05:33 实测）

- 现有 **22 个 run 里没有任何一条 `SM_BAGITEMS(201)`**。
- 驱动器自动握手在 `SM_LOGON` 时会发一次 `CM_QUERYBAGITEMS(81)`；C# `PlayObject.Message.cs:785` 对该查询有
  **30s 节流**。为此写了 `sleep 31s → CM_QUERYBAGITEMS(81) → 等 201` 的临时场景再测：
  C# 侧 38 帧、**仍无 201**（多出来的帧是 `46 SM_DAYCHANGING` 等）。
- ⇒ `bag_equip` / `eat_item` / `pickup_item` 三个场景**暂时无法产出证据**；需要先确认 oracle 里背包查询的
  真实回包号与前置条件（是否必须 NPC 仓库会话 / 是否走 `SM_SENDUSEITEMS(621)` 路径）。
- 附带观察：本跑 C# 也出现了 `SM_HIT(14)`，与 §12 的「两侧都会发」一致。

## 15. 本批证据 → A 卡对照（供验收判定）

| A 卡 | 本批证据 | 结论 |
|---|---|---|
| `openmir2-login-654-order` | 两侧 `654` 各 1 条、位次一致（21/22 对齐） | **有证据** |
| `openmir2-login-charstatus-657` | 两侧 `657` 各 2 条、邻居一致 | **有证据** |
| `openmir2-login-frame-order-52-752` | `752` 各 1 条位次一致；`52` body 54 vs 78 | 帧序有证据；`52` 布局待规则 |
| `openmir2-login-numeric-gaps` | 除 `52`/同 tick `100` 外逐字节相同（6 对 run） | **有证据** |
| `openmir2-npc-def-loading` | 点庄家两侧各得 `643`，正文逐字相同 | **有证据** |
| `openmir2-npcs-fixed-loading` | NPC `10+41` 出现帧两侧均有（C# 偶发多一对） | **有证据（含偶发差）** |
| `openmir2-npc-hit-frames` | `SM_HIT(14)` 两侧都发，**series C#=0 / Rust=4** | **有证据（差异已定位到字段）** |
| `openmir2-openybdeal-host-wiring` | `@kaiz`/`@chouma` 应答迟到且不稳定 | 受阻（需 oracle 应答口径） |
| `openmir2-script-take-persist` / `persist-useitems-wiring` / `click-path-bag-writeback` | 22+ run 无 `SM_BAGITEMS(201)` | 受阻（需 oracle 回包口径） |
| `openmir2-gm-commands-design` | `CM_SAY` 两侧均无回包 | 受阻（需确认回包号/前置） |

前 7 张卡的证据已可直接引用；后三组需要 Leader 回口径（`5511ea0d` gm、`25220260` bag、`2840f903` dice）。

## 16. 庄家菜单选择：四种组合都不回第二条 643（05:36–05:45）

部署脚本本身（`Market_Def/赌博NPC/赌场-B013.txt`，两侧 root 都有）里 `[@kaiz]` **没有 `#if`/`#act`，是直接 say**，
所以正常情况下选完应立刻收到第二条 `SM_MERCHANTSAY(643)`。实测：

| 组合 | 等待 | 结果 |
|---|---:|---|
| `@kaiz` | 3s / 30s | 无 643 |
| `kaiz`（去 `@`） | 30s | 无 643 |
| `@chouma` | 8s / 15s | 无 643（仅一次 8s 后 drain 拿到过一条，且伴随 14） |
| Rust `@kaiz` | 3s | 无 643 |

多次跑后面都跟着 `10`/`41`（NPC 转身）却始终没有第二条 643 ⇒ **不是等待时长问题，是选择没有被脚本引擎接受**。
可能方向（需 oracle 侧确认）：① 客户端应发的 label 是否要带 `@`（两种都试过）② NPC 会话是否受 `Npc_Def` 装载影响
（见卡 `openmir2-npc-def-loading`）③ EDCode 编码后的 label 在服务端解码后是否与脚本标签完全一致。

结论：庄家族（`openmir2-openybdeal-host-wiring`）在弄清 label 语义前**无法产出证据**。

## 17. **根因找到**：驱动器对 c2s body 做了两次 EDCode（05:45–05:52）

`plan_step` 里 `send.text` / `say` / `dlg_select` 的 body 先被 `encode_text(...)`（= GBK+EDCode）编码，
随后 `build_c2s_frame` 又把 **12B 头 + body 整体** 再 EDCode 一次 ⇒ 线上是**双重编码**；
服务端 `EDCode.DeCodeString(msgBuff[12..^1])` 只解一次，于是 label/说词变成乱码 ⇒ 静默忽略。

这解释了此前所有「无回包」现象：
- `CM_CLICKNPC(1010)`（空 body）→ **正常**，643 弹窗一直能拿到；
- `CM_SAY(3030)` / `CM_MERCHANTDLGSELECT(1011)`（带 body）→ **全部无回应**（GM、`@kaiz`/`@chouma`）；
- 因此 §6（gm 无回包）、§16（label 未被接受）**结论作废，是驱动器 bug，不是 server 行为差异**。

**修复**（commit 见下）：body 一律回到**原始 GBK 字节**，EDCode 只在 `build_c2s_frame` 做一次；
selftest 期望同步改为 `.encode("gbk")`，8 项仍全绿。

**修复后实测（C# 侧）**：
- `gm_time`：exit=0，收到 `SM_SYSMESSAGE(100) 当前日期时间: 2026-10-23 05:49:33` ✅
- `dice_chain_auto`：从原来的 **steps[3] 超时**推进到 **steps[7]**——`@kaiz→643`、`@ya5→643` 都通了；
  最后一步 `@ya52` 无 643，是因为脚本该支 `CHECKitem 筹码包 1` 不满足 ⇒ 走 `MESSAGEBOX 请打开商铺兑换[筹码]`，
  **属内容前置，不是 bug**。
