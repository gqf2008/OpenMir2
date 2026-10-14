# drop-exp —— M3④ 掉落/经验跨侧对拍夹具（骨架）

> 卡：`openmir2-m3-drop-exp-align`（M3 逐系统对齐 ④ 掉落/经验）。
> 本目录交付的是**夹具骨架 + 场景纯度门**（不占窗口）：判据本体（比较器 + fail-closed + 改坏必红）
> 与 **whitelist T-35 的纯度门**（`drop_exp_purity_gate.py` + `drop_exp_consumers.tsv`）都已离线就绪、可复跑；
> 真机 100 杀需要窗口 + BotSrv 侧驱动（承接卡见
> [`M3④掉落经验现状盘点与夹具骨架报告-20261013.md`](../M3④掉落经验现状盘点与夹具骨架报告-20261013.md)），
> 在此之前 real-run 分支 **fail-closed 拒跑**，不静默起栈。

## 判据（对应卡内 `machine_checkable_acceptance` ②③）

固定图 + 固定怪 + 固定起点 + 同种子杀 **100** 次，两侧各落一份 `drop_exp_raw.jsonl`
（一次击杀一行），跨侧比较：

1. **掉落列表逐项相同**：按**多重集/计数式**（`Counter`）比，同时逐槽位比 —— 定位到
   「第 k 次击杀 / 第几个掉落槽」；**不许 set 差集**。
2. **经验曲线逐级相同**：`exp_delta`（本次增量）、`exp_total`（累计）、`level` 逐 k 相同。
3. **金额/经验不得 i32 截断**：比较器按 Python 任意精度整数**原宽度**比；`selftest` 有
   `i32-truncation-red` 用例钉住「只在高 32 位不同的两个值必须判红」。
4. **fail-closed**：缺文件 / 缺字段 / 非法 JSON / k 不连续 ⇒ 非零退出并点名（`PREREQ_RED`）。
5. **改坏必红**：改掉落项名 / 金币 / 经验 / 掉落条数 / 记录条数 / 高 32 位，逐个必须判红。
6. **场景纯度（whitelist T-35，owner comment `c78bd13f`）**：除被测量路径（1 只 pin 怪 + 击杀→掉落/经验取数）外 **RNG 消费者数 = 0**，由 `drop_exp_purity_gate.py` 按**实测抽数流水**证明（逐消费者清单 + 计数）；**墙钟消费者必须压到 0**（本夹具取 (a) pin 抑制）；**认不出 / 缺 site / 未分类 ⇒ 点名 RED，拒比**。
7. **消费者表锚点保鲜（本片新增）**：`drop_exp_consumers.tsv` 的 `oracle_ref` 是站点归因的**唯一人复核入口**，必须 fail-closed 保鲜：① 存在性 + 行号范围；② **C# 行的 ref 必须落在其 `pattern` 点名的 `Class.Method` 方法体内**（只查存在/范围抓不到「行号在文件里但方法不对」）。真身缺陷：`RegenMonsters` 行的 ref 曾写 `WorldServer.MonGen.cs:238`，该行实际落在 `ProcessMonsters` 里。③ **2026-10-14 收口 O-1**（reviewer-5 `f7318e59`）：方法体识别只认真**声明**——旧口径把 `if (_Attack(...)) { … }` 这类**调用点块**误当方法体（伪 range `(13,17)`）⇒ 潜在假负通道；正则族名（`UpgradeRandom\w+`）按字面前缀 `UpgradeRandom\w*(` 匹配。

## 文件

| 文件 | 作用 |
| --- | --- |
| `drop_exp_scenario.json` | 固定输入（`kills=100`、`seed=42`、`map=0137`、`monster=稻草人`、attacker pose `(17,20) dir=2`）与记录 schema |
| `drop_exp_diff.py` | 判据本体（`diff` / `selftest`） |
| `drop_exp_purity_gate.py` | **纯度门**（whitelist T-35；`gate` / `selftest`） |
| `drop_exp_consumers.tsv` | **消费者表（纯度门唯一真源）**：`side/class/pattern/max/oracle_ref/note`；`oracle_ref` 由门 `selftest` 保鲜（存在/范围 + C# 方法体归属） |
| `run_drop_exp_pair.ps1` | 夹具入口（`-SelfCheckNoStack` / `-DiffOnly` / `-PurityOnly` / real-run：`-Go` 后复用 `run_pair.ps1` 两相 pin + `-DropExpSequence`） |

## 命令

```bash
# 判据本体自检（fail-closed + 改坏必红）：期望 SELFTEST ok=12/12 rc=0
python -I mir2-rs/tests/parity/m3/drop-exp/drop_exp_diff.py selftest

# 纯度门自检（fail-closed + 改坏必红 + 真表承重 + oracle_ref 保鲜）：期望 SELFTEST ok=32/32 rc=0
python -I mir2-rs/tests/parity/m3/drop-exp/drop_exp_purity_gate.py selftest

# 无栈自检：比较器 + 纯度门 + LF 写出 + 零端口 + no-Go fail-closed + DoD② run_pair/BotSrv DropExp 透传 + BotSrv 生产函数 selftest；期望 SELFTEST ok=20/20 rc=0
powershell -NoProfile -ExecutionPolicy Bypass -File mir2-rs/tests/parity/m3/drop-exp/run_drop_exp_pair.ps1 -SelfCheckNoStack

# 场景纯度（离线；给该侧全场景抽数流水；认不出/未分类 ⇒ 点名 RED）
python -I mir2-rs/tests/parity/m3/drop-exp/drop_exp_purity_gate.py gate \
    --side csharp --stream <rng-stream.tsv> --expected-kills 100 --raw <drop_exp_raw.jsonl>

# 只比较两份既有原始件（离线，不用栈）
powershell -NoProfile -ExecutionPolicy Bypass -File mir2-rs/tests/parity/m3/drop-exp/run_drop_exp_pair.ps1 -DiffOnly -CsharpRaw <csharp.jsonl> -RustRaw <rust.jsonl>
```

## 记录 schema（每行一个 JSON 对象）

```json
{
  "k": 0,
  "monster": "稻草人",
  "x": 17, "y": 20,
  "gold": 37,
  "drops": [
    {"name": "金币", "count": 37},
    {"name": "木剑", "index": 2, "count": 1, "dura": 13, "dura_max": 20}
  ],
  "exp_delta": 120,
  "exp_total": 120,
  "level": 1,
  "hero_exp_delta": 0
}
```

必填：`k/gold/drops/exp_delta/exp_total/level`；可选：`monster/x/y/hero_exp_delta`。
`k` 必须从 0 起连续。

> **英雄经验口径**：oracle（本代码库）没有独立的英雄经验表；「英雄/下属」路径 = 宠物击杀走
> `ExpHitter.Master`（`src/M2Server/Actor/BaseObject.cs:2434-2437`）。本场景单 attacker 不覆盖，
> 登记为承接项（报告 §4 / whitelist T-33）。`hero_exp_delta` 字段保留：两侧都缺 ⇒ 不参与比较；
> 任一侧给出 ⇒ 逐 k 比较。

## 真机 100 杀的命令序列（等 owner 发窗口后照此跑）

> 起栈命令**禁接管道**（`| tee`/`| tail` 会挂住调用方），一律重定向到文件。

1. **C# 基线**：`run_drop_exp_pair.ps1 -Side csharp -Kills 100 -Seed 42 -Prefix m3cde1`（内部复用 `run_pair.ps1` 的两相 pin + peer 护栏 + 会话入库）。落 `baseline.json.start_pose`、`drop_exp_raw.jsonl` 与**全场景抽数流水**（`MIR2_RNG_LOG`）。
2. **Rust 同序列**：`run_drop_exp_pair.ps1 -Side rust -Kills 100 -Seed 42 -Prefix m3rde1 -PeerSession <第 1 步会话>`。落 `drop_exp_raw.jsonl` 与 Rust `rng-stream.tsv`。
3. **场景纯度（两侧，先于比较）**：对每侧的全场景流水跑 `drop_exp_purity_gate.py gate`（见上），**必须 GREEN**（`wallclock=0 / other=0 / unknown=0` 且 `measured>0`）。若报 `unknown_consumer`：**不许**为了拿绿放宽分类——把该站点按 oracle 证据显式写进 `drop_exp_consumers.tsv`（`measured`/`setup`/`wallclock`/`other`）后再跑；报 `wallclock_consumer` ⇒ pin 抑制未生效，**拒比**。
4. **判据**：`run_drop_exp_pair.ps1 -DiffOnly -CsharpRaw <csharp drop_exp_raw.jsonl> -RustRaw <rust drop_exp_raw.jsonl>` ⇒ 期望 `DIFF 0`。
5. **收工**：`-Stop` + 十端口实拍 + 卡上写「窗口已释放」+ 证据入库（`SHA256SUMS.txt` 按仓库 LF/git-blob 口径，含两侧纯度门的 `--json` 原始件）。

> 现状：BotSrv「掉落/经验序列」驱动 + `--selftest-dropseq` 已实现（卡 `openmir2-m3-drop-exp-fixture-botsrv` DoD①）；
> DoD② 接线（`run_drop_exp_pair.ps1` real-run → `run_pair.ps1 -DropExpSequence` → `tools/botload/run_bots.ps1` DropExp* 配置透传）已落地，无栈自检 20/20。
> **仍未接线的依赖**：「0137 上运行时 pin 出单只固定怪」（C# 侧 GM/脚本 spawn + Rust 宿主 AI 模式开关），归 shared 承接卡
> `openmir2-m3-rng-scene-driver`（规格见 [`rng-trace-scene/DRIVER-CONTRACT.md`](../rng-trace-scene/DRIVER-CONTRACT.md) §7，与 ④ 同批窗口）；
> 单只怪未落地时 `run_pair` 的 DropExp 分支会在「0 杀 < -DropExpKills」处 RED（fail-closed，不符就是不符）。
> 纯度门（T-35）**离线已就绪**（门 selftest 32/32、无栈自检 19/19）；`drop_exp_consumers.tsv` 的
> **两侧实测站点行**已按 oracle 源码 + 真机实测（C# 掉落/构造/墙钟 + `rng-isolate-attack/attack_sites.tsv` 的
> 两侧攻击路径）与 ④-D（master `4521d451`）Rust 实现文件级锚补入：Rust 侧不再判 `PREREQ`，`measured`
> （drop/kill）/`setup`（spawn/构造）/`wallclock`（monai/merchant）各类都有站点行；`max` 未实测者留 `*`，
> 窗口期按真实计数收紧。未分类站点仍判 `RED`、墙钟站点仍判 `RED`（**拒绝假绿**）。
