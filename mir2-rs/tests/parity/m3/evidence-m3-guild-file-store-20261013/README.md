# M3⑧-3 行会文件加载/落盘 · 证据包（2026-10-13）

- 卡：`openmir2-m3-guild-file-store`（issue `68015a01...`，⑧ 队列第 2 张）
- 车道：`openmir2-worker-5` / worktree `m3cs` / branch `worktree-m3-guild-file-store`
- base：`origin/master 7febf884a`（开工后 master 前移两次，均 `git merge origin/master` 后在
  **合并树**上重跑四门禁）
- 写集：`mir2-rs/crates/data/**`（+ 探针/证据 `mir2-rs/tests/parity/m3/**`）。
  **未改** `bins/game-svr/**`：本卡交付的是**文件层**，启动接线归 ⑧-4 行会状态机卡（见 §残留）。

## 1. 交付物

| 文件 | 内容 |
| --- | --- |
| `crates/data/src/guild.rs` | `GuildManager.LoadGuildInfo/SaveGuildList` + `GuildInfo.LoadGuild/LoadGuildFile/SaveGuildFile` + `GuildConf` 读侧；`GuildStore`（路径/IO/名单回写）+ 纯编解码（`parse_guild_file` / `serialize_guild_file` / `parse_guild_list` / `serialize_guild_list` / `parse_guild_conf` / `get_valid_str3`） |
| `crates/data/tests/guild_store_vectors.rs` | 11 条真身向量用例（逐字段 + 逐字节 + 边界 + fail-closed） |
| `tests/parity/m3/guild-store-probe/**` | C# 真身探针 + `vectors/guild_store_vectors.tsv`（89 字段）+ `vectors/guild_file_bytes.tsv`（3 逐字节） |

## 2. 判据（oracle 真身，非源码重推）

`guild-store-probe` 直调 `M2Server.Guild` 的 `GuildManager` / `GuildInfo` / `GuildConf`，在
`E:/tmp/m3cs-guildstore/work` 临时树里实跑 8 个场景，产出 `guild_store_vectors.tsv` 的
89 个字段与 3 个逐字节向量；Rust 用例逐条比对。

**锁死的 oracle 行为（每条都有向量/红检）**：

| # | 行为 | oracle 锚 | 证据 |
| --- | --- | --- | --- |
| ① | **落盘不新建文件**：`StringList.SaveToFile` 先 `FileMode.Open` ⇒ 目标不存在抛异常被吞；新行会 `<名>.txt` 写不出去 | `StringList.cs:260-274` + `GuildInfo.cs:428` | `A_save_missing_target{ txt_exists_after_save=False }` + `vector_save_refuses_to_create_target` |
| ② | `<名>.ini` 只被建成**空文件**，五键**从不落盘**（`SaveGuildConfig` 无 `Save()`） | `GuildConf.cs:8-17,27-33` | `ini_size=0`（A/B/D）+ `vector_ini_is_only_ever_created_empty` |
| ③ | 行会战 `WarTime` 加载**恒 0**（读跨行残留 `s20`，本段从不赋值） | `GuildInfo.cs:288-291` | `C_load{war_0_time=0, war_0_tick_nonzero=True}` + 红检(b) |
| ④ | **联盟段被丢弃**（`//GuildAllList.Add` 被注释） | `GuildInfo.cs:307-312` | `C_load ally_count=0` + `vector_parse_save_round_trip_drops_allies` |
| ⑤ | 职名只在 `Length > 30` 时截到 `GuildRankNameLen`(16) | `GuildInfo.cs:312-315` | `G_rank_0_name_len=16 / rank_1_name_len=30` |
| ⑥ | 缺 `<名>.txt` ⇒ 该会移除并**回写名单**；`ServerIndex != 0` 时回写 no-op | `GuildManager.cs:94-103,126-129` | `D_list_rewritten`（逐字节）/ `E_list_unchanged`（逐字节） |
| ⑦ | 名单文件缺失 ⇒ fail-closed（记错误、会表为空、**不建文件**） | `GuildManager.cs:81,108` | `F{list_file_still_absent=True, find_甲会=False}` |
| ⑧ | 分词 `Split(div, 2, RemoveEmptyEntries)` 的「余串含尾分隔符」口径 | `HUtil32.cs:391-402` | `H_get_valid_str3` 10 例 + `vector_get_valid_str3_tokenizer` |

## 3. 读数

| 判据 | 读数 |
| --- | --- |
| 探针（C# 真身） | 89 字段 / 3 逐字节向量（`02-probe-run.txt`） |
| `cargo test -p mir2-data` | **30 + 1 + 11 + 1 + 3 passed / 0 failed**（`03-rust-vectors.txt`） |
| 改坏必红 (a) 分隔行 `" "`→`""` | `vector_save_bytes_exact` **FAILED**，rc=101（`04-…`） |
| 改坏必红 (b) 「修好」WarTime quirk | lib 单测 + 向量用例**双红**，rc=101（`05-…`） |
| 四门禁（合并树 · `E:/tmp/m3cs-target` · `--offline`） | fmt rc=0 / clippy rc=0 / **95 suites 924 passed 0 failed 21 ignored** / msgcodegen **604**（`06..09`） |
| 边界 | `git diff <base> -- src sql MirClient` 空；只新增 `crates/data` 3 件 + `tests/parity` 5 件（`10-boundary.txt`） |
| 证据 hash | `verify_sha256sums.py --check-extra` GREEN（LF 归一 / git blob 口径）+ `eolcheck` GREEN |

## 4. 未做 / 残留（如实登记）

1. **启动接线未做**：`bins/game-svr` 里没有「启动时 `LoadGuildInfo` / 退出时 `SaveGuildList`」的调用点
   —— 属 ⑧-4（`openmir2-m3-guild-state`，写集含 `bins/game-svr`）。本片按写集边界只交文件层。
2. **`GuildConf` 五键的写侧不存在**（照搬 oracle：无 `Save()`）；若将来要「修好落盘」，
   属 `S-*` 新卡，不得静默偏着改。
3. **真机两侧对拍**（`run_guild_castle_pair.ps1`，⑧-10 夹具）要独占窗口，且依赖 ⑧-3..⑧-9 落地
   ⇒ 本片不占窗口、零起栈、零端口。
4. 探针临时树在 `E:/tmp/m3cs-guildstore/**`（含 `work/` 的 `.ini`/`.txt` 副作用件）——
   保留至本卡收口，收工时可整目录删除（不入库）。

## 5. 文件清单

`00-commands.txt` / `01-oracle-anchors.txt` / `02-probe-run.txt` / `03-rust-vectors.txt` /
`04-redcheck-a-separator.txt` / `05-redcheck-b-wartime-quirk.txt` / `06-gate-fmt.txt` /
`07-gate-clippy.txt` / `08-gate-test.txt` / `09-gate-msgcodegen.txt` / `10-boundary.txt` /
`11-vectors-inventory.txt` / `README.md` / `SHA256SUMS.txt`
