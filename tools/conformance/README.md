# tools/conformance — 双栈对拍比较器

`compare_runs.py` 吃**两次无头对拍各自的产物目录**（Rust 一次、C# oracle 一次），
输出「帧流 + 数据库后态」的逐项 diff 报告，并给出 PASS / FAIL / UNDECIDABLE。

它与驱动器（`tools/probe/`，产物 `frames.ndjson` + `run.json`）**完全解耦**：只读目录/文件，
不 import 对方的任何模块，不假设对方在本仓库里。对方格式变了，这里只会在解析层报
UNDECIDABLE，不会静默少比。

全离线：不连网、不连库、不起进程、不写数据库；库快照只读 JSON→临时 tsv。

---

## 用法

```sh
python -I -B tools/conformance/compare_runs.py \
    --oracle E:/tmp/run-oracle --rust E:/tmp/run-rust \
    --out report.json
```

完整参数：

| 参数 | 说明 |
| --- | --- |
| `--oracle DIR` / `--rust DIR` | 两侧产物目录（必需） |
| `--rules FILE` | 规则表（默认 `tools/conformance/rules.json`） |
| `--out PATH` | 报告路径（默认 `report.json`；`-` = stdout） |
| `--allow-unclassified IDENTS` | 显式放行未覆盖 ident（逗号分隔，可重复）；仍会在报告里登记 |
| `--db-oracle PATH` / `--db-rust PATH` | 覆盖库快照位置（默认 `<side>/db_snapshot.json`） |
| `--db-time-mode exact\|presence\|normalized` | 覆盖规则表 `db.time_mode` |
| `--db-key-mode pk\|multiset` | 覆盖规则表 `db.key_mode` |
| `--db-ignore-columns A,B` | 覆盖规则表 `db.ignore_columns`（只允许已登记易变列/时间列/主键） |
| `--db-allow-unknown-table` | 放行未登记表（默认未知表 ⇒ 无法判定） |
| `--quiet` | 不打印终端摘要 |
| `selftest` | 离线自检（见下） |

## 退出码（唯一的判定口径）

| 码 | verdict | 含义 |
| --- | --- | --- |
| 0 | `PASS` | 帧流与库后态在规则范围内全等 |
| 1 | `FAIL` | 有真实差异（帧 body/头部、帧数、库单元格/行） |
| 2 | `UNDECIDABLE` | **判不了**：缺产物、解析失败、规则没覆盖（UNCLASSIFIED）、40/58 解不出、只一侧有库快照、库快照结构问题…… |

**同时出现差异与「判不了」时取 2。** 未完成的判据优先，绝不用「有差异」掩盖「没判完」；
也绝不在缺规则/缺数据时静默当绿。

## 输入：驱动器产物约定

`<DIR>/frames.ndjson`（必需，一行一帧 s2c）：

```json
{"seq":1,"t_ms":1000,"phase":"login","conn":"bot0","ident":50,
 "recog":123,"param":10,"tag":20,"series":3,
 "body_len":0,"body_hex":"","body_sha256":"e3b0c442..."}
```

* `ident` 必需（u16）；`body_hex` 是**解码后的 body 字节**（十六进制，可带 `0x`）。
* 其余字段缺失可容忍，但会被**显式登记**：
  * 两侧都缺某头部字段 ⇒ 报告 `missing_fields`（该字段未比较）；
  * 只有一侧缺某头部字段 ⇒ UNDECIDABLE；
  * 两侧都没 `body_hex`/`body_sha256` ⇒ UNDECIDABLE（`body_unavailable`）；
  * 只有一侧有 `body_hex` 且两侧都有 `body_sha256` ⇒ 用 sha 互验（`body_partial` 登记，不定位偏移）。
* `body_len` / `body_sha256` 与 `body_hex` 自相矛盾（驱动器产物自相矛盾）⇒ UNDECIDABLE
  （`input_inconsistent`），不替驱动器猜。

`<DIR>/run.json`（可选）：`argv`、起止墙钟、步骤与匹配结果。缺失只在报告里登记
（`inputs.<side>.run.present=false`），不影响帧判定；缺 `argv/started_at/ended_at`
会在 `missing_fields` 里列出。

`<DIR>/db_snapshot.json`（可选，见「库快照」）。

## 帧判定口径

1. **对齐按文件顺序**，不按 `seq` 建键。`action=ignore` 的消息号（默认心跳 10/41）在
   两侧**先各自剔除**，剩余序列逐位对齐；这样一侧多/少心跳不会把后续帧顶偏。
2. `ident` 永远比较（即使其余字段被掩码）；错位即 `ident` 差异。
3. 头部字段 `recog/param/tag/series` + 驱动器字段 `phase/seq/t_ms/conn`：
   **默认掩码** recog（会话 ActorId）、t_ms（墙钟）、conn（会话标签）、seq（本地序号）；
   `phase` 默认比较。所有掩码差异都进 `whitelisted`（带 reason/tier/approved_by），绝不静默。
4. body 默认逐字节比较；差异记录 `ident + frame_index + offset`、首处不同字节与不同字节数。

### 规则动作

| action | 语义 |
| --- | --- |
| `compare` | 头部按全局掩码 + 本条覆盖后比较；body 逐字节比 |
| `mask_fields` | `fields` 里的头部字段不比（如 46 的 param/tag = 墙钟小时位）；body 仍逐字节比 |
| `mask_body_ranges` | body 的 `[start,end)` 区间（`end:null` = 到末尾）不比；长度必须相等，否则 body_len 差异。区间内若确有不同，仍登记 `mask_body_ranges` |
| `ignore` | 该消息号在两侧对齐前剔除（帧数/时刻不稳的心跳）；`ident` 也因此不比 |
| `ability_layout` | 52 SM_ABILITY：`{40,58}` 布局对按 18 个语义字段逐项比（布局表从 `tools/loginfixture/capture_login_compare.py` 导入，**不抄第二份**）；同长度同布局退回逐字节；未知长度 ⇒ UNDECIDABLE |

每条 ident 规则还可带：

* `head_mask: [...]`：在本 ident 上额外掩码头部字段；
* `head_compare: [...]`：从全局掩码里**取回**字段比较（如 52 的 recog 是 Gold 不是 ActorId）。

## 规则表 `rules.json`

```json
{
  "format": "conformance-rules/1",
  "global": [
    {"fields": ["recog"], "action": "mask_fields",
     "reason": "...", "tier": "L", "approved_by": "PENDING owner/coordinator review"}
  ],
  "rules": [
    {"ident": 52, "action": "ability_layout", "head_compare": ["recog"],
     "reason": "...", "tier": "L", "approved_by": "PENDING owner/coordinator review"},
    {"ident": [10, 41], "action": "ignore",
     "reason": "...", "tier": "L", "approved_by": "PENDING owner/coordinator review"}
  ],
  "db": {"time_mode": "presence", "key_mode": "pk", "ignore_columns": [],
         "allow_unknown_table": false, "reason": "...", "tier": "L", "approved_by": "..."}
}
```

校验（任何一条不满足 ⇒ 退出码 2，不会带着错规则去比）：

* `format` 必须是 `conformance-rules/1`；
* 每条 `reason` / `tier`（`R`/`L`）/ `approved_by` 必填且非空；
* `ident` 是整数或非空整数数组，范围 0..65535；**同一 ident 出现两次 ⇒ 拒绝**（歧义）；
* `mask_fields.fields` 只能是 `recog/param/tag/series`；
* `mask_body_ranges.ranges` 是 `[start,end]`，`start>=0`，`end>start` 或 `null`；
* `global` 条目只能是 `mask_fields`，且字段不能重复声明；
* `db.time_mode ∈ {exact,presence,normalized}`、`db.key_mode ∈ {pk,multiset}`。

**怎么加规则**（不动代码）：

1. 在 `rules.json` 的 `rules` 里加 `{ident, action, reason, tier, approved_by}`；
2. 跑 `python -I -B tools/conformance/compare_runs.py selftest`（规则表本身必须仍能被解析）；
3. 跑真实产物，确认该 ident 从 `coverage.unclassified_idents` 消失；
4. 规则 = 判据，必须走评审；`approved_by` 保持 `PENDING ...` 时报告会带
   `rules.rules_pending_review=true`。

`--rules FILE` 可以整表替换（例如临时把某条掩码收紧做反例实验）。

## 什么算「不可判定」（退出码 2）

* 缺 `frames.ndjson`、目录不存在、JSON/十六进制坏行、`body_len`/`body_sha256` 自相矛盾；
* 某 ident 在任一帧里出现但规则表没有条目 ⇒ `UNCLASSIFIED`（除非 `--allow-unclassified` 点名）；
* 某头部字段只有一侧给了；body 两侧都缺；52 的 body 长度不在已知布局表；
* 依赖模块缺失/加载失败（`capture_login_compare.py` / `dbsnap_pair.py`）；
* 库快照：只有一侧有、缺 manifest、未知表、列不符、快照被事后改动、行数不符、重复主键；
* 规则表缺失/非法。

## 库快照

只读 JSON。`<DIR>/db_snapshot.json`（或同名目录）三种形态：

```jsonc
// a) 指向一个 dbsnap 快照目录（含 manifest.json + tsv）
{"snapshot_dir": "E:/tmp/snap-rust"}

// b) 裸 dbsnap manifest（file 相对本 json 所在目录）
{"tables": [{"table": "mir2_db.characters", "file": "mir2_db.characters.tsv",
             "sha256": "...", "rows": 1, "columns": [{"name": "Id"}, ...]}]}

// c) 内联（本工具转成临时 dbsnap 快照再比）
{"format": "dbsnap-inline/1",
 "tables": [{"table": "mir2_test_db.characters_ablity",
             "columns": ["PlayerId", "Level", ...],
             "rows": [["1", "7", ...]]}]}
```

比对口径**直接复用** `tools/db-parity/dbsnap_pair.py`（import 调 `run_pair`）：主键对齐、
逐表/逐行/逐列、NULL 口径、时间列口径、跨库前缀归一（`mir2_test_db_oracle.*` 与
`mir2_test_db_rust.*` 视为同一张表）、结构校验。

* 默认 `db.time_mode=presence`（两次独立运行的墙钟必然不同，时间列只比 NULL/非 NULL）；
  要逐时刻比用 `--db-time-mode exact|normalized`。
* 跨运行自增 Id 不同时用 `--db-key-mode multiset --db-ignore-columns Id,PlayerId`；
  `dbsnap_pair` 只允许忽略**已登记易变列/时间列/主键**，行为列（如 `MakeIndex`）不许静默忽略
  —— 传了会直接拒绝（退出码 2）。
* 两侧都没有库快照 ⇒ `db.state=absent_both`，退出码不受影响，但报告明确写「库后态未比较」；
  只有一侧有 ⇒ 2。
* 库结构问题（未知表/列不符/快照被改/行数不符/重复主键）⇒ 2；单元格/行差异 ⇒ 1。

## 自检

```sh
python -I -B tools/conformance/compare_runs.py selftest
```

全离线合成两次运行，33 条断言，覆盖：全等 ⇒ 0；改一个字节 ⇒ 1 且定位 ident+offset；
未覆盖 ident ⇒ 2（`--allow-unclassified` 放行）；缺规则表/坏规则/坏 JSON ⇒ 2；
52 的 40/58 语义比（一致 ⇒ 0、改 hp ⇒ 1）；头部掩码不吞 body 差异；心跳剔除后仍能定位真差异；
帧数不同；缺 body；产物自相矛盾；库逐列 diff、跨库前缀、单侧缺快照。

**牙**：每条「改坏必红」都断言具体退出码与定位字段/偏移；比较器若退化成「永远 0」，
自检会以 RED 收场。

## 当前规则覆盖（待真机首跑补全）

已覆盖 ident：`10, 41, 42, 46, 50, 51, 52, 54, 213, 658, 752, 766, 811, 1106, 5008, 11029`。

已知待复核/缺口：

* `10/41` 按任务口径当心跳 `ignore`，但协议表里 10=`SM_TURN`、41=`SM_FEATURECHANGED`；
  若真机首跑显示它们承载真实状态，必须改成 `compare`（或按 phase 区分心跳/真实帧）。
* 驱动器可能产出但尚未登记的：`201 SM_BAGITEMS`、`6/7/9 SM_RUSH*`、`9 SM_BACKSTEP`、
  以及任何登录后新增的系统消息 —— 首跑会以 UNCLASSIFIED（退出码 2）暴露，按上面步骤逐条加规则。
* `46/50/51` 只掩了已知的墙钟位；若真机上还有别的定时器字段，首跑会红，届时按证据扩规则。
* 全部规则 `approved_by` 均为 `PENDING owner/coordinator review`，尚未获批准。
