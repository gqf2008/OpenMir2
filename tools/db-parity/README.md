# tools/db-parity —— LB-10 持久化面字段级证据工具

对应审计 `mir2-rs/tests/parity/m3/audit-dbsrv-persistence-20261020.md` 的 **LB-10**
（9 表 + `characters_indexes` 的跨实现逐字段 diff、`NEWCHR`/`DELCHR`/`SELCHR` 库后态、
错误路径故障注入），设计依据 `design-dbsrv-connection-20261021.md` §6（C1..C8 / M1..M8 /
W-DB-1..7）。本目录**只放工具**，不改业务实现，不跑真机。

## 文件

| 文件 | 作用 |
|---|---|
| `dbsnap_pair.py` | 复用 dbsnap 快照格式，按主键对齐做**逐表/逐行/逐列** diff，输出 JSONL + 摘要；含 C3 `changed` 标志模式与离线 `selftest` |
| `newchr_delchr_selchr_probe.py` | 建角/删角/选角**库后态**采集骨架（`plan`/`run`/`assert`/`compare`）+ 离线 `selftest` |
| `README.md` | 本文件（用法、口径、未做项） |

两者都**不依赖 Python MySQL 驱动**：真机路径只调用 `tools/dbsnap/dbsnap.py`（只读快照）
与调用方提供的驱动命令；`selftest`/`assert`/`plan`/`compare` 全离线。

## 一键自检（不连库、不起任何进程）

```sh
python tools/db-parity/dbsnap_pair.py selftest
python tools/db-parity/newchr_delchr_selchr_probe.py selftest
python tools/db-parity/dbsnap_pair.py check-spec   # 与 scoped_diff.py 表登记对账
```

预期：`GREEN: dbsnap_pair selftest 30/30`、`GREEN: newchr_delchr_selchr_probe
selftest 25/25`、`GREEN: 表登记一致`。退出码 0=绿，1=红，2=用法/输入错误。

## 1. 两份快照的字段级 diff（W-DB-1 / C3）

先用探针的 `snap` 对两份同基线库拍快照（窗口卡执行；本车道不跑）。它底层调用
`tools/dbsnap/dbsnap.py`，会把表名重定到 `--db`、并自动加 `--allow-db`：

```sh
python tools/db-parity/newchr_delchr_selchr_probe.py snap --db mir2_test_db_oracle --out E:/tmp/w1/db-oracle
python tools/db-parity/newchr_delchr_selchr_probe.py snap --db mir2_test_db_rust --out E:/tmp/w1/db-rust
```

> 注意：`dbsnap.py` 的 `--db` 只认**预设/已知库名**（`test|prod|mir2_db|mir2_account|
> mir2_test_db|...`）；自定义克隆名（如 `mir2_test_db_oracle`）直接传给 dbsnap 会因
> `DB_ROLE` 未登记而报错。此时要么用上面的探针 `snap`（推荐），要么逐张
> `--table mir2_test_db_oracle.characters ... --allow-db mir2_test_db_oracle`。
> 只想拍 LB-10 十表时，探针默认就是这十张（`--tables` 可改）。

再逐字段比：

```sh
python tools/db-parity/dbsnap_pair.py diff \
  E:/tmp/w1/db-oracle E:/tmp/w1/db-rust \
  --scope lb10 \
  --require-tables mir2_db.characters,mir2_db.characters_indexes,mir2_db.characters_ablity,mir2_db.characters_item,mir2_db.characters_bagitem,mir2_db.characters_storageitem,mir2_db.characters_bonusability,mir2_db.characters_status,mir2_db.characters_item_attr,mir2_db.characters_magic \
  --jsonl E:/tmp/w1/diff.jsonl --summary-out E:/tmp/w1/diff.json
```

`--scope lb10` 只比 LB-10 的十张表；`--require-tables` 不只是存在性检查：**点名的表会
进入比较集**（即使被 `--tables` 收窄也不会被跳过），缺表直接 RED，避免「悄悄不比」。
任何一条 JSONL 记录 ⇒ `request_changes`，不得用「语义等价」放过（设计 §6.5）。

**库前缀归一（E1 克隆必需）**：E1 裁决的两份库是 `mir2_test_db_oracle` /
`mir2_test_db_rust`，快照 manifest 里的表名带各自前缀。本工具把
`<任意库前缀>.<表名>` 归一到登记的 `mir2_db.*` / `mir2_account.*` 后再对齐，
因此两侧只差前缀也能逐列比较；未登记表仍按 `unknown_table` 处理。

### C3 changed 标志（时间列不得只比「变化了」）

四份快照：A/B 两侧各有 before（run 前基线）与 after（run 后）：

```sh
python tools/db-parity/dbsnap_pair.py diff \
  E:/tmp/w1/after-oracle E:/tmp/w1/after-rust \
  --scope lb10 \
  --baseline-a E:/tmp/w1/before-oracle --baseline-b E:/tmp/w1/before-rust \
  --time-mode normalized \
  --jsonl E:/tmp/w1/diff.jsonl --summary-out E:/tmp/w1/diff.json
```

机判三类记录：

- `value` / `row_only_a` / `row_only_b`：逐列或行集不一致；
- `changed_mismatch`：一侧 `after != before`、另一侧没变（或反向）；
- `time_not_later`：changed 的时间列 `after` **必须严格晚于** `before`；`after <= before`
  （含相等）即红。时间列支持 DATETIME 文本与纯整数 unix 秒，两者各按同一量纲比较。
- before 与 after 走**同一套**结构校验（manifest sha256、columns 名单/列序、`rows`、
  重复主键）：before 被事后篡改或列序反序同样必红。

## 2. 建角/删角/选角库后态（S1 / C5）

### 2.1 离线打印规范序列

```sh
python tools/db-parity/newchr_delchr_selchr_probe.py plan --account acct1 --name Hero
```

输出 `create` / `delete` 两条序列、每步的 `delay_before_ms`（`NEWCHR`/`DELCHR` 必须
>1000ms，oracle 限频）与期望后态（`characters_indexes.SelectID/IsDeleted`、建行表集合）。

### 2.2 真机采集（R 窗口；本车道不执行）

驱动由窗口卡提供（真实 socket 客户端或夹具），本脚本只负责编排与快照：

```sh
python tools/db-parity/newchr_delchr_selchr_probe.py run \
  --db mir2_test_db --sequence create --account acct1 --name Hero \
  --out E:/tmp/w1/run-oracle \
  --driver-cmd "python E:/tmp/w1/driver.py --step {step} --account {account} --name {name}"
```

`run` 做三件事：拍 before 快照 → 按序列调用驱动（env 注入 `PROBE_STEP` 等）→ 拍 after
快照，落 `run.json`。快照表名会**重定到 `--db`**（例如 `--db mir2_test_db` ⇒
`mir2_test_db.characters`），绝不会在目标库是测试克隆时误读 `mir2_db` 生产表。**本脚本不启动服务端/客户端、不写数据库**；驱动写库是窗口卡的授权范围。
库名必须是 `mir2_test_*`（`test` = `mir2_test_db`），否则拒绝；非测试库必须显式
`--allow-db <库名>`（stderr 告警）。

**fail-closed**：before 快照失败 ⇒ 立即停止、**不调用驱动**、返回非零；任一驱动步骤
非零 ⇒ 返回非零；after 快照失败 ⇒ 返回非零。`run.json` 始终落盘（含
`ok`/`aborted_reason`/`failures`），便于失败现场审计。存档的 argv 会把
`--password <明文>` 脱敏为 `--password ***`。`--driver-cmd` 用 `shlex.split(posix=False)`
解析，Windows 反斜杠路径不再被打坏（含空格的路径请整体加引号）。

### 2.3 离线断言与比对

```sh
python tools/db-parity/newchr_delchr_selchr_probe.py assert --run-dir E:/tmp/w1/run-oracle \
  --json-out E:/tmp/w1/assert.json
python tools/db-parity/newchr_delchr_selchr_probe.py compare \
  --a E:/tmp/w1/run-oracle --b E:/tmp/w1/run-rust \
  --summary-out E:/tmp/w1/cmp.json --jsonl E:/tmp/w1/cmp.jsonl
```

`assert` 不断言回包字节（那是 C1，由帧采集负责），只判**库后态**：

| 序列 | 断言 |
|---|---|
| `create` | `characters` 1 行；`characters_indexes` 1 行 `IsDeleted=0`；SELCHR 成功后 `SelectID=1`（否则 0）；同账号其他行 `SelectID=0`；`ablity`/`status` 有行；`item`/`bagitem`/`storageitem` 非空 |
| `delete` | `characters_indexes.IsDeleted=1`；`characters` 行**仍在**（oracle 软删除只写索引表） |

`compare` 调 `dbsnap_pair.run_pair`，默认 `--key-mode pk`、`--time-mode normalized`，
并把 `require_tables` 钉到 LB-10 十表；返回码即 C3 判据。表名按两份快照的**实际库前缀**
归一对齐（E1 克隆各跑各的也能比）。

S1 建角会写 `now()` 的 5 个时间列，两侧独立 run 的墙钟必然不同；可绿的组合是显式忽略
已登记时间列（随机建角夹具还需忽略 `Hair/Sex/Job`，自增列按同基线/固定夹具处理）：

```sh
python tools/db-parity/newchr_delchr_selchr_probe.py compare   --a E:/tmp/w1/run-oracle --b E:/tmp/w1/run-rust   --time-mode normalized --ignore-time-columns   --ignore-columns Id,PlayerId,Hair,Sex,Job   --summary-out E:/tmp/w1/s1-cmp.json --jsonl E:/tmp/w1/s1-cmp.jsonl
```

## 3. 口径（显式定义）

### 主键与排序

| 表 | 主键 |
|---|---|
| `mir2_account.account` | `Id` |
| `mir2_account.account_protection` | `AccountId` |
| `mir2_db.characters` | `Id` |
| `mir2_db.characters_indexes` | `Id` |
| `characters_ablity` / `characters_status` | `PlayerId` |
| `characters_bonusability` | `PLAYERID` |
| `characters_item` / `bagitem` / `storageitem` / `item_attr` / `magic` / `quest` | `Id` |

- 默认按主键元组对齐，**与 tsv 行序无关**；同侧重复主键 ⇒ RED（`duplicate_key`）。
- 两侧自增 Id 不同时用 `--key-mode multiset`，并用 `--ignore-columns` 排除
  `Id`/`PlayerId`；`--ignore-columns` 只允许**已登记易变列/时间列/主键**，行为列
  （`Level`/`Position`/`SelectID`/`IsDeleted` 等）不许静默忽略（工具 rc=2 拒绝）。
- 排序：按 (表名, 主键) 输出，输入相同则 JSONL 字节稳定。
- **字节保真**：tsv 按**原始字节**读入（逐字段 latin-1 一一映射，不会因 GBK 名字
  解码失败）；JSONL 用 ASCII 转义输出，比对结果与终端编码无关。非 ASCII 值在 JSONL
  里显示为 U+00xx 转义序列，但比较仍是逐字节的。
- `Hair`/`Sex`/`Job` 在默认（BotSrv 随机建角）夹具里登记为易变列，允许
  `--ignore-columns`；**固定建角场景**（`openmir2-m3-botsrv-fixed-newchr`）下它们是行为列，
  必须逐字节比，不得忽略。

### NULL

- dbsnap `mysql --batch --raw` 把 SQL NULL 落成字面量 `NULL`；本工具默认把它判为 SQL NULL
  （`--null-token` 可改）。
- 已登记限制：**字符串值 `"NULL"` 与 SQL NULL 不可区分**（同 dbsnap L2）。需要区分时
  必须改 dbsnap 的转义口径，本工具不静默猜测。
- NULL vs 非 NULL 在任何时间模式下都判差异。

### 时间列

登记在 `TABLE_SPECS[...]["time_columns"]`：

- `exact`（默认）：逐字节比；
- `normalized`：DATETIME 统一为 `YYYY-MM-DD HH:MM:SS.ffffff`（`T`→空格、小数右补 0）；
  纯整数 unix 秒保持十进制并按整数比较；无法识别格式按 exact 比，并在摘要的
  `unrecognized_time_values` 计数（不静默）；
- `presence`：只比 NULL/非 NULL。

时间列**不会自动忽略**；`CreateDate/ModifyDate/LASTUPDATE` 等要忽略必须显式
`--ignore-columns`（或探针 `--ignore-time-columns`），且只允许已登记的时间列
（工具 rc=2 拒绝其他列）。

## 4. 改坏必红（selftest 覆盖）

`dbsnap_pair.py selftest`（30 项）：一致判绿；改 `Level`、增/删行、NULL vs 0、时间格式、
schema 列序、tsv 篡改、重复主键、非法 ignore、multiset 身份列、changed 奇偶、缺 required
表、scoped_diff 表登记漂移——每项都指定「必须红/必须绿」。新增牙包括：两侧仅库前缀不同
判绿、`--require-tables` 进入比较集、changed 行的 before 篡改/列序反序必红、时间列
严格晚于（相等/回退必红）、unix 秒时间列、normalized 无法识别计数、
`--allow-unknown-table` 摘要标注。

`newchr_delchr_selchr_probe.py selftest`（25 项）：create/delete 规范后态判绿；
`IsDeleted=1` 于建角、SELCHR 未成功却 `SelectID=1`、SELCHR 成功却 `SelectID=0`、同账号
其他行被选中、删角后 `IsDeleted` 仍 0、删角把 `characters` 行删掉、驱动步骤失败、
`Position` 变异（M1 类比）、库名守卫——逐项必红。新增牙：前缀不同但内容相同的两份 run
assert/compare 判绿、`run` 在 before 快照失败时非零且驱动未被调用、驱动失败/after 失败
非零、口令脱敏、Windows 反斜杠驱动命令、`--allow-db` stderr 告警。

这些是**工具自身的牙**，不是 LB-10 的真机证据；真机证据仍须 W-DB-1..7 占窗口。

## 5. 未做 / 未验证（如实登记）

- **本车道未连 MySQL、未起服务端/客户端、未写数据库**；所有 selftest 用临时目录里的
  合成 dbsnap 快照。`run`/`snap` 的真机路径**未被本车道执行**（属 R 窗口）。
- **未做**跨库自增 Id 的自动映射：两侧独立自增时需先重置到同一基线，或 `--key-mode
  multiset --ignore-columns Id,PlayerId` 并在报告里记录「身份列未比较」。
- 值内 TAB/LF 的结构风险沿用 dbsnap 的已登记限制（S1/S2 由 dbsnap 负责）；本工具只看
  dbsnap 已经产出的快照，并额外核对 `manifest.rows` 与 tsv 行数（不符 ⇒ `row_count` 红）。
- `quest` 表已登记列/主键，但**不在** LB-10 的 `--scope lb10` 十表内（G-04 已知不持久化）。
- 未覆盖 5700 拍卖行（部署未加载 MarketSystem.dll，S4B-5..10 已登记）。
- 帧/DB 语句序/锁采样（`frames.ndjson`/`db_stmt.ndjson`/`locks.ndjson`）**不在本目录**：
  那是设计 §6.2 的采集物，由窗口卡的驱动/代理产出；本目录只消费 `table_dump` 等价物
  （dbsnap 快照）。
