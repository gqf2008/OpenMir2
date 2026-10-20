# LB-10-E1 持久化面真机证据 —— run sheet（工具就绪版）

- 卡/设计：`audit-dbsrv-persistence-20261020.md` LB-10；
  `design-dbsrv-connection-20261021.md` §6（C1..C8 / M1..M8 / W-DB-1..7）。
- tier：**R**（持久化落库 / 时序 / 故障注入 ⇒ 必须有独占窗口）。
- 工具：`tools/db-parity/`（用法与口径见 `tools/db-parity/README.md`）。
- 本文件是**窗口执行清单**；工具本身 tier=L，已离线自检绿。真机项**未执行**时不得写
  「已验证」。

## 0. 前置（窗口卡执行）

1. 两份同基线库：`mir2_test_db_oracle`、`mir2_test_db_rust`（E1 裁决：测试库克隆；
   schema/data 同基线。设计文档里的 `mir2_db_oracle`/`mir2_db_rust` 若不带 `mir2_test_`
   前缀，dbsnap/探针会按库名守卫拒绝）。
2. `performance_schema=ON`；失败时降级 `general_log=ON, log_output=TABLE` 并在 `notes.md` 写明。
3. 记录两端 build：C# DLL hash、Rust binary hash、`git rev-parse HEAD`。
4. 采集目录：`/e/tmp/<window>/db-evidence/<run_id>/`；每 run 一个目录，`manifest.sha256`
   对全部文件做 `sha256sum -c`。

## 1. W-DB-1 正常路径（S1+S2，C1/C2/C3/C4）

```sh
# 拍 run 前基线（探针 snap 会把表名重定到 --db，并自动 --allow-db）
python tools/db-parity/newchr_delchr_selchr_probe.py snap --db mir2_test_db_oracle --out E:/tmp/w1/before-oracle
python tools/db-parity/newchr_delchr_selchr_probe.py snap --db mir2_test_db_rust   --out E:/tmp/w1/before-rust

# S1：建角/选角（两侧各跑一遍；驱动由窗口卡提供）
python tools/db-parity/newchr_delchr_selchr_probe.py run --db mir2_test_db_oracle \
  --sequence create --account acct1 --name Hero --out E:/tmp/w1/run-oracle --driver-cmd "<oracle 驱动>"
python tools/db-parity/newchr_delchr_selchr_probe.py run --db mir2_test_db_rust \
  --sequence create --account acct1 --name Hero --out E:/tmp/w1/run-rust --driver-cmd "<rust 驱动>"

# S1 库后态断言（C5 的库后态部分）与逐字段 diff（C3）
python tools/db-parity/newchr_delchr_selchr_probe.py assert --run-dir E:/tmp/w1/run-oracle
python tools/db-parity/newchr_delchr_selchr_probe.py assert --run-dir E:/tmp/w1/run-rust
python tools/db-parity/newchr_delchr_selchr_probe.py compare \
  --a E:/tmp/w1/run-oracle --b E:/tmp/w1/run-rust \
  --summary-out E:/tmp/w1/s1-cmp.json --jsonl E:/tmp/w1/s1-cmp.jsonl

# S2：6000 取档/穿脱/仓库/存档（驱动由窗口卡提供）后再拍 after，做 C3 changed 模式
python tools/db-parity/dbsnap_pair.py diff \
  E:/tmp/w1/after-oracle E:/tmp/w1/after-rust --scope lb10 \
  --require-tables mir2_db.characters,mir2_db.characters_indexes,mir2_db.characters_ablity,mir2_db.characters_item,mir2_db.characters_bagitem,mir2_db.characters_storageitem,mir2_db.characters_bonusability,mir2_db.characters_status,mir2_db.characters_item_attr,mir2_db.characters_magic \
  --baseline-a E:/tmp/w1/before-oracle --baseline-b E:/tmp/w1/before-rust \
  --time-mode normalized \
  --jsonl E:/tmp/w1/s2-diff.jsonl --summary-out E:/tmp/w1/s2-diff.json
```

判据：`diff.jsonl` 为空且 assert 两项 GREEN；时间列按 `changed_mismatch` /
`time_not_later` 机判，不许只写「变化了」。

## 2. W-DB-2 故障矩阵（S3，C1/C3/C5/C7）

每格独立 run：MySQL stop/start、`KILL` 连接、`characters_item` 第 5 条 INSERT `SIGNAL`、
`characters_indexes` UPDATE `SIGNAL`、`SELECT ... FOR UPDATE` 持锁、`max_connections=1`。
每格都要：回包帧（`frames.ndjson`，C1/C2 由帧采集负责）+ 库后态（本目录 assert/compare）+
无「零回包超时」差异。对应变异 M3/M5/M7 必须让指定断言红。

## 3. W-DB-3 慢存档并发（S4，C6）

外部事务持 `characters` 行锁/`SLEEP(3)`；同时发 6000 LOAD、5100 QUERYCHR、SELCHR、
另一角色 SAVE、5700 查询。P99 阈值见设计 §6.4 C6（500ms / 1s），oracle 同场景同表采集。

## 4. W-DB-4 崩溃/重启（S5，C8）

事务中途 `KILL` 连接 / 杀 DBSrv；重启后跑 `assert` + `compare`：9 表无半套行、
`characters_indexes` 孤儿集合与 oracle 一致。

## 5. W-DB-5 M3 ability RED（承卡确认，不另开窗）

`openmir2-m3-scoped-diff-ability`：`scoped_diff.py diff --canonical` 在
`mir2_db.characters_ablity` 的 4 行。本卡只确认承卡状态；未闭环则 LB-10 不得 `closed`
（设计 §6.1）。本工具 `check-spec` 可用于确认表登记未漂移，但**不**替代该承卡判据。

## 6. W-DB-6 / W-DB-7

- W-DB-6：并发 10 取档 + 停库；C7 + `SHOW PROCESSLIST` 峰值 <10 即红。
- W-DB-7：两个 GameSvr 连接同时 LOAD，DB `thread_id` 不同且时间窗重叠；若同连接实际
  串行，C6/C7 的 oracle 基线需重算。

## 7. 变异牙（M1..M8，钉住 commit 执行）

| 变异 | 期望红 |
|---|---|
| M1 `save_item_table` Position 绑定错位 | `dbsnap_pair` `value`（`characters_item.Position`）或 C1 帧 |
| M2 `CM_DELCHR` 写 `IsDeleted=0` | probe `assert`：`indexes.IsDeleted`（selftest 已证此牙） |
| M3 `store_human_rcd` Err 上抛 | C5 帧；单元测试 G2/G3（不是 LB-01 的 load 用例） |
| M4 全局 Mutex / 池压到 1 | C6 P99、C7 并行超时 |
| M5 `save_record` 去掉事务 | C4 事务分组、C8 半套行 |
| M6 跳过 `SaveBonusability` | `dbsnap_pair`：`characters_bonusability` |
| M7 建角 storageitem 失败即回滚 | C5 建角后态/回包 |
| M8 6000 每请求 spawn 后直接回包 | C2 同连接回包顺序、C4 |

每颗牙记录三件套：**先红 → 逐字节还原 → `sha256sum -c` 绿**（命令 + 输出 + 路径 + hash）。

## 8. 收口

- W-DB-1..7 全绿，或 owner 对具体项书面 `D-*` 豁免；
- `review` 写 `tier=R` + `tier_why`；R 级卡合并后 `status ≠ closed`（保持 in-progress
  并写明待窗口项），除非窗口项已全绿。
