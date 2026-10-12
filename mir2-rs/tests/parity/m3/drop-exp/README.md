# drop-exp —— M3④ 掉落/经验跨侧对拍夹具（骨架）

> 卡：`openmir2-m3-drop-exp-align`（M3 逐系统对齐 ④ 掉落/经验）。
> 本目录交付的是**夹具骨架**（不占窗口）：判据本体（比较器 + fail-closed + 改坏必红）已完整、
> 可离线复跑；真机 100 杀需要窗口 + BotSrv 侧驱动（承接卡见
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

## 文件

| 文件 | 作用 |
| --- | --- |
| `drop_exp_scenario.json` | 固定输入（`kills=100`、`seed=42`、`map=0137`、`monster=稻草人`、attacker pose `(17,20) dir=2`）与记录 schema |
| `drop_exp_diff.py` | 判据本体（`diff` / `selftest`） |
| `run_drop_exp_pair.ps1` | 夹具入口（`-SelfCheckNoStack` / `-DiffOnly` / real-run fail-closed） |

## 命令

```bash
# 判据本体自检（fail-closed + 改坏必红）：期望 SELFTEST ok=12/12 rc=0
python -I mir2-rs/tests/parity/m3/drop-exp/drop_exp_diff.py selftest

# 无栈自检：比较器 selftest + 进程内集成 + LF 写出 + 零端口；期望 SELFTEST ok=7/7 rc=0
powershell -NoProfile -ExecutionPolicy Bypass -File mir2-rs/tests/parity/m3/drop-exp/run_drop_exp_pair.ps1 -SelfCheckNoStack

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

1. **C# 基线**：`run_drop_exp_pair.ps1 -Side csharp -Kills 100 -Seed 42 -Prefix m3cde1`（内部复用 `run_pair.ps1` 的两相 pin + peer 护栏 + 会话入库）。落 `baseline.json.start_pose` 与 `drop_exp_raw.jsonl`。
2. **Rust 同序列**：`run_drop_exp_pair.ps1 -Side rust -Kills 100 -Seed 42 -Prefix m3rde1 -PeerSession <第 1 步会话>`。
3. **判据**：`run_drop_exp_pair.ps1 -DiffOnly -CsharpRaw <csharp drop_exp_raw.jsonl> -RustRaw <rust drop_exp_raw.jsonl>` ⇒ 期望 `DIFF 0`。
4. **收工**：`-Stop` + 十端口实拍 + 卡上写「窗口已释放」+ 证据入库（`SHA256SUMS.txt` 按仓库 LF/git-blob 口径）。

> 现状：第 1/2 步的 BotSrv「掉落/经验序列」尚未实现（承接卡见报告），real-run 因此 fail-closed。
