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
| `--ignore-idents IDENTS` | 显式剔除消息号（逗号分隔，可重复）；默认规则表**没有** ignore |
| `--allow-empty` | 两侧可比较帧数都是 0 时仍判（默认 `UNDECIDABLE(empty_run)`） |
| `--allow-missing-fields` | 两侧都缺同一头部字段时只登记不判死（默认 `UNDECIDABLE`） |
| `--require-db` | 要求两侧都有库快照（默认两侧都缺只登记 `absent_both`） |
| `--db-oracle PATH` / `--db-rust PATH` | 覆盖库快照位置（默认 `<side>/db_snapshot.json`） |
| `--db-time-mode exact\|presence\|normalized` | 覆盖规则表 `db.time_mode` |
| `--db-key-mode pk\|multiset` | 覆盖规则表 `db.key_mode` |
| `--db-ignore-columns A,B` | 覆盖规则表 `db.ignore_columns`（只允许已登记易变列/时间列/主键） |
| `--db-allow-unknown-table` | 放行未登记表（默认未知表 ⇒ 无法判定）；仅两侧表名相同才可比 |
| `--quiet` | 不打印终端摘要 |
| `selftest` | 离线自检（见下） |

## 退出码（唯一的判定口径）

| 码 | verdict | 含义 |
| --- | --- | --- |
| 0 | `PASS` | 帧流与库后态在规则范围内全等 |
| 1 | `FAIL` | 有真实差异（帧 body/头部、帧数、库单元格/行） |
| 2 | `UNDECIDABLE` | **判不了**：缺产物、解析失败、规则没覆盖（UNCLASSIFIED）、40/58 解不出、只一侧有库快照、库快照结构问题；以及空运行（`empty_run`）、两侧都缺同一头部字段、PENDING 的 ignore 真剔了帧 |

**同时出现差异与「判不了」时取 2。** 未完成的判据优先，绝不用「有差异」掩盖「没判完」；
也绝不在缺规则/缺数据时静默当绿。

## 一条命令跑完（`run_dual.sh`）

把「**恢复基线 → 起 A 栈 → 跑场景 → 抓库快照 → 停栈 → 起 B 栈 → 同样 → compare**」
固化成一条命令：

```sh
CONFORMANCE_PASSWORD=... bash tools/conformance/run_dual.sh   --scenario tools/probe/scenarios/login_only.json   --login-gate HOST:PORT --account '<账号>' --server-name '<服务器名>'   --stack-a-cmd '<起 C# 栈的命令>' --stop-a-cmd '<停 C# 栈的命令>'   --stack-b-cmd '<起 Rust 栈的命令>' --stop-b-cmd '<停 Rust 栈的命令>'   --restore-cmd '<从基线恢复库的命令>'   --db-a-cmd '<把 C# 侧库快照写到 $DUAL_RUN_DIR/db_snapshot.json 的命令>'   --db-b-cmd '<同上，Rust 侧>'   --out E:/tmp/conformance/runs/round2
```

| 参数 | 说明 |
| --- | --- |
| `--scenario FILE` | **必需**。驱动器场景 JSON（`tools/probe/scenarios/*.json`） |
| `--stack-a-cmd` / `--stack-b-cmd` | **必需**。起栈命令；必须**返回**（栈在后台起好后返回 0） |
| `--out DIR` | **必需**。产物目录（`a/`、`b/`、`report.json`、`run_dual.json`、`run_dual.log`） |
| `--login-gate HOST:PORT` / `--account` / `--password` | **必需**。驱动器连接参数；脚本**不写死端口/凭据**，密码建议走 `CONFORMANCE_PASSWORD` |
| `--server-name` / `--char-name` | 可选，透传给驱动器 |
| `--restore-cmd` | 可选。基线恢复；**每侧起栈前各跑一次** |
| `--stop-a-cmd` / `--stop-b-cmd` | 可选但强烈建议。停栈；**中途失败也会执行** |
| `--db-a-cmd` / `--db-b-cmd` | 可选。抓库快照；命令里用 `$DUAL_RUN_DIR` 定位本侧产物目录 |
| `--probe-cmd` | 可选。驱动器命令前缀（默认仓库内 `tools/probe/mir2_probe.py`） |
| `--probe-arg ARG` | 可重复，追加给驱动器 |
| `--rules FILE` | 可选，默认本目录 `rules.json` |
| `--allow-missing-fields` / `--require-db` / `--allow-empty` / `--quiet` | 透传给 `compare_runs.py` |

* 每步命令都拿到导出的环境：`DUAL_SIDE`(`a`/`b`)、`DUAL_OUT`、`DUAL_RUN_DIR`、`DUAL_SCENARIO`。
* **缺参 / 某步失败 / compare UNDECIDABLE ⇒ 退出码 2**；有真实差异 ⇒ 1；PASS ⇒ 0。
  绝不出现「A 侧没跑成却因为别的原因判绿」。
* A 侧没跑完就**不再起 B 侧**（两栈同端口，不能并存）。
* 自检（离线，桩命令，不连网/不连库/不起真栈）：
  `bash tools/conformance/run_dual.sh --selftest`（15 条断言：全流程 PASS、b 侧差一字节 ⇒ 1、
  缺参 ⇒ 2、起栈失败 ⇒ 2 且不跑场景、场景失败仍停栈）。

## 输入：驱动器产物约定

`<DIR>/frames.ndjson`（必需，一行一帧 s2c）：

```json
{"seq":1,"t_ms":1000,"phase":"login","conn":"bot0","ident":50,
 "recog":123,"param":10,"tag":20,"series":3,
 "body_len":0,"body_hex":"","body_sha256":"e3b0c442..."}
```

* `ident` 必需（u16）；`body_hex` 是 body 字节的十六进制（可带 `0x`）。**本仓库驱动器（`tools/probe/mir2_probe.py`）落的是线缆字节**（EDCode 编码后、已去掉 CommandMessage 头）。
  比较器对一般 ident 直接按字节比（EDCode 是确定性的：线缆逐字节相等 ⇔ 明文相等）；
  只有 52 需要先按已知线缆长度做 EDCode 归一，见下「52 的布局对」。
* 其余字段缺失会被**显式登记**；默认不是「可容忍」而是判不了：
  * 两侧都缺某头部字段 ⇒ 默认 `UNDECIDABLE(missing_field_both_sides)`；
    `--allow-missing-fields` 才降级为 `missing_fields` + `allowed_missing_fields` 登记后放行；
  * 只有一侧缺某头部字段（**即使该字段被掩码**）⇒ 永远 UNDECIDABLE（`missing_field`）；
  * 两侧都没 `body_hex`/`body_sha256` ⇒ UNDECIDABLE（`body_unavailable`）；
  * 只有一侧有 `body_hex` 且两侧都有 `body_sha256` ⇒ 用 sha 互验（`body_partial` 登记，不定位偏移）。
* `body_len` / `body_sha256` 与 `body_hex` 自相矛盾（驱动器产物自相矛盾）⇒ UNDECIDABLE
  （`input_inconsistent`），不替驱动器猜。

`<DIR>/run.json`（可选）：`argv`、起止墙钟、步骤与匹配结果。缺失只在报告里登记
（`inputs.<side>.run.present=false`），不影响帧判定；缺 `argv/started_at/ended_at`
会在 `missing_fields` 里列出。

`<DIR>/db_snapshot.json`（可选，见「库快照」）。

## 帧判定口径

1. **对齐按文件顺序**，不按 `seq` 建键。默认**没有** ignore 规则：`action=ignore` 只能来自
   规则表或显式 `--ignore-idents`，被剔除的消息号在两侧**先各自剔除**，剩余序列逐位对齐。
   10=`SM_TURN` / 41=`SM_FEATURECHANGED` 是真实协议帧（`Messages.cs:161/202`，downlink 有逐字节
   黄金断言），默认 `compare` 逐字节比。规则表里 `approved_by` 仍是 `PENDING` 的 `ignore`
   若真的剔了帧 ⇒ `UNDECIDABLE(pending_ignore_rule)`，不许直接判绿。
   两侧剔除后可比较帧数都是 0 ⇒ `UNDECIDABLE(empty_run)`，只有 `--allow-empty` 放行。
2. `ident` 永远比较（即使其余字段被掩码）；错位即 `ident` 差异。
3. 头部字段 `recog/param/tag/series` + 驱动器字段 `phase/seq/t_ms/conn`：
   **默认掩码** recog（会话 ActorId）、t_ms（墙钟）、conn（会话标签）、seq（本地序号）；
   `phase` 默认比较。所有掩码差异都进 `whitelisted`（带 reason/tier/approved_by），绝不静默。
4. body 默认逐字节比较；差异记录 `ident + frame_index + offset`、首处不同字节与不同字节数。
5. `multiset_by_tick` 的 ident：**同 ident 连续段**（被别的 ident 隔开不合并）内的帧按
   `(param,tag,series,body_hex,phase)` 排序后逐项比。顺序不同但集合相同 ⇒ rc 0，并在报告顶层
   `masked_order` 里逐组登记两侧原始顺序（不是静默 PASS）；集合不同 ⇒ rc 1，逐条给出缺失/
   多余的具体帧（`ident + param/tag/series/phase + body 前 16 字节 + body_sha256`）。
   **`t_ms` 不是判定边界**：它是全局掩码的墙钟字段，按它硬切块会把「两侧帧集完全相同、只是
   差 1ms」误判成缺帧（reviewer-2 I-3）；`tick_tolerance_ms` 只用于在 `masked_order` 里标注
   两侧 tick 窗口（`oracle_windows`/`rust_windows` + `tick_split`）。任一侧任一帧缺 `t_ms`
   ⇒ rc 2（`multiset_tick_missing`，点名 side/frame_index/seq）。

### 规则动作

| action | 语义 |
| --- | --- |
| `compare` | 头部按全局掩码 + 本条覆盖后比较；body 逐字节比 |
| `mask_fields` | `fields` 里的头部字段不比（如 46 的 param/tag = 墙钟小时位）；body 仍逐字节比 |
| `mask_body_ranges` | body 的 `[start,end)` 区间（`end:null` = 到末尾）不比；长度必须相等，否则 body_len 差异。区间内若确有不同，仍登记 `mask_body_ranges` |
| `multiset_by_tick` | **同 ident 连续段**（被别的 ident 隔开不合并）内的帧按 `(param,tag,series,body_hex,phase)` 多重集比（排序后逐项）。顺序不同但集合相同 ⇒ rc 0 并登记 `masked_order`；集合不同 ⇒ rc 1 并给出缺失/多余的具体帧。`tick_tolerance_ms` 只用于标注 tick 窗口（`tick_split`），`t_ms` 不作判定边界；任一帧缺 `t_ms` ⇒ rc 2。**只允许用于无因果序的帧**（当前仅 ident 100） |
| `ignore` | 该消息号在两侧对齐前剔除；`ident` 也因此不比。默认规则表**没有** ignore；PENDING 复核的 ignore 真剔了帧 ⇒ UNDECIDABLE |
| `ability_layout` | 52 SM_ABILITY：`{40,58}` 明文布局对按 18 个语义字段逐项比（布局表从 `tools/loginfixture/capture_login_compare.py` 导入，**不抄第二份**）；线缆字节先按 `54→40 / 78→58` 归一（EDCode，见下）；同长度同布局退回逐字节（归一过的在明文空间比）；未登记长度对 ⇒ UNDECIDABLE |

每条 ident 规则还可带：

* `head_mask: [...]`：在本 ident 上额外掩码头部字段；
* `head_compare: [...]`：从全局掩码里**取回**字段比较（如 52 的 recog 是 Gold 不是 ActorId）。

### 52 的布局对（明文 40/58，线缆 54/78）

`SM_ABILITY(52)` 的 body 长度**不是常量**，它随客户端版本/ability 位集（如客户端编译期
`HIGHHP`）变化。因此规则表登记的是**布局对**，不是单一长度：

| 侧 | 明文（布局表描述的结构） | 线缆（EDCode 编码后 = 驱动器 `body_hex`） | 来源 |
| --- | --- | --- | --- |
| C# oracle | **40B** | **54B** | `Ability.cs` 声明序 + `EncodeMessage(WAbil)`；`encoded_len(40)=54` |
| 冻结客户端 | **58B** | **78B** | `HIGHHP=True` 的 `TAbility`（`record.rs::encode_ability_client`；`client-e2e-enter-gaps-20261013/21-client-ability-layout.md`）；`encoded_len(58)=78` |

`encoded_len` 为 `(n//3)*4 + {0:0,1:2,2:3}[n%3]`（`capture_login_compare.py`）。
首次双栈真机对拍实测的正是 (54, 78)：EDCode 归一成 40/58 后 18 个语义字段全等
（`docs/conformance-first-dual-run-20261023.md` §2.3）。

判据：

* **按场景登记真实长度对**，不写死 40/58；新增对 = 在 `compare_runs.py::ABILITY_WIRE_LENS`
  加 `线缆长度 → 明文长度`，且明文布局必须已存在于 `ABILITY_LAYOUTS`（两者成对）。
* **新增对必须附来源**：C#/Rust 的源码锚（谁按什么顺序写的哪些字段）+ 实测样本（帧
  `body_hex`/sha256 或日志），写进本 README 并走评审。没有来源的长度对一律 **不登记**——
  遇到就是 `UNDECIDABLE(ability_layout_unknown)`（退出码 2），绝不猜偏移。
* 只登记「同一份 `WAbil` 换布局」的对；语义字段集合（18 个）不同就是另一条路径，另开卡。

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
    {"ident": [10, 41], "action": "compare",
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
* `multiset_by_tick.tick_tolerance_ms` 必须是非负整数（默认 0）；该字段**只允许**出现在
  `multiset_by_tick` 规则上，出现在别的动作 ⇒ 拒绝（退出码 2，fail-closed）；
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
* 某 ident 在任一帧里出现但规则表没有条目 ⇒ `UNCLASSIFIED`（除非 `--allow-unclassified` 点名；
  `--ignore-idents` 点名的 ident 视为已分类：剔除即处置）；
* 某头部字段只有一侧给了（含被掩码字段）；两侧都缺同一头部字段（默认）；body 两侧都缺；
  52 的 body 长度不在已知布局表；
* 两侧可比较帧数都是 0（`empty_run`，除非 `--allow-empty`）；
* 规则表里 PENDING 的 `ignore` 真的剔了帧（`pending_ignore_rule`）；
* 依赖模块缺失/加载失败（`capture_login_compare.py` / `dbsnap_pair.py`）；
* 库快照：只有一侧有、缺 manifest、未知表、列不符、快照被事后改动、行数不符、重复主键；
* 规则表缺失/非法。

## 偶发帧对的复跑口径（N≥3，禁止「重跑到绿」）

双栈对拍里**偶发**多发/少发帧对（例如 C# 偶尔多出一对 `SM_TURN(10)+SM_FEATURECHANGED(41)`、
Rust 偶尔多出 `SM_HIT(14)`）不能用「再跑一次直到 PASS」当判定。固定口径：

1. **同一场景、同一基线、同一规则表**，每侧连续跑 N≥3 次；两侧各自独立跑 N 次，
   **不得只重跑失败的那一侧**。
2. 每次运行都保留产物（`frames.ndjson` + `run.json` + 库快照），路径带序号，例如
   `E:/tmp/conformance/runs/<scenario>/<side>-r1..rN/`。
3. 对每侧、每次运行输出 **ident 直方图**（ident → 帧数），N 份都要贴进报告/卡片，
   不能只写 PASS/FAIL。可直接用：
   ```sh
   python -I -B -c "import json,collections,sys; c=collections.Counter(); [c.update([json.loads(l)['ident']]) for l in open(sys.argv[1],encoding='utf-8') if l.strip()]; print(dict(sorted(c.items())))" <side>/frames.ndjson
   ```
   **证据落盘格式（固定文件名，便于复审逐项重放）**：
   * 每个 run 目录写 `hist.json`：
     `{"side":"oracle","run":"r1","ident_counts":{"50":1,"100":4},"frame_order":[50,51,...,100]}`；
   * 场景目录写 `matrix.json`：
     `{"runs":{"oracle":["r1","r2","r3"],"rust":["r1","r2","r3"]},"matrix":[{"oracle":"r1","rust":"r2","exit_code":0,"verdict":"PASS"},...]}`。
   直方图、帧序、配对矩阵三者缺一不算跑完。
4. **取运行与配对范围**：只在「主帧序列相同」的运行之间比（主帧序列 = 该场景各步骤应出现的
   不同 ident 序列；顺序抖动只允许发生在已登记 `multiset_by_tick` 的 ident 上）。配对范围是
   两侧 N 次运行的**全交叉 N×N**（不是只取第一对/最后一对代表）：矩阵里每个格子的
   PASS/FAIL/UNDECIDABLE 与退出码都要贴进 `matrix.json`。N 次里若某侧出现 ≥2 种主帧序列，
   先判该侧不稳定并按第 5 条处置，**不得挑一次好看的当结果**。
5. **判定**：
   * 某侧**稳定**多发/少发某一对帧（N 次里该 ident 计数始终多/少）⇒ **判缺陷**；确要用规则表登记，
     必须给 reason/tier/approved_by 与实测证据（`rules.json` 走评审），不得用 `--ignore-idents` 临时绕过；
   * 某侧**偶发**多发/少发（N 次里计数不一致）⇒ 记录两侧 N 次直方图 + 每种的帧序，按 **R 级**处理
     （时序/随机源），并在卡片线程留 `comment`；
   * 只有**两侧 N 次 ident 直方图一致、且每个已登记多重集 ident 的 body 多重集相同**，
     才算该场景的帧集稳定。
6. **禁止**：把「重跑到绿」写成通过、只贴最后一次绿的报告、用 `--ignore-idents` 掩盖稳定多发帧。
   已登记的 `multiset_by_tick`（当前只有 ident 100）只放宽**同 ident 连续段内**的先后顺序
   （被别的 ident 隔开的段不合并），报告逐组登记在 `masked_order`；它**不会**让多发/少发帧
   变成 PASS，也不会跨过别的 ident 重排。

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
* 两侧都没有库快照 ⇒ `db.state=absent_both`，默认退出码不受影响，但报告明确写「库后态未比较」；
  要求库后态时用 `--require-db` ⇒ 2（`db_snapshot_absent`）；只有一侧有 ⇒ 2。
* `--db-allow-unknown-table` 只放行**两侧表名相同**的未登记表（按 multiset 比）；未登记表在
  两侧库前缀不同时是两个不同的表名 ⇒ `missing_table` ⇒ 2。要跨前缀比未登记表，先在
  `dbsnap_pair.TABLE_SPECS` 登记（走评审）。
* 内联快照（`dbsnap-inline/1`）的单元格一律按字符串落 tsv：JSON `null` 与字符串 `"NULL"`
  都落成同一个 NULL token，无法区分。要区分「真 NULL」与字面 `"NULL"` 请改用 dbsnap 快照目录。
* 库结构问题（未知表/列不符/快照被改/行数不符/重复主键）⇒ 2；单元格/行差异 ⇒ 1。

## 自检

```sh
python -I -B tools/conformance/compare_runs.py selftest
```

全离线合成两次运行，**113 条断言**，覆盖：全等 ⇒ 0；改一个字节 ⇒ 1 且定位 ident+offset；
未覆盖 ident ⇒ 2（`--allow-unclassified` 放行）；缺规则表/坏规则/坏 JSON ⇒ 2；
52 的 40/58 语义比（一致 ⇒ 0、改 hp ⇒ 1）；头部掩码不吞 body 差异；显式剔除后仍能定位真差异；
10/41 默认参与比较、只有 ident=14（RNG 支）是默认 ignore 且 PENDING 剔帧 ⇒ 2；`--ignore-idents` 可剔除未登记 ident；空运行、两侧都缺头部字段、被掩码字段单侧缺失 ⇒ 2；
块数不同（`block_align`；两侧帧数相同也不谎报帧数差）；缺 body；产物自相矛盾；
库逐列 diff、跨库前缀、单侧缺快照、`--require-db`。

首跑补规则后新增的断言（防止「加了规则却不判」）：

* 7 条新 ident（100/520/525/529/530/654/657）逐条：规则存在且 `action` 符合预期
  （530 = `mask_body_ranges`，其余 = `compare`）、
  带 `reason/tier/approved_by=PENDING`；两侧相同 ⇒ 0；**改一个字节 ⇒ 1 且定位到该 ident+offset**；
* 520 的 `recog`（=nChrCount）不同 ⇒ 1（`head_compare` 真的取回了比较）；
  530 的 `recog` 不同 ⇒ 0 且登记 `masked_head`（会话/游戏时长，故意掩）；
  530 的 body 末段（SessionID，偏移 16..end）不同 ⇒ 0 且登记 `mask_body_ranges`；
  530 的 body 端口字节改一位 ⇒ 1（掩码不得吞掉 ip/端口）；
* 52 **线缆 (54,78)**：归一后语义全等 ⇒ 0 且登记 `ability_layout`（含 `wire_normalized`
  与 `oracle_wire_len=54 / rust_wire_len=78`）；线缆形态改 hp ⇒ 1（`space=plain`）；
  两侧同为 58B 线缆 ⇒ 明文逐字节比、改一字节 ⇒ 1（偏移 46）；
* 52 未登记长度对（41,41）⇒ 2（`ability_layout_unknown`，fail-closed）。
* `multiset_by_tick`（对拍降噪 v2）：内置表**只有 ident=100** 用它（不扩散）且
  `tick_tolerance_ms=0`；两条 100 顺序颠倒 ⇒ 0 且 `masked_order` 登记；
  少一条 100 ⇒ 1 且缺失帧带 param/series/body 前 16 字节；三条 100 轮换（集合相同）⇒ 0；
  多塞一条 41 ⇒ 1（机制不吞非多重集 ident）；一条 body 改掉 ⇒ 1 且缺失+多余各一条。
  **reviewer-2 复审补的牙**：
  * I-1 `[100A,42,100B]` vs `[100B,42,100A]` ⇒ 1 且 `masked_order` 为空（绝不跨别的 ident 合并）；
    对照同序 ⇒ 0；
  * I-2 运行中的**后续帧**缺 `t_ms` ⇒ 2，并点名 `side`/`frame_index`/`seq`；整侧缺 ⇒ 2；
  * I-3 同帧集 + 1ms 抖动（`t=1000/1001`）+ 顺序不同 ⇒ 0（不误报缺帧），`masked_order` 里
    `tick_split=true`、两侧窗口 1 vs 2；容差 5ms ⇒ `tick_split=false`；同序 + 1ms 抖动 ⇒ 0 无 diff；
  * I-3① 块数不同但帧数相同 ⇒ `block_align`（`oracle_frames==rust_frames==2`），绝不输出
    「帧数 2 vs 2 不同」；
  * M-1 重复 body、phase 不同、顺序互换 ⇒ 0 + `masked_order`（phase 已进键）；phase 真不同 ⇒ 1；
  * `tick_tolerance_ms<0` 或用在非 multiset 动作 ⇒ 2。

**牙**：每条「改坏必红」都断言具体退出码与定位字段/偏移；比较器若退化成「永远 0」，
自检会以 RED 收场。

## 当前规则覆盖

已覆盖 ident：`10, 14, 41, 42, 46, 50, 51, 52, 54, 100, 213, 520, 525, 529, 530, 654,
657, 658, 752, 766, 811, 1106, 5008, 11029`。

**跨进程不可复现的两条**（2026-10-23 双栈窗口登记，见 `rules.json` 的 reason）：

* `14`（固定 NPC 受击帧）——oracle 只在 `Random(30)==0` / `Random(50)==0` 命中时发，
  随机源是墙钟播种的 `new Random()`（`src/OpenMir2/RandomNumber.cs:35`）⇒ 用 `ignore`；
  帧形态与收件人由 `downlink::hit_frame` 黄金用例钉死，不靠本规则。
* `10`（SM_TURN）的 `series` = `MakeWord(Dir, Light)` **整个 word**（对所有 actor 生效），固定 NPC 的
  `Dir` 由随机支 `TurnTo(RandomByte(8))` 改写 ⇒ 掩 `series`，body/param/tag 仍逐字节比。
  代价：玩家「原地转身」的 Dir 目前**没有任何帧级判据**（`SM_WALK(11)`/`SM_RUN(13)` 尚未登记规则，出现即 UNCLASSIFIED；
  要兜底须先登记它们，另开卡）。
* `530` 的 `mask_body_ranges` 偏移是 **EDCode 线缆空间**不是明文空间（见 rules.json 的映射表）；
  selftest 必须用 `lc.encode` 造线缆字节，否则「掩错位置还全绿」这类缺陷咬不到。

首跑缺口补齐的 7 条（每条都给 reason + tier + approved_by=PENDING，全走评审）：

| ident | 消息 | action | 判据（细节见 `rules.json` 的 reason） |
| --- | --- | --- | --- |
| 100 | `SM_SYSMESSAGE` | multiset_by_tick | body 是 GBK 静态文案（首跑 4 条实测无墙钟/时间戳）。**同 ident 连续段**内的多条 100 按 `(param,tag,series,body_hex,phase)` 多重集比：C# 的 actor 发件队列是 `PriorityQueue<SendMessage,byte>`、入队优先级 `(byte)wIdent`（`BaseObject.Message.cs:94`），同优先级出队顺序未定义 ⇒ 顺序跨进程不稳。`t_ms` 是全局掩码的墙钟字段，**不作判定边界**（只标注 `tick_split`）；被别的 ident 隔开的段绝不合并；集合不同仍 rc 1 并给出缺失/多余帧。**只给它，不扩散到 10/41/14** |
| 520 | `SM_QUERYCHR` | compare + `head_compare:[recog]` | body=角色列表文本；recog=nChrCount（协议内容）⇒ 从全局掩码取回比较 |
| 525 | `SM_STARTPLAY` | compare | body=GameGate ip/port（同端口方案下必须逐字节相同） |
| 529 | `SM_PASSOK_SELECTSERVER` | compare | body=服务器名/状态；**保留**首跑真差异（`Idle` vs `0`）继续可定位，不掩码 |
| 530 | `SM_SELECTSERVER_OK` | compare | body=SelGate ip/port/session；recog=游戏时长或 SessionID ⇒ 全局掩码 |
| 654 | `SM_CHANGELIGHT` | compare | header-only；recog=ActorId 掩码；param=Light / tag=nClientKey 逐值比 |
| 657 | `SM_CHARSTATUSCHANGED` | compare | header-only；recog=ActorId 掩码；param/tag=状态位、series=HitSpeed 逐值比 |

已知待复核/缺口：

* `10/41` 的默认动作已按评审（review oid d280be71 CH-2）改为 `compare`：10=`SM_TURN`、
  41=`SM_FEATURECHANGED` 是真实协议帧。确需按 phase 区分心跳时用显式 `--ignore-idents`，
  或新增规则并走评审。
* 驱动器可能产出但尚未登记的：`201 SM_BAGITEMS`、`6/7/9 SM_RUSH*`、`9 SM_BACKSTEP`、
  以及任何登录后新增的系统消息 —— 会以 UNCLASSIFIED（退出码 2）暴露，按上面步骤逐条加规则。
* `46/50/51` 只掩了已知的墙钟位；若真机上还有别的定时器字段，会红，届时按证据扩规则。
* **`654` 的 `param=Light`** 由装备光源/游戏内昼夜决定（C# `PlayObject.Message.cs:1840`），
  **`657` 的 `series=HitSpeed`** 由属性重算派生（同文件 `:1846`）。两者本轮按 `compare` 登记；
  若下一轮真机出现差异，按 `46/50/51` 的样式登记**窄掩码**（`head_mask` 逐个字段 + 依据），
  不得整帧 ignore。
* `529` 的 `Idle` vs `0`、以及「C# 多出 10/100/654/657」是**真实行为差异**
  （`docs/conformance-first-dual-run-20261023.md` §2.1/§2.2），不是规则缺口：规则补齐后重跑，
  它们仍会以 `body_len` / `block_align` / `multiset_missing`/`multiset_extra` 等差异出现，直到服务端行为被另卡修好。
* 全部规则 `approved_by` 均为 `PENDING owner/coordinator review`，尚未获批准。

## `card_window.py`（窗口批处理）的 fail-closed 口径

一条命令跑完双栈窗口（用法见 `docs/leader-queue-20261023.md` §5）；`run_dual.sh` 仍是底层
语义参照，本工具只调用 `mir2_probe.py` / `compare_runs.py` / `dbsnap.py`，不改它们。

* **参数发现（卡 `openmir2-window-discovery-wiring`）**：场景声明了 `required` 时**自动**
  先跑 discovery（默认 `npc_click_auto`：点一次 NPC 收 `SM_MERCHANTSAY(643)`），从**本侧**
  帧推导 required，再把本侧的 `--var` 传给 `mir2_probe`（`--discover` 可强制开启）。任一侧
  推不出 => 该侧 rc=2 并**点名缺哪个变量**（不跑场景、不抓库）；两侧 `kind` 不一致
  （`derived` vs `rule`）或非会话变量（坐标等）取值不同 => rc=2。**绝不拿另一侧的值、
  绝不用默认值/空值填空**。每侧 `<out>/<side>/discovered.json`、汇总 `<out>/discovered.json`
  （`format=param-discovery/1` / `discovered/1`，含值、推导来源、缺失清单、两侧一致性）。
  `run_dual.sh` 不改（0 字节 diff），接线只在 `card_window.py`。
* **`binaries` 是必需项**：两侧各至少一条非空路径，加载期校验；空列表/缺省直接
  ConfigError（否则会出现「PASS 报告里没有任何二进制 sha256」）。二进制文件缺失时提前
  收尾，但**产物集仍然完整**：`report.json` + `README.txt` + `card-window.json` +
  `card-window.log`，且 `verdict=UNDECIDABLE` / `exit=2` / `report.json.reason=missing_binaries`。
  **本工具明确选择「完整产物集」而不是「fail-closed 不写部分产物」**——窗口证据的价值在于
  事后谁都能复查，写半套反而让复查者以为产物丢了。
* **无法判定为测试库的库名是永久拒绝**：在 `db.databases` 里再声明一次也不放行，
  `--allow-production-db` 与 `--dry-run` 同样不放行；补救办法只有一个——把库名换成
  `preset=test` 或 `mir2_test_*` 一类。旧提示让人「在 db.databases 里显式声明」，但照做
  之后仍被拒（review F-1），该提示已改为「永久拒绝 + 请换库」。
* **停栈失败挂在调用点**：起过栈而停栈没跑成（`stop_rc=None`）或 `rc!=0` 一律判本侧失败
  （rc=2）；不只是 `stop_ok()` 辅助函数有判据，`run_side` 的 finally 调用点也有集成断言
  （monkeypatch `Runner.run` 让 stop 返回 `None`/非 0）。
* `--selftest` 离线覆盖上述每条，不连网、不连库、不起真栈、不占 15xxx/17xxx。
* **先定稿 verdict/exit_code，再写产物**（H-1）：`write_artifacts()` 在 README.txt 写失败时
  置 `exit=2`，这段判定必须发生在 `0 if ok else 2` **之后**，否则 PASS 运行会「缺 README
  却报绿」。`write_summary_json()` 写失败同样返回 False 并让调用方置 `exit=2`——两件产物
  都在卡面承诺内，不允许静默吞掉。
* **旧证据归档唯一化**（H-2）：同一 `--out` 反复使用会把上一次产物整体挪进
  `_stale-<ts>-<pid>[-n]/`；selftest 连跑三次，断言两个 `_stale-*` **各自独立**（第一次的
  PASS 存档不会被后一次覆盖）。
* **`--post-comment` 只记「证据包完整」的 PASS**（N-1）：comment 在 report.json /
  README.txt / card-window.json **全部写完之后**才发。顶层只有一个**三态** `verdict`
  （D-1）：compare FAIL ⇒ `FAIL`；无结论或证据缺件 ⇒ `UNDECIDABLE`；完整且 PASS 才 `PASS`。
  同时带 `verdict_reason`、`blockers`（如 `evidence_incomplete: README.txt write failed`）、
  `exit_code`、`report_kind`、`finalize_ok`；compare 的事实（verdict/rc/frames/db_state/
  diffs/undecidable/whitelisted）收进**嵌套 `compare`**，顶层不再有 `report_verdict`——
  只扫顶层 PASS 的读者/正则不会误读成「窗口通过」。comment 之后对 card-window.json 只再
  追加一条回执（不新增证据），回执写失败同样 `exit=2`。**这是 R 卡证据账本，绝不允许缺件
  时留下 PASS。**

### 牙（回归判据的可复跑 harness）

`tools/conformance/selftest_mutations.py` 是上面每条 fail-closed 口径的**变异牙**：
每颗牙对 `card_window.py` 做一处（或几处）精确替换，要求

1. `--selftest` 退出码非 0（子进程带 timeout，超时按失败 `rc=124`，不会走成绿）；
2. stdout 至少一条 `FAIL `；
3. 这批 FAIL 里**至少一条命中该牙的 `expect` 子串**——锁住「FAIL 落在该牙的判据组」，
   防止某颗牙只是被别的兜底断言顺带抓到（N-2；E3/E5 曾只被「全流程 => 退出码 0」兜住）；
4. 输出无 `Traceback`（干净 RED，不是异常噪声）；
5. 还原后 sha256 == 变异前；**不一致 => 整体 rc≠0**（N-3）。

```sh
python -I -B tools/conformance/selftest_mutations.py            # 跑全部 30 颗
python -I -B tools/conformance/selftest_mutations.py --list     # 只列牙（含每颗的 expect）
python -I -B tools/conformance/selftest_mutations.py --only T1,E2
python -I -B tools/conformance/selftest_mutations.py --selfcheck  # harness 自身 fail-closed
```

覆盖：T1 F-2 调用点回退 / T2 F-5 run_id 常量 / T3 F-3 空 binaries / T4 F-4 半套产物 /
T5 F-1 旧提示 / T6 H-1 写产物提前 / T7 H-2 固定 `_stale-fixed` / T8 N-1 comment 判定
看不到 README 写失败（等价于先写账本再发 comment）/ T9 N-1 comment verdict 固定 PASS /
T10 Y1 `finalize_ok` 丢掉 `json_ok` 那一半 / T11 D-1 证据缺件不降级 / T12 D-1 compare FAIL
被压成 UNDECIDABLE / T13 M-1 FAIL 分支 `verdict_reason` 写成 ok / T14–T16 M-2 嵌套 `compare`
分别删掉 whitelisted / db_state / undecidable（字段集合断言）/ T17–T20 **P-1 诊断值**变异
（db_state 写错、diffs 与 db_state 互换、diffs 固定 999、undecidable 固定 -1，断言一律从
本次 report.json 实读）/ T21 F-1 side doc `ok` 的两处兜底都丢掉（discovery probe 失败仍 ok=True）；额外牙 E1 fallback run_id
常量、E2 放宽未知库判定、E3 不落 card-window.json、E4 口令抹除失效、E5 不写 README、
E6 忽略 card-window.json 失败标志、E7 comment body 不带 exit_code；发现接线牙 D1 probe
命令丢掉 `--var`（值推出来了却没传给场景）、D2 发现失败时静默用空值继续跑。
