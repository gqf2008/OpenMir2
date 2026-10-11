# M2 ③ 差异登记落签（owner §25.3）——46.param / 50.series 白名单

- 卡：`openmir2-m2-gamesvr-host`（M2 验收①②③）
- 本片写集：`tools/m2-acceptance/**`（白名单 + harness 传参）、`docs/**`
- **未改**：`crates/**`、`src/**`（C# oracle）、`MirClient`、表结构/脚本语法/数据格式；**未 merge、未签 review**
- 依据：owner §25.3 裁决——`52 WAbil` 是真缺陷（归 `m3bag` 修，本 lane 接线后复查）；
  `46.param`（Bright 按真实小时）与 `50.series`（MakeWord(Dir,Light)，取决于采集时朝向）**登记白名单，不阻塞「同序 + 等长」**。

## 1. 白名单条目（`tools/m2-acceptance/whitelist.jsonl`）

owner §25.3 点的是**两条根因**：`ident=46 field=param`、`ident=50 field=series`。
`s2c_diff.py` 对**头部字段**差异会同时产出一条逐字段行 + 一条同一字节的 `raw_hex` 行
（raw_hex 是整帧编码后的投影）⇒ 为**不改判据、不外扩字段**把 ③ 判绿，文件里显式登记 4 行
（raw_hex 行与根因行同因、同审）。若 owner 要求严格两条，删掉两行 raw_hex 即可（届时 raw_hex 会判红）。

| 编号 | ident | field | 为什么无害 |
| --- | --- | --- | --- |
| ③-1 | 46 | param | `Bright = M2Share.GameTime` 按**真实小时**（`SystemProcessor.cs:98-120`）；金标准 09:28=白天(1) / 本片 01:28=夜间(3) |
| ③-2 | 46 | raw_hex | 同上，byte 投影（只差 12B 头 param 字节 1↔3） |
| ③-3 | 50 | series | `series = MakeWord(Dir, Light)`，取决于采集对象朝向；金标准角色 `Dir=0` / 本片观察者 `Dir=4` |
| ③-4 | 50 | raw_hex | 同上，byte 投影（只差 12B 头 series 字节 0↔4） |

## 2. 两侧采集口径（owner §25.3 要求如实写明）

| 项 | 金标准 `c-line-baseline-c3-20261010.jsonl` | 本片 Rust 侧 capture |
| --- | --- | --- |
| 采集时刻 | 2026-10-10 **09:28**（白天） | 2026-10-12 **01:28**（夜间） |
| `46.param`(Bright) | 1 | 3 |
| 采集角色 `Dir` | 0 | 4 |
| `50.series` | 0 | 4 |

两条差异都取决于**采集时刻 / 采集对象的即时状态**，不是 Rust 实现偏差（C# 侧同时刻也会得到相同的本片值）。

## 3. 复跑与判定

```powershell
python tools/m2-acceptance/s2c_diff.py `
  --rust tools/m2-acceptance/evidence/m2svr-run-20261012-2/s2c/capture.jsonl `
  --golden mir2-rs/tests/golden/c-line-baseline-c3-20261010.jsonl `
  --start-ident 658 --upto-ident 46 `
  --whitelist tools/m2-acceptance/whitelist.jsonl `
  --out tools/m2-acceptance/evidence/m2svr-whitelist-20261012/s2c-diff.json
```

**结果**（`s2c-diff.txt` / `s2c-diff.json`）：

- **同序 + 等长 GREEN**：15/15 ident 逐项相等（`658,51,50,41,213,11029,42,766,54,5008,1106,52,752,811,46`）。
- 4 条已登记差异进 `whitelisted`；**剩 2 条未登记差异全在 `52 SM_ABILITY`**（`body_sha256` + `raw_hex`，即 `max_hp/max_mp=0` 的能力层缺口）。
- `exit=1`（RED）：**③ 在 `52 WAbil` 修好前不可能 GREEN**——`52` 归 `m3bag` 修（owner §25.2），本 lane 接线后复查。
  这不是未知故障，是本片如实登记的状态。

## 4. 失败怎么判红

- 白名单条目缺 `reason`/`reviewer` ⇒ `s2c_diff.py` 直接 `exit 1`（§6.0-3，不静默放行）。
- 出现**未登记**差异 ⇒ `exit 1`；缺 capture/金标准 ⇒ `exit 2`（PREREQ）。
- `run_acceptance.ps1` 默认带上本白名单（`tools/m2-acceptance/whitelist.jsonl` 存在时）；文件缺失则不传，对账按 0 差异判红。

## 5. 残留物清单

- 本目录：`s2c-diff.{json,txt}`（新判据）、本 README（证据，入库）。
- 原始 capture 复用已入库的 `tools/m2-acceptance/evidence/m2svr-run-20261012-2/s2c/capture.jsonl`（未改）。
- 仓库外无新增残留物。
