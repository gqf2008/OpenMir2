# 分散地图测量设计（D1/D2/D3，500 档）

> 卡：`openmir2-distributed-map-design`（tier=L）。状态：**设计件**（needs-review）。
> **本文件只做设计：不跑窗口、不改产品代码、不改 `Envir` 内容。** 文中所有实测数字都引用既有归档，
> 推算处显式标「假设」。
>
> 背景：现有全部 500 档数据都是 **500 bot 同图**（`docs/capacity-500-conclusion-20261116.md` §6
> 明写「分散地图 / 不同密度未测」）；容量模型对客户端轴与 AOI 的外推都建立在这个最坏形态上
> （`docs/capacity-model-20261114.md`「同图口径警告」：10k 同图 O(N²) 是最坏上界，分散场景未测）。

## 0. 先纠正两个会毁掉对照的事实（口径前置）

1. **「500 同图」实际是 map 0 上的两个簇，不是一个人群。**
   `E:/MirServer/M2GameSvr/Envir/StartPoint.txt` 有 8 条，`StartPointSize=2`，前两条都是 map 0
   （289,618）与（650,631）；服务端启动日志确认 `StartPointSize=2`。
   500 档归档 `E:/UserFleet/load-360-recvnull-verify-20261116/stack-logs/game-svr.out.log` 的
   500 条 `进世界 ChrName:... CurMap:...` 里 **499 条 CurMap=0、1 条 CurMap=0136**；map 0 的
   **分簇实测为 276/223**，两簇**落点中位数**为 288,617 / 649,630（按 276/223 分簇统计；单条日志里的
   293,617 / 650,647 只是个体样本，`289,618 / 650,631` 是 StartPoint 坐标/取整，不是簇中位数）。
   ⇒ **D3 对照必须是「map 0 × 两簇」**，不能是「500 一簇」；
   主对照取**对称化 250/250**（去掉簇大小协变量），并在报告里同时列实测 276/223。
2. **多图不增加地图装载成本。** 服务端启动即加载全部注册图：
   `Envir 真地图注册表：322 张`、`地图格子属性已灌入 ... maps=322 cells=11734654`
   （同归档 game-svr 日志）。322 个 section → 314 个不同 `.map` 文件（`mir2-rs/bins/game-svr/src/mapwire.rs`
   模块头登记的静态普查）；其余 258 张 `.map` 未注册、不进世界。⇒ 分散到已注册图不需要额外的地图加载/预热。

## 1. 测量分布档位

三档，按「**每图/每簇的 bot 密度**」而不是按「图名」分层。理由是 AOI 成本主要随
**同簇玩家数**和**全局实体数**变化，而不是随地图张数变化；三档形成 2 → 5 → 8 个簇的梯度，
可把「分散的收益」从「单轮方差」里分离出来。

| 档 | 名称 | 簇数 | 图数 | 每簇 bot | 两两可见对指数 P | 相对 D3 | 覆盖 AOI 扇出的哪一段 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| **D3** | 极端集中（对照；对称化 250/250） | 2 | 1 | 250 / 250（实测 276/223） | 62,250 / 62,703 | 1.00 | 同簇密度最高段；peer AOI 与广播收件人最多 |
| **D2** | 近似运营（头部集中） | 5 | 4 | 25–150 | 30,375 | 0.49 | 热门图仍承载 30% 的单簇；覆盖「一张热图 + 尾图」的运营形态 |
| **D1** | 均匀分散 | 8 | 6 | 62–63 | 15,376 | 0.25 | 同簇密度最低段；peer AOI 接近消失，但全局实体扫描仍在 |

- `P = Σ n_i(n_i−1)/2`（n_i = 第 i 簇 bot 数），是「peer AOI 规模」的一阶代理。
  它只用于**预登记预期方向**，不是判据本身。
- 三档都固定 **总 bot = 500**、同一棵树、同一 BotSrv 配置、同一 `HoldSec`；
  唯一变量是「簇数 × 每簇人数 × 地图」。

### 1.1 为什么不是 1 档 / 4 档 / 10 图

- **必须保留 D3（同图）**：没有同树同口径的同图对照，任何「分散更好」的差值都不可归因
  （既有同图数据在别的树上，见 §4.8）。
- **D3 是「对称化控制」，不是「复刻现状」**：现状 map 0 分簇实测 276/223（另 1 个在 0136）。
  对称化 250/250 的 P=62,250，实测 276/223 的 P=62,703（差 <1%）。取对称化的理由：
  去掉簇大小协变量，且 0136 那 1 个是夹具残留、不是设计变量；报告仍须并列实测分簇数。
- **D2 是「假设」，不是「真值」**：项目拿不到真实玩家分布（无生产遥测；拿生产数据还涉及敏感信息）。
  D2 的权重是**声明式假设**，必须在报告标题里带 `assumed-weights`，不得写成「真实分布」。
  替代客观口径（MonGen 权重）见 §6.2，只作灵敏度，不作主档。
- **不上 10 图**：只有 6 张图有 `StartPoint.txt` 的合法出生点；再扩图需要新的坐标搜索夹具
  （设计见 §5.3），多一层坐标校验风险，第一轮不建议。D1 的 62–63/簇已经把 peer AOI 压到 D3 的 25% 量级，
  足够回答「分散是否降低 AOI 量」。

## 2. 每档的 bot 分配与地图数

### 2.1 候选地图（全部来自部署 `Envir/MapInfo.txt`，只读核对）

| 图名 | 描述 | 尺寸 | Flag | MonGen 计划数 | 备注 |
| --- | --- | --- | --- | --- | --- |
| 0 | 比奇省 | 700×700 | DAY | 1,733 | 现状基线图 |
| 2 | 毒蛇山谷 | 600×600 | DAY | 1,010 | |
| 3 | 盟重省 | 1000×800 | DAY | 225 | 面积最大 |
| 4 | 封魔谷 | 500×500 | DAY | 473 | MonGen 高度集中在出生点附近 |
| 5 | 苍月岛 | 800×800 | DAY | 350 | 出生点附近无 MonGen |
| 11 | 白日门 | 500×500 | DAY | 37 | 出生点附近无 MonGen |

- 六张图均无 `FIGHT` / `NORECONNECT` / `NORECALL`，都是 `DAY`；不会因地图 flag 改变昼夜或战斗规则。
- 六图 MonGen 合计 **3,828 / 39,019（9.8%）**；环境里其余怪仍在别的图，**世界总实体数不会因分散而减少**。

### 2.2 出生点与坐标（唯一事实源：`Envir/StartPoint.txt`）

| 图 | 坐标 | 校验结果（本次只读核对） |
| --- | --- | --- |
| 0 | 289,618 | 中心 WALK；5×5 全 WALK（25/25） |
| 0 | 650,631 | 中心 WALK；5×5 全 WALK（25/25） |
| 0 | 330,266 | 中心 WALK；5×5 全 WALK（25/25） |
| 2 | 503,483 | 中心 WALK；5×5 全 WALK（25/25） |
| 3 | 330,330 | 中心 WALK；5×5 全 WALK（25/25） |
| 4 | 240,200 | 中心 WALK；5×5 全 WALK（25/25） |
| 5 | 141,337 | 中心 WALK；5×5 全 WALK（25/25） |
| 11 | 178,326 | 中心 WALK；5×5 全 WALK（25/25） |

校验方法：按 `mir2-rs/crates/envir/src/mapfile.rs` 模块头登记的格式（52 字节头、12 字节/格、
`k = x*Height + y`、`wFrImg` bit15 ⇒ LowWall、`wBkImg` bit15 ⇒ HighWall）**裸读只读部署 `.map`**。
⇒ oracle 的出生抖动是 `GetRandHomeX/Y = Home - 2 + Random(3)`，即 x/y 各取 **-2/-1/0 的 3×3**；
这些点的 5×5 邻域全 WALK，所以 3×3 抖动必然落在可走格，
**不会出现「进图落墙格 ⇒ 保留角色但不入格表」的伪在world**。

**每簇的本地 MonGen 协变量**（计划刷怪数，按出生点 ±50 / ±100 格内统计）：

| 簇 | 图 | 半径 50 内 | 半径 100 内 |
| --- | --- | --- | --- |
| 0a | 0 | 130 | 390 |
| 0b | 0 | 0 | 130 |
| 0c | 0 | 21 | 83 |
| 2 | 2 | 2 | 6 |
| 3 | 3 | 20 | 40 |
| 4 | 4 | 472 | 472 |
| 5 | 5 | 0 | 0 |
| 11 | 11 | 0 | 0 |

这张表**必须随报告一起出**：D1 的 8 簇不是等负荷实验，map 4 簇是重猎场，5/11 簇近乎静默。
不记录它，就会把「簇间负荷差异」误读成「分散效应」。

### 2.3 三档的逐簇分配（可直接照此建夹具）

**D3（对称化对照；现状实测 276/223 + 1 个 map0136）**

| 簇 | 图 | 坐标 | bot 数 |
| --- | --- | --- | --- |
| 0a | 0 | 289,618 | 250 |
| 0b | 0 | 650,631 | 250 |
| 合计 | 1 图 | | 500 |

- 250/250 是对称化控制；按实测算 P=62,703（276/223），与对称化的 62,250 差 <1%。
  报告必须并列两个分簇数。实测那 1 个 `0136` 的来源未核（推测为旧夹具角色）；本轮夹具把
  500 个 `loadbot*` 账号全部 pin 到 map 0，所以 M2 对 D3 的期望是 map 0 = 500。

**D2（近似运营；权重是假设）**

| 簇 | 图 | 坐标 | bot 数 | 占名义 |
| --- | --- | --- | --- | --- |
| 0a | 0 | 289,618 | 150 | 30% |
| 0b | 0 | 650,631 | 100 | 20% |
| 3 | 3 | 330,330 | 150 | 30% |
| 2 | 2 | 503,483 | 75 | 15% |
| 4 | 4 | 240,200 | 25 | 5% |
| 合计 | 4 图 | | 500 | 100% |

假设依据（必须原文引用）：比奇省（0）与盟重省（3）是两座主城/主枢纽，占 80%；
毒蛇山谷（2）与封魔谷（4）各给一条狩猎尾。**没有真实分布数据支撑**；这是可检验的
「头部集中」假设，不是观测值。

**D1（均匀分散，按出生点而不是按图）**

| 簇 | 图 | 坐标 | bot 数 |
| --- | --- | --- | --- |
| 0a | 0 | 289,618 | 63 |
| 0b | 0 | 650,631 | 63 |
| 0c | 0 | 330,266 | 62 |
| 2 | 2 | 503,483 | 63 |
| 3 | 3 | 330,330 | 63 |
| 4 | 4 | 240,200 | 62 |
| 5 | 5 | 141,337 | 62 |
| 11 | 11 | 178,326 | 62 |
| 合计 | 6 图 | | 500 |

- 每簇 bot 数取 62/63 是因为 500 = 8×62 + 4；**均匀的是簇，不是图**（map 0 有 3 个合法出生点，
  所以占 188 人）。
- 每簇内部做 **3×3 抖动**，与 oracle 出生抖动同范围（x/y 各 -2/-1/0）。确定性映射：
  `dx = -2 + (i mod 3)`，`dy = -2 + ((i div 3) mod 3)`（i = 该簇内账号序号）。这样三档之间
  除分配外没有别的差异；5×5 全 WALK 已核，3×3 子集必然可走。

### 2.4 AOI 预期方向（推算，未实测）

四份同图归档的**稳态** AVI（按 §3.4/§4.3 的 `t1_avi = 最后 gate summary − 15s` 重算）为
**recvnull 417.2 / A 535.0 / B 527.0 / profiled 508.9 帧/客户端/s**（区间 417–535，同树 A/B 差 ≈1.5%）；
整窗口径（含爬坡，仅作量级）为 349.49 / 492.09 / 486.63 / 445.37。本设计**只用稳态口径**做判据。

若广播量由「同簇 peer 收件人」主导，按 P 的比值，D1 ≈ D3 的 25%、D2 ≈ 49%。
但这不是承诺：怪物走位扇出里 **`hear_targets_at` 的实体扫描是全局的**（§6.1 R2），
收件人变少只减少编码/发送，不减少扫描。**该方向必须由 AVI 判据实测确认**（§3.4）。

## 3. 判据（全部可机检）

### 3.1 硬门禁（每档，沿用现有 `load_gate` / `load_report` 口径）

| # | 判据 | 来源/字段 | 通过线 |
| --- | --- | --- | --- |
| G1 | 对外 verdict | `tier-500/report.json.verdict` | `GREEN`（rc=0） |
| G2 | 登录成功率 | `login_success_pct` | **100%**（tier<1000 的现有默认；卡面建议 95%，但 95% 只作「数据可用下限」，不改变 verdict） |
| G3 | 无应答 | `login_conn_no_resp` | 0 |
| G4 | 内部错误 | `internal_errors` | 0（白名单预算 0；缺字段 fail-closed） |
| G5 | 错误普查 | `error_census` 20 类 | 每类 0；`internal_errors == Σ(feeds_internal)` |
| G6 | OOM / BotSrv 崩溃 | `oom_hits`、`botsrv_self_exited` | 0 / false |
| G7 | 稳态掉线 | `conn_lost_steady` | 0 |
| G8 | 窗口有效 | `window.hold_sec`、`conclusion_valid` | ≥360s / true |

### 3.2 分布不变量（新增，直接决定这一轮是不是「分散」）

| # | 判据 | 机检来源 | 通过线 |
| --- | --- | --- | --- |
| M1 | 进入世界的 bot 数 | `stack-logs/game-svr.out.log` 的 `进世界 ChrName:... CurMap:...`（按 ChrName 去重、取最后一次） | 500 个不同 ChrName |
| M2 | 每图实际人数 | 同上，按 `CurMap` 分组 | 与 §2.3 计划**逐图相等** |
| M3 | 每簇人数 | 实际 `(CurMap, 落点±3)` 分簇 | 与计划逐簇相等（±1 允许抖动越界） |
| M4 | 结束时仍在该图 | **优雅停服且存档收尾成功**后的 `characters` 快照，按 `LoginID` 分组 | 与 M2 相等；`STOPPED game-svr ... force`、快照缺失、或日志无 `关服存档收尾：… 超时 false` 且 `应答==发起` ⇒ PREREQ |

- M1/M2 来自**服务端自己**的进图日志，不依赖 BotSrv 自报；M4 来自 DB 存档（GameSvr 关服写回）。
- M4 的生产者必须存在：`Stop-Process -Force` 不存档（§5.3），所以每档收尾必须走 game-svr 的
  `openmir2-gamesvr-shutdown-<pid>` 管道并确认 `STOPPED game-svr ... graceful`，随后在 MySQL
  仍可用时（若已停则单起一次）做跑后快照。夹具的「写前/写后」快照是**写库前后**，不是跑后，
  不能替代 M4。
- **只看到 `graceful` 不够**：`shutdown_save_all` 超时也会正常退出并打印 `graceful`。
  必须再断言 game-svr 日志的 `关服存档收尾：在册 N / 发起 N / 应答 N / 超时 false / 耗时 …ms`
  中 **`超时` 为 false/0 且 `应答==发起`**；该行为假、缺行或 `应答<发起` ⇒ M4 PREREQ
  （`snapshot-post` 同判）。
- **假设**：每个 `loadbotN` 账号恰好 1 个未删除角色（本次未连库验证；夹具必须断言）。
  M4 以**账号**为计数单位；一个账号多角色时按账号整组写同图并只计一次。
- 任一不满足 ⇒ **该档判 `DIST_RED`**，即使 `report.json` 是 GREEN 也不得出「分散场景达标」。
- 现有日志格式已包含 `CurMap`（500 档归档可见），**不需要改产品代码**。

### 3.3 容量/延迟判据

| # | 判据 | 定义 | 通过线 |
| --- | --- | --- | --- |
| C1 | 客户端轴（50% 预算） | `HostInbound / 500` | ≤ **0.20 ms/客户端/tick**（= 100ms / 500，容量模型 §4.6 的 50% 预算线） |
| C2 | 入站总量 | TICKPROF `HostInbound` | ≤ 100 ms/tick |
| C3 | 工作段 | `HostTickTotal − HostSleep` | ≤ 100 ms/tick（50% 预算） |
| C4 | tick P99（客户端仪器） | `report.json.tick_p99_ms` | ≤ 100 ms |
| C5 | tick P99（服务端仪器） | TICKPROF 直方图 P99 | ≤ 100 ms |
| C6 | 超长 tick | `ge201_samples` | 0（A 轮基线也是 0；出现 1 个即标注并复跑） |
| C7 | 非劣于同图 | 各档 vs **同树 D3** | `per_client_inbound ≤ 1.10 × D3` 且 `P99 ≤ 100` |

C1/C7 是「分散场景达标」的主判据；C2/C3 是「总量没有恶化」的兜底。
**不要**把 C1 读成「玩家上限」：BotSrv 是恒压动作探针，真实玩家动作频率未测（容量模型 §4.6 的口径警告）。

### 3.4 AOI 经济性（预登记，用来区分「真收益」与「噪声」）

定义 **AVI**（AOI 广播尝试量，帧/客户端/秒），**只在稳态窗内计**：

```text
att(t) = drop_noncritical_frames(t) + noncritical_sent_frames(t)   # gate 累计计数
AVI    = [att(t1) − att(t0)] / 500 / (t1 − t0)
```

其中 t0/t1 是 §4.3 的稳态窗边界，`att(t0)/att(t1)` 取最接近该边界的
`event=gate_backpressure_summary` 采样（约 5s 一条）。**禁止**用
`att(最后) / 500 / window_sec`：`window_sec`/`uptime_ms` 含爬坡，会与 §4.3「禁止全窗均值」
直接矛盾。归档与报告必须同时列整窗值与稳态值，并注明两者不同。

**四份同图归档的背景值**（按 §4.3 的 `t1_avi` 定义重算；第二轮复审 `581d6977` 已更正边界错配）：

| 归档 | 整窗（含爬坡） | 稳态 |
| --- | --- | --- |
| `load-360-recvnull-verify-20261116`（判据轮） | 349.49 | **417.2** |
| `load-360-slowsep-A-20261116` | 492.09 | **535.0** |
| `load-360-slowsep-B-20261116` | 486.63 | **527.0** |
| `load-360-recvnull-verify-20261116-profiled` | 445.37 | **508.9** |
| 区间 | 349–492（约 41%） | 417–535（22–28%） |

（稳态列按 `t1_avi` 取点；summary 是 5s 栅格，允许 ±1 帧/客户端/s 的取点差。）

| # | 判据 | 通过线 | 失败时的处置 |
| --- | --- | --- | --- |
| A1 | D1 的 AVI 低于**同树 D3** | **D1 ≤ 0.80 × D3(同树)** | 记为负结果：「分散未降低 AOI 广播量」，不得改口说「分散没用」或「分散有用」 |
| A2 | D2 的 AVI 低于**同树 D3** | D2 ≤ 0.90 × D3(同树) | 同上（D2 预期收益较弱） |
| A3 | 网关无背压恶化 | `overflow_frames=0`、`degrade_inworld_frames=0`、`close_inworld_sessions=0` | RED |

- 阈值 0.80 / 0.90 在**跑之前**登记；跑完不得改；**分母只能是同树 D3**。
- 为什么不能跨树：同树 A/B 的稳态 AVI 只差 **≈1.5%**（535.0 vs 527.0），但跨树
  recvnull/A/B/profiled 相差 **22–28%**（417–535）。⇒ 20% 的门槛只在同树内成立；
  跨树的绝对值只作背景区间，**不得当门槛、不得混算**。
- A1 的物理含义：如果分散只降低 game-svr 的发送编码（`HostAoiDownlink` 的 3–8%），而全局扫描不变，
  AVI 仍会明显下降——那说明收益在**网关侧**，不在 game-svr 工作段。两种结果都必须如实写。
- 若本轮没有同树 D3：A1/A2 判 **PREREQ（不可评估）**，只出 D1/D2 的绝对量与区间，不改判 GREEN。

### 3.5 与同图 500 档的对照方式

- **主对照 = 同树 D3**。D1/D2 的所有差值只对 D3 算，不跨树。
- 既有同图数据（W2 `c74b4ce42`、A/B `57a0c9f12`）只作**背景量级**，用于检查新数据是否落在
  已知区间（work 约 43.7–50.2ms、inbound 24.9–30.7ms、稳态 AVI 417–535 帧/s、实体均值约 4.7–4.9k）；
  **不得**把跨树差值写成「分散收益」。
- 如果 D3 无法在本窗口跑（例如 binary_stamp 已漂移），则本轮**只能出 D1/D2 的绝对量**，
  不得出「分散 vs 同图」的结论。

### 3.6 明确不许下的结论

- 不许由 500 档分散数据外推 10k（容量模型 §4.6：客户端轴 10k 实测/外推差 5.7×；分散未改变入站成本）。
- 不许把「本轮 GREEN」写成「500 档分散稳定通过」（既有 500 同图也有偶发 `recv_packet_null` 的 RED 轮）。
- 不许把 D1 的 AVI 下降写成「容量翻倍」；AVI 只是广播尝试量。

## 4. 可比口径（照此计算）

### 4.1 树与二进制

- 三档必须同一 `git rev-parse HEAD`、同一二进制 stamp（`binary_stamp` 是严格 head 相等；
  窗口期禁 push，先提交后 rebuild）。
- 每档**新栈**（现有实践：同栈连跑两档会让第二档进世界变慢 4×；`load_gate -RestartStackPerTier`
  或由 §5 的 wrapper 负责）。
- 归档必须记录：HEAD、`game-svr.exe` / `BotSrv.exe` 的 sha256 与 mtime、`preflight-env.txt`。

### 4.2 BotSrv 配置（三档逐字相同）

```text
# load_gate 参数（CLI）
-Tiers 500  -HoldSec 360  -SkipBuild  -BotHeapLimitGB 4  -MemKillThresholdGB 8

# BotSrv AppSetting.json（BotPlay 段）
ChrCount=500  TotalChrCount=500  LoginAccount=loadbot  NewAccount=false  ConnectStaggerMs=20

# BotSrv / game-svr 进程环境
MIR2_BOT_ACTION_PROBE=1  MIR2_BOT_ACTION_PROBE_MS=1000
MIR2_TICK_PROFILE=1      MIR2_TICK_PROFILE_SLOW_MS 不设（与 A 轮同形态）
```

- `BotHeapLimitGB=4` 与 500 基线一致（A 轮审计行 `BOTGUARD_ENV ... heap_limit_gb=4`）；
  8 只是 runbook 的旧写法，不是本轮实际值，不能用 8 制造口径差。

- `SLOW_MS` 不设：A 轮（主门开、快照关）是 500 档的正式口径；B 轮（快照开）只用于诊断代价实验。
- `NewAccount=false`：账号已存在；角色由 §5.2 的预热相创建/校验，不能让 BotSrv 在本轮新建角色
  （新角色 MapName 为空 ⇒ 落到 StartPoint 的 map 0，直接毁掉分布）。

### 4.3 稳态窗（与 `E:/UserFleet/load-360-recvnull-verify-20261116/env/loaded_window_compare.py` 同法）

1. **t0** = 服务端日志第 500 条 `进世界 ChrName:` 的时间戳；用 `tier-500/load_stats.ndjson`
   首次 `in_world >= 500` 的时间交叉核对，两者相差 >10s ⇒ PREREQ（记录但不出结论）。
2. **t_stop** = 发 game-svr `SHUTDOWN` 之前的墙钟时刻（`stack-stop.log` 的
   `STOPPED game-svr ... graceful` 行附近；`bots.log` 的 soft-close 可交叉核对）。
   收尾必须优雅存档，见 §5.2；`force` 停服的轮次 M4 判 PREREQ。
3. **AVI 的 `t1_avi`（写死，全表统一）** = **最后一条 gate summary 的时间 − 15s**；
   `att(t1)` 取时间最接近 `t1_avi` 的那条 summary（5s 栅格，允许 ±2.5s）。
   本设计固定用 `t1_avi`；**不得**用「末条事件 att ÷ (t1−t0)」——那会把 t1 之后约 15s 的流量
   算进分子，系统性偏高约 4%（第二轮复审 `581d6977` 的 P1）。若改用 `t_stop` 作 t1，
   必须预先声明并全表统一（该口径下四值为 415.4 / 530.5 / 525.7 / 509.2），不得与 `t1_avi` 混用。
4. **TICKPROF 稳态块**：只取完整 50-tick 块（5 tick/s ⇒ 10s/块），且满足
   `block_start >= t0` 且 `block_end <= t_stop − 15s`（15s 是 shutdown-drain 保护带）。
5. 块内各段按 **ticks 加权平均**；缺 `HostSleep` 或 `HostTickTotal` 的块整块丢弃并计数。
6. 上线爬坡段（t0 之前）单列，不混进 P99/均值；`ramp_ms` 单独报。
7. 收尾 drain 块一律不进稳态：禁止用「最后一块」或「全窗均值」。

### 4.4 归一化公式

| 量 | 公式 | 说明 |
| --- | --- | --- |
| 每客户端入站 | `HostInbound / 500` | ms/客户端/tick；C1 用 |
| 每客户端 AOI 下行 | `HostAoiDownlink / 500` | 记录，不单独设阈 |
| 工作段 | `HostTickTotal − HostSleep` | 与既有表同义 |
| 工作段占比 | `work / HostTickTotal` | 记录 |
| AVI | `[att(t1) − att(t0)] / 500 / (t1 − t0)`，att = `drop_noncritical + noncritical_sent`；t0 = §4.3 第 1 条，t1 = §4.3 第 3 条的 `t1_avi` | 帧/客户端/s；§3.4 用；**不得用 `window_sec`；t_stop 口径须全表统一声明** |
| 实体密度 | `entities_mean / 500` | 每客户端世界实体数（不是同屏） |
| 每簇成本 | 若必须分簇，只按 M3 的落点归属做静态分摊；**不假装有 per-map 运行时计费**（见 §6.2） |

### 4.5 实体密度、饱和与分箱

- TICKPROF 每块给出 `实体内 N`；报 `mean`、`last6_mean`、`prev6_mean`、`last6_vs_prev6_pct`。
- **饱和判据**（容量模型「口径要求」）：末 6 块相对前 6 块变化 **<1%** 才算饱和；
  否则所有容量数字标「未饱和 ⇒ 下界」。既有 500 同图 A 轮末段 +10.01%，未饱和。
- **对不齐实体数时用分箱**（容量模型 2026-11-15 的双因子法）：按块内 `entities` 分箱（宽 1,000），
  在**相同实体箱**内比较 D1/D2/D3 的 work、`HostInbound/客户端`、AVI；每箱至少 6 个块才出点，
  否则报「箱内样本不足」。这样即使三档的实体爬坡不同，也能在匹配密度下比较。

### 4.6 网关计数

- 每个 gate 取 `uptime_ms` 最大的那条 `gate_backpressure_summary`（与 `analyze_window.py` 同法）。
- gate summary 是**累计计数**（约 5s 一条）；AVI 必须做 t0/`t1_avi` 差分（§3.4、§4.3），
  `window_sec`/`uptime_ms` 只用于整窗背景值，**不进判据**。
- 报：`drop_noncritical_frames`、`noncritical_sent_frames`、`overflow_frames`、
  `degrade_inworld_frames`、`close_inworld_sessions`、`drop_pct`、`window_sec`。
- `drop_pct` 只是「丢弃 /（丢弃+发送）」，**不是**容量指标；AVI 才是广播量指标。

### 4.7 报告必须有的表

每档一行：

```text
tier | verdict | in_world | dist_ok(M1-M4) | work | HostInbound | per_client_inbound
     | HostAoiDownlink | AVI_steady（同时记 AVI_whole） | P50/P90/P99/max | ge201
     | entities_mean | last6_vs_prev6_pct | saturation | AVI_vs_D3 (同树) | ramp_ms | census20_zero
```

另出「D1 vs D3」「D2 vs D3」两列差值表，并注明这是**同树配对**。

### 4.8 跨树规则

- 既有同图数据（`docs/capacity-500-conclusion-20261116.md` 的表）与本轮**不同树**，
  只能作区间检查；不得混算、不得平均、不得把两棵树的比值当收益。
- 若必须在旧树上比较，唯一合法做法是「同树重跑 D3」；不能拿 W2/A/B 代替。

## 5. 成本

### 5.1 窗口轮次与独占时间

| 项 | 轮数 | 每轮 | 小计 |
| --- | --- | --- | --- |
| 空载 preflight（CPU<20%、无 cargo/dotnet/msbuild、端口/二进制/HEAD） | 1 | 5 min | 5 min |
| 冒烟（500、`HoldSec=60`，验 M1/M2 分布、优雅停栈 + 跑后快照 M4、无异常） | 1 | 5 min | 5 min |
| 结论轮（D3 / D2 / D1，各 `HoldSec=360`、新栈） | 3 | 8.9–10.0 min | 27–30 min |
| 归算与归档（TICKPROF 稳态窗、分布、AVI、分箱、sha manifest） | 1 | 20 min | 20 min |
| **合计（一轮/档）** | | | **约 57–60 min；按 60–75 min 申请独占** |

- 每轮 8.9–10.0 min 的构成：优雅停栈（game-svr 管道存档，≤8s）15–20s → 单起 MySQL 10s →
  夹具写库 15–30s → 起整栈 60s（MySQL 已在跑，栈只打 `MYSQL already up`）→ 上线 15s → hold 360s →
  优雅停栈 15–20s → **跑后 DB 快照** 5–10s → 停 MySQL 5s → 归档 30–45s。
  与 500 档 runbook 的 45 min（smoke+1 轮+分析）同量级；多出的时间主要是每档两次优雅停栈。
- **若 A1/A2 要当「结论」而不是「探索」**：每档再加 1 轮（同样的 360s），
  合计 +27–30 min ⇒ **约 84–90 min（= 57–60 + 27–30），按 85–95 min 申请**。
  建议先跑一轮/档；若 AVI 的轮间差 >10% 再补第二轮。
- 若需要「饱和态」的绝对容量数字：把 D1 的 hold 延到 600s（+4 min）；本轮**不默认**做。

### 5.2 能复用现有栈/二进制吗？

**能，且不需要重建产品代码。**

- 栈：测量栈由 `tools/m2-acceptance/start_rust_stack.ps1` 起（MySQL + DBSrv + LoginSrv +
  Rust game-svr + 三网关）；单二进制 dispatcher 形态则用 `start_mir2_server.ps1`。
  **停栈必须走 game-svr 的优雅通道**：`start_rust_stack.ps1 -Stop`（或 `stop_mir2_server.ps1`）
  会连接 `openmir2-gamesvr-shutdown-<pid>` 写 `SHUTDOWN`，进程先存档再退出（8s 上限，内部存档 5s）；
  **不得用 `E:/MirServer/start-all.ps1 -Stop` 或 `Stop-Process -Force`**——那条路径不存档，M4 无生产者。
- 单起 MySQL 的写法直接复用 `tools/e2e/stack_e2e.ps1` 里的 `mysqld --datadir=D:\mysql\data ...`。
  **推荐的每档时序**是「先单起 MySQL（不被栈的 pid 文件记录）→ pin → 起栈（栈见 3306 已通，
  只打 `MYSQL already up`、不接管）→ 跑 → 优雅停栈（只停栈内进程，MySQL 仍在）→ 跑后快照」；
  这样跑后快照不需要重启数据库。若 MySQL 是由栈自己启动的（pid 文件含 `MySQL`），
  优雅停栈会把它一起停掉，则快照前再单起一次。
- 压测：`tools/botload/load_gate.ps1`（`-Tiers 500 -HoldSec 360 -SkipBuild`）、
  `tools/botload/run_bots.ps1 -StopAfterCharCreated`（预热建角）。
- 归算：`E:/UserFleet/load-360-recvnull-verify-20261116/env/loaded_window_compare.py` 的稳态窗逻辑，
  加一个只读的分布/AVI 汇总。
- 二进制：当前 stamped build；若窗口前必须重建，按项目规则重建 `mir2-server`（dispatcher）
  并核 `mtime + sha256`，仅 build `-p game-svr` 不会更新 dispatcher。

### 5.3 需要新增的件（只写职责，不写代码）

1. **`tools/botload/distmap_fixture.py`（必需）** — 数据夹具：
   - 读 §2.3 的档位计划；读 `StartPoint.txt`，确认坐标在该文件里；若扩到无出生点的图，
     按 `.map` 格式扫 5×5 全 WALK 的补丁作为坐标（缺坐标 ⇒ rc=2，不猜）。
   - 按 `LoginID`（`loadbotN`）解析账号 → 角色；要求每个账号至少 1 个未删除角色；
     一个账号多角色时**按账号整组写同一图**，避免「选中的是哪一个」不确定。
   - 写 `characters(MapName, CX, CY, HomeMap, HomeX, HomeY)` + `characters_ablity(Hp=MaxHP)`。
     `HomeMap` 必须写成目标图：进图改写与死亡改写用的是**记录 HomeMap**（`rewrite_enter_position`
     读过记录），只改 `MapName` 不够——见 §6.1 R3。
   - 默认 `--dry-run`；`--apply` 才写库；库名守卫只允许 `mir2_test_*`；写前后各出一份
     `(LoginID, ChrName, MapName, CX, CY, HomeMap, Hp, MaxHP)` 快照，`after == plan` 否则 rc=1。
   - `verify-log` 模式：解析 game-svr 的 `进世界` 行，按 §3.2 输出每图/每簇计数与 mismatch。
   - `snapshot-post` 模式：优雅停栈后读 `characters`/`characters_ablity`（3306 须可用；必要时单起一次），
     输出每账号 MapName/HP 与分簇计数，供 M4。**双重前置断言**：`STOPPED game-svr ... graceful`
     且 `关服存档收尾：… 超时 false` + `应答==发起`；任一缺失/为假 ⇒ rc=2（不当作跑后状态）。
   - 自测：空输入、未知图、墙格坐标、计划 500/实际 499、单簇少 1、全在 map 0 必须红；
     不得连真库跑自测（合成夹具 + 假 DB）。
2. **`tools/botload/distmap_window.ps1`（必需，或给 `load_gate` 加一个 `-PreTierHook`）** — 编排：
   - 顺序固定：`start_rust_stack.ps1 -Stop`（断言 `STOPPED game-svr ... graceful`）→ 单起 MySQL →
     fixture apply + 写前快照 → 起栈（`MYSQL already up`）→ 等 7000 端口 →
     `load_gate -Tiers 500 -HoldSec 360 -SkipBuild`（栈已在跑，不再 restart）→
     `start_rust_stack.ps1 -Stop`（再次断言 `graceful`）→ fixture **跑后 DB 快照**（M4，
     MySQL 仍在；若 3306 已 DOWN 则单起一次）+ `verify-log` → 停 MySQL。
   - 若停栈日志出现 `STOPPED game-svr ... force`、没有 `graceful` 行、或 game-svr 日志缺
     `关服存档收尾：… 超时 false` / `应答 != 发起` ⇒ 该档 M4 判 PREREQ，
     不得把快照当「跑后状态」。
   - 任一 rc≠0 立即中止，不跑下一档；把每档的计划、实际分布、HEAD、二进制 sha 写进归档。
   - 为什么必须新编排：`load_gate -RestartStackPerTier` 在**脚本内部**重启栈，外部无法在
     「重启后、bot 启动前」插写库；单靠现有参数做不到「每档先 pin 再跑」。
3. **只读分析器（可并入 fixture 的 verify，或扩 `analyze_window.py`）** — 稳态窗、分布、AVI、
   per-client 归一、实体分箱、D3 差值表；输入只有归档文件，不连库、不起进程。

三个件都是测试侧 Python/PowerShell，**不改 Rust/C#、不改协议、不改 Envir**；不需要 cargo/dotnet 构建。

### 5.4 磁盘与清理

- 每份归档约 41MB（W2 实测）；三档 + 冒烟约 **130–170MB**。
- DB 侧只改 `loadbot*` 角色的 7 列（`characters` 的 MapName/CX/CY/HomeMap/HomeX/HomeY +
  `characters_ablity` 的 Hp）；快照保留；窗口结束后可恢复原值或明确记录「保持分散值」。
  测试数据由创建方清理（列清单、备份、核验三步）。

## 6. 风险与不可行项

### 6.1 风险清单

| # | 风险 | 机制/证据 | 影响 | 处置 |
| --- | --- | --- | --- | --- |
| R1 | **D2 的「真实分布」不存在** | 无生产遥测；真实分布属未公开数据 | D2 的结论不可外推、不可称「真实」 | 标题写 `assumed-weights`；D2 只作「头部集中」假设；灵敏度见 §6.2 |
| R2 | **分散不会降低全局 AOI 扫描** | `hear_targets_at_scan` 对所有实体做 `iter_send_ref_targets` 后按 `map_id` 过滤；`docs/host-aoi-profile-20261114.md` §0 记录扇出占 `HostAoiDownlink` 95.5% | game-svr 工作段可能几乎不降；只有编码/发送/网关量下降 | 预登记 AVI（§3.4）；把「工作段不降」当正常结果写，不算失败 |
| R3 | **HP≤0 的记录会在进图时被送回 HomeMap** | `rewrite_enter_position`：记录 `hp<=0` ⇒ `relocate_on_death` 用**记录 HomeMap**；上一轮死掉的 bot 存档 HP=0 | 下一轮「看起来 pin 了但人在 map 0/别处」，分布 RED | fixture 同时写 `HomeMap=目标图` 且 `Hp=MaxHP`；M4 用停机后 DB 快照复核 |
| R4 | **DBSrv 角色索引是启动缓存** | `Index(sChrName)` 读 `_NameQuickMap`（启动时建）；`Query` 每次读库 | 本轮新建的角色不会被索引，选角失败/退回建角 | 角色必须在起栈前已存在（预热相独立一轮）；pin 在起栈前完成 |
| R5 | **pin 与 GameSvr 存档竞态** | 关服时 `MakeSaveRcd` 写回当前图/坐标 | 边跑边改库会被覆盖；或改了还没进世界的账号状态 | 只在栈停、MySQL 单起时写；bot 从未进世界时不许改 |
| R6 | **簇间负荷不均** | §2.2：半径 50 内 MonGen 从 0 到 472 | D1 的均值被重/静簇混合，效应可能被稀释 | 报告逐簇表；必要时用「同 MonGen 箱」配对或换坐标重跑（fixture 支持坐标扫描） |
| R7 | **出生重叠推挤在 ramp 不同** | 现状 250/簇 ⇒ 10 bot/格；D1 63/簇 ⇒ 2.5 bot/格；`CharPushed` 逐格发 RM_PUSH | ramp_ms/进图时延不同；稳态无影响 | ramp 单列；不把 ramp 的差异写进稳态结论 |
| R8 | **实体未饱和** | A 轮末段 +10.01%，均值 4,886 仍在涨 | 任何绝对容量数都是下界 | 报饱和标记；用实体分箱（§4.5）；不拿「末块」当稳态 |
| R9 | **跨树不可比** | 既有同图数据在 `c74b4ce42`/`57a0c9f12`；本轮树不同 | 混合出表会伪造收益 | 主对照只用同树 D3；跨树只做区间检查 |
| R10 | **地图 flag 差异** | 六图均 DAY、无 FIGHT/NORECONNECT（已核 `MapInfo.txt`） | 昼夜/战斗规则差异 | 已规避；扩图时逐图核 flag |
| R11 | **5/11 簇近乎静默** | 出生点 ±100 内 MonGen 为 0 | D1 的平均被两个「空图」拉低；不是最坏上界 | 明确 D1 是「均匀中位形态」，最坏上界仍是 D3 |
| R12 | **1000 档分散被顺手加入** | 1000 档有 BotSrv 4GiB/网关丢帧/串行载入三项前置，均未解除 | 窗口必失败、证据作废 | 本设计**明确不含 1000 档**；另卡处理 |
| R13 | **窗口独占/构建抢内存** | 项目既有规则：窗口期禁 cargo；多车道并行构建会被 reaper 杀 | 判据噪声/进程被杀 | preflight 记录 `heavy_samples`；窗口期不跑构建 |
| R14 | **DB 共享测试数据被别的车道使用** | 500 账号/角色是共享夹具 | 别的车道跑出混合分布 | 写前快照、写后记录；与 Leader 约定窗口独占 `loadbot*` 账号 |
| R15 | **强制停服/存档超时会让 M4 没有生产者** | `start-all.ps1 -Stop` / `Stop-Process -Force` 不存档；`shutdown_save_all` 超时也会打印 graceful | 跑后快照与在world分布不一致，或读不到最新存档 | 命名管道优雅停；断言 `STOPPED game-svr ... graceful` **且** `关服存档收尾：… 超时 false`、`应答==发起`；快照前确认 3306 可用（必要时单起） |

### 6.2 不可行项（明确写出）

1. **获取「真实玩家分布」**：没有生产遥测，也拿不到可公开的分布；用生产数据还涉及敏感信息授权。
   ⇒ 只能做声明式假设（D2），不能声称测量了真实分布。客观替代 = MonGen 权重
   （六图 3,828 只怪的权重：0=45.3%、2=26.4%、4=12.4%、5=9.1%、3=5.9%、11=1.0%），
   但那是**怪物分布**，不是玩家分布，局限更明显；只作为灵敏度，不进主档。
2. **运行时 per-map 实体数**：现有遥测只有世界总实体数（TICKPROF 的 `实体内 N`）；
   没有按图分解的实体计数。不新增产品仪表就不可得；本设计的判据不依赖它，
   只用**静态 MonGen 协变量 + 每图 bot 数**。若后续需要，另卡加只读 census。
3. **不写库就做确定性分配**：`StartPointSize=2` 的出生是随机的，且改 `StartPoint.txt` 属于
   部署内容变更（内容冻结）。⇒ 确定性分布**必须**走 DB 夹具；这是本设计的硬前提。
4. **1000 档分散**：三项前置（BotSrv 堆、网关背压、串行载入）未解除前不可行，见 R12。
5. **与基线逐字节同落点**：基线是 `Random(3)` 抖动；本设计用确定性抖动，只能保证**同分布族**，
   不能保证逐点相同。不要把「落点不同」误判成回归。

### 6.3 我认为最可能跑不起来的一档

**D1（8 簇 / 6 图）。** 原因不是地图本身（8 个出生点都已验证 5×5 全 WALK、六图均已注册且 flag 干净），
而是它叠了最多的独立前置，且**每个前置失败都是静默的**：

1. **角色必须已存在**（R4）：若预热相没把 500 个角色的索引建进 DBSrv，BotSrv 会走「无角色 ⇒ 建角」，
   新角 MapName 为空 ⇒ 落 map 0；日志里照样 500 人在世界，只有 M2/M1 能抓出来。
2. **必须同时写 HomeMap + HP**（R3）：只写 MapName，上一轮死掉的 bot 会在进图改写时回 `HomeMap`
   （默认 `0`），D1 变成「部分回 map 0」；第一次跑很容易漏。
3. **写库时序**（R5）：必须在栈停、MySQL 单起时写；如果图省事在栈跑着时写，会被关服存档覆盖，
   或写进了还没选角的会话。
4. **五张非默认图 + 三个 map 0 簇**：任何一张图名/坐标/账号组写错，M2 逐图相等就红；
   D3 只有一张图、两簇，对同样的夹具 bug 有很强的容错（bug 也表现为 map 0，恰好等于计划）。

D2 的问题不同：它能跑，但「近似真实」是**有效性**问题（R1），不是执行问题；
如果 Leader 不接受 `assumed-weights` 标签，D2 应降级为「头部集中灵敏度」，而不是当成一档测量。
