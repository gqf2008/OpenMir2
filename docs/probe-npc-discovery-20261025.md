# probe NPC 寻址（name: / pos:）— 2026-10-25

- 卡：`openmir2-probe-npc-discovery`（tier **L**，owner `openmir2-dev-2`，branch `card/probe-npc-discovery`，base `a60e04ad9`）。
- 定位：`click_npc` 原先只接受 **actor id** 或 `auto:last_turn`（「最近一个非己 `SM_TURN`」），
  于是 `run3 #24 castle_merchant_tax_upgrade` 与 W-CO-*（castle-official）这类**知道 NPC 是谁、却点不到**的场景卡在门口。
  本卡给 DSL 补两种**按可见对象**寻址：`name:<NPC 名>` 与 `pos:<x>,<y>`，并且**歧义一律 fail-closed**。
- 写集：只动 `tools/probe/mir2_probe.py` + 本文件。**不改** `run3-scenarios.json`（那是另一张卡）、
  `tools/conformance/card_window.py`、`tools/window/run_batch.sh`。

## 1. `click_npc` 的四种写法

| 写法 | 含义 | 命中要求 | 备注 |
|---|---|---|---|
| `click_npc: 12345` 或 params 名 | 直接给 actor id（既有行为） | 不校验 | 与 oracle 的 `CM_CLICKNPC.recog = ActorId` 同口径 |
| `click_npc: auto:last_turn` | 最近一个非己 `SM_TURN(10)` 的 `recog` | 无兜底 | 既有行为；**只在没有更精确写法时用** |
| `click_npc: name:沙巴克城堡管理` | 按**名尾 `ShowName`** 精确匹配可见对象 | **恰好 1 个** | 名尾口径见 §2 |
| `click_npc: pos:7,16` | 按**坐标**匹配**当前可见的固定/静态对象**（启发式，见 §2.5） | **恰好 1 个静态对象** | 空格/多对象/活动对象都算不中 ⇒ 红；逃生阀 `pos:7,16?allow_any=1`（默认关） |

- 三种新写法都**仍发 `CM_CLICKNPC`**，`recog` 取该对象 actor id，`param/tag/series/body` 全 0 —— **不新造协议**。
- 未知写法（如 `auto:lastturn`、`foo:bar`）在**加载期**就 `ScenarioError`（退出码 2），不会拖到运行期：
  错误串写明只认 `auto:last_turn` / `name:<名>` / `pos:<x>,<y>`。
- `name:` 空名、`pos:` 非「十进制 x,y」或坐标越界（>65535）同样加载期拒绝；`?allow_any` 只认 `0`/`1`。

## 2. 名字从哪来（oracle 口径，不是猜）

`SM_TURN(10)` 的帧体 = `Encode(8B CharDesc)` [ + `Encode("ShowName/Color")` ]，两段**各自** EDCode：

- oracle：`src/M2Server/Player/PlayObject.Message.cs:1549-1555` —— `Msg` 非空时追加 `EncodeString($"{Msg}/{nObjCount}")`；
  出现帧路径（`PlayObject.cs:3240` 的 `SendMsg(..., baseObject.GetShowName())`）**带非空 Msg**；
- Rust 同构：`mir2-rs/bins/game-svr/src/downlink.rs::turn_pair`（`Encode(CharDesc)` + `Encode("ShowName/Color")`）；
- **`CM_TURN` 下行路径无名尾**（`turn_pair_nameless`，`PlayObject.cs:1922` 的 `sMsg` 恒空）⇒ 该对象的 `name=None`，
  只能按 `pos:` / actor id 寻址。probe 的实现据此只解析「体长 > 11 字符」的帧（`SM_TURN_DESC_CHARS = 11`
  是 `Encode(8B)` 的可逆长度）。

实测样例（`E:/tmp/window/run2/cs-probe` 的一条出现帧）：帧体解码后名尾 = `〖庄〗/255` ⇒ `name=〖庄〗`。

## 2.5 `pos:` 的对象类别门（启发式，**不是协议级类型判定**）

`pos:` 只接受**固定/静态对象**，命中之前判定、不满足即 fail-closed。三条同时满足才算静态：

| # | 判据 | 依据 |
|---:|---|---|
| 1 | 该 actor **没有任何移动帧**（`SM_WALK/SM_RUN/SM_HORSERUN/SM_BACKSTEP/SM_RUSH/SM_RUSHKUNG`） | 移动帧族 = 对象动过的直接证据（值见 `crates/protocol/src/messages.rs`） |
| 2 | 坐标从未变过（全部 `SM_TURN` 的 `Param/Tag` 同一格） | 同帧字段 |
| 3 | **首见于玩家登录后的第一份可见快照内**（`first_seq ≤ snapshot_end_seq`） | 固定 NPC 随地图加载下发；怪由 MonGen 后来刷出。`snapshot_end_seq` = **第一个场景步执行前**的帧序号（进世界 + dwell 之后） |

**为什么只能做启发式**：`SM_TURN` 体只有 `Encode(8B CharDesc)` + 可选 `Encode("ShowName/Color")`
（`src/M2Server/Player/PlayObject.Message.cs:1549-1555`；Rust `mir2-rs/bins/game-svr/src/downlink.rs::turn_pair`），
**协议里没有对象类别字段**；probe 的可见集也只认 `SM_TURN`。⇒ 本判据是「按可观测行为推断静态」，
不能当权威类型系统用；写场景的人仍需保证目标格上只有那一个固定 NPC。

- **当前可见**语义：`SM_DISAPPEAR(30)` 会把对象移出候选集（再出现则重新加入）——此前只增不减，等于「曾出现」。
- **分类证据进日志**：命中/未命中的错误串与 `steps[]` 记录都带
  `name/(x,y)/actor/首见 seq/移动帧数/类别=静态|活动（原因）`；无名对象另附**原始名尾**。
- **逃生阀**：`pos:<x>,<y>?allow_any=1`（**默认关闭**）。只在确知该格上就是目标、且它被误判为活动对象时才用；
  它会关掉类别门、仍保留「恰好 1 个」的歧义门。降级理由写进场景 `note`，不得当默认玩法。
- **名字含 `/`（N3）**：名尾按**最后一个** `/` 切，且最后一段必须是十进制颜色（`ShowName/Color`）；
  末段不是颜色 ⇒ `name=None`（**不截断成前缀名**），错误串里显示 `<无名>` + 原始名尾。所以 `name:GM/测试` 能选中
  名尾 `GM/测试/255` 的对象；而名尾 `A/B` 不会被当成名字 `A`。

## 3. 歧义三态（fail-closed，绝不「挑第一个」）

| 命中数 | 结果 | 错误串里给什么 |
|---:|---|---|
| 1 | 正常发帧，`recog` = 该对象 actor id | 运行记录多一条 `kind=click_npc`（`selector`/`actor`/`name`/`x`/`y`） |
| 0 | `StepFailure`（rc=1） | `name:` ⇒ 列出**全部**可见对象；`pos:` ⇒ 列出该格 **±3** 邻近可见对象（若邻域没有，退回全量） |
| ≥2 | `StepFailure`（rc=1） | 「命中 N 个对象（歧义，fail-closed，不挑第一个）」+ 每个候选的 `name/(x,y)/actor` |
| 该格只有**活动对象**（`pos:`） | `StepFailure`（rc=1），**命中之前**判定 | 「都**不是固定/静态形态**」+ 每个候选的 `类别=活动（原因）`；提示 `?allow_any=1` |

候选格式统一为 `name=<名或 <无名>> (x,y) actor=<id>`，操作者可直接把 `actor=<id>` 抄回 `click_npc` 重跑。
**没有任何一步会回落到 `auto:last_turn`**：名字不存在就是不存在（selftest 牙 c 钉住这一点）。

## 4. 与 `param_discovery.py` / `--discovery-scenario` 的关系

- `tools/probe/param_discovery.py` 的 `npc_actor` 规则**在点击之后**推导：优先取 `SM_MERCHANTSAY(643).recog`，
  否则退化为「首个非己 `SM_TURN`」（**规则**，不是帧语义，不计入发现能力）。
  它解决的是「我点到的到底是谁」；`name:`/`pos:` 解决的是「我点的是谁」——**先定向，再推导**。
  用 `name:沙巴克城堡管理` 定向后，`643.recog` 才能作为**该 NPC** 的已证 actor id（`derived`），
  而不是「某个应答者」。
- 设计文档 `docs/param-discovery-design-20261023.md:117` 提到的 `--discovery-scenario` 是**尚未实现**的设想
  （仓库里 grep 不到该选项）。本卡的 `name:`/`pos:` **不依赖**它：定向能力直接落在 `click_npc` 上，
  现有 discovery 场景（`npc_click_auto`）只要把 `click_npc: auto:last_turn` 改成 `click_npc: name:<名>`
  即可，无需新的 discovery 机制。它也不是 `--discovery-scenario` 的替代品：后者（若做）是多场景编排，
  本卡只给单步寻址。

## 5. 在 #24 与 W-CO-* 上怎么用

- **#24 `castle_merchant_tax_upgrade`**：部署里城堡商人是固定 NPC（`Npcs.txt` 的 `0150 沙巴克城堡管理`；
  `MerChant.txt:83` 的 `0151 武器商行` 是升级腿的买卖人）。场景里把
  `click_npc: auto:last_turn` 换成 `click_npc: name:沙巴克城堡管理`（或该 NPC 的实际坐标 `pos:<x>,<y>`），
  就能**确定**点到的是城堡管理而不是旁边恰好最近的那个。注意：**本卡不改 run3 的 #24 `blocked` 文案**，
  解封是另一张卡（场景 JSON + 清单行两处同步）。
- **W-CO-*（castle-official）**：`@main` 说词逐字节（mask `$DATETIME`）要求先点中 `沙巴克城堡管理`；
  用 `name:` 定向后，`643` 的应答者身份不再靠「最近」假设，W-CO-1 的前置从**规则**变**已证**。
  动作段（W-CO-6）仍受「测试专用城主注入」限制，与本卡无关。

示例片段（`<-` 为新增写法）：

```json
{
  "name": "castle_merchant_tax_upgrade",
  "steps": [
    { "click_npc": "name:沙巴克城堡管理" },
    { "wait_for": 643, "timeout_ms": 3000 },
    { "dlg_select": "@buy" },
    { "wait_for": 645, "timeout_ms": 3000 }
  ]
}
```

## 6. 尚未覆盖（诚实清单）

| 缺口 | 现状 | 影响 |
|---|---|---|
| 无名尾对象（`CM_TURN` 路径的 `SM_TURN`） | `name=None`，`name:` 匹配不到 | 该类对象只能用 `pos:`/actor id |
| 还没收到 `SM_TURN` | 红（「还没看到任何非己对象…先 wait_for」），不猜 | 场景需在 `click_npc` 前等到出现帧（进图 AOI 内通常已有） |
| 同一 actor 的多条出现帧 | 按 actor id **去重**（保留最后坐标/名字） | 不会把「同一个人转身两次」误判成歧义 |
| 对象类别无协议字段 | `pos:` 用**启发式**静态判定（无移动帧 + 坐标不变 + 首见于初始快照）；`name:` 仍不区分类别 | 一只能说不动、且首见于初始快照活动的对象理论上可骗过 `pos:`；要绝对限定由调用方保证/用 `name:` |
| 坐标语义 | `pos:` 只匹配当前可见对象的 `Param/Tag`，不做寻路/相邻/朝向校验 | 点到之后是否够得着由 oracle 的点击门决定（超时响亮失败） |
| `pos:` 与 params | 只收字面十进制坐标，不收 `--var` 名 | 需要动态坐标时先用 `send` 或扩展（未做，避免半吊子语义） |
| 真机 | **未做真机**（本轮只做到构造级 selftest；宿主内存紧张、私栈在 tester 手里） | 需要在窗口轮次里抽样冒烟一次（见卡面 ④） |

## 7. 验证

```sh
python -I -B tools/probe/mir2_probe.py --selftest
# => mir2_probe selftest: OK (18 checks; ...)   dsl-npc-discovery 内含下列全部用例
```

`dsl-npc-discovery` 覆盖（全部构造级、不连网）：

- schema：2 正 6 反（未知前缀 / 空名 / 坏坐标 / 越界 / `auto:lastturn` 等）；
- 寻址三态：同名两处去重后仍歧义、零命中列全部可见对象、`pos:` 空格给 ±3 邻近；
- **N2**：格上有活动对象（有移动帧）⇒ 红且列 `类别=活动（有移动帧 N 条）`、提示 `allow_any`；
  `pos:8,16?allow_any=1` ⇒ 放行并选中该 actor；对象首见于**初始快照之后** ⇒ 红（`晚于初始快照界`）；
- **N1**：自身 actor 在目标格/同名时，`name:` 与 `pos:` 各一例都**不把自己当候选**（去掉自身过滤即红，见变异）；
- **N3**：名尾 `GM/测试/255` ⇒ `name:GM/测试` 命中；名尾无颜色段（`A/B`）⇒ 不截断、列 `<无名>` + 原始名尾；
- **DISAPPEAR**：对象出现后收到 `SM_DISAPPEAR(30)` ⇒ 不再是候选。

变异（每条都从 HEAD 还原后重跑确认 `OK (18 checks)`）：

| 变异 | 咬哪颗牙 |
|---|---|
| 去掉自身过滤（`self_actor` 一行） | N1：按名/按坐标把自己当候选 |
| `pos:` 的静态门整条关掉 | N2：活动对象被直接命中 |
| `DISAPPEAR` 不剔除（`disappeared` 恒 False） | 已消失对象仍可被 `name:` 选中 |
| 名尾颜色不校验（退回按 `/` 直接切） | N3：无名尾被截断成前缀名 `A` |
| `allow_any` 默认开 | N2：活动对象无需逃生阀即被选中 |
| 去重键从 actor id 改成名字（reviewer 的变异） | 同名两处被合并成一个 ⇒ 歧义门失效 |
| 多命中挑第一个（`if len(hits)==1` → `if hits`） | 同名两处未被拒 |
| 零命中不再早报 | 邻近对象不再被列出 |
| 运行期不解析选择器 | 发出 `recog=None` 的帧（FrameError） |
| 名字匹配恒真 | 不存在的名字未被拒 |

## 8. 残留物清单

- 入库：`tools/probe/mir2_probe.py`（+ 本文件）。
- 未入库（一次性补丁/变异/证据脚本）：`E:/tmp/npc_a.py`、`E:/tmp/npc_b.py`、`E:/tmp/npc_c1.py`、`E:/tmp/mutate_npc.py`、
  `E:/tmp/f1_a.py`、`E:/tmp/f1_b.py`、`E:/tmp/f1_c.py`、`E:/tmp/f1_d.py`、`E:/tmp/mut_follow.py`、`E:/tmp/follow_doc*.py`。
- 未触碰：`run3-scenarios.json`、`card_window.py`、`run_batch.sh`、`src/**`、mysqld；
  未起服务端、未占端口、未申请 `stack-claim`。
