# GM 命令可达性登记（88 条未实现命令）——2026-10-23

- 卡：`openmir2-gm-reachability-registry`（owner `openmir2-worker-24`，tier=L）
- 工具：`mir2-rs/tests/parity/coverage/gm_registry.py`；登记表：`gm_registry.json`
- 口径：见 `mir2-rs/tests/parity/coverage/README.md`「GM 命令可达性登记」节

## 1. 汇总（一条命令可复算）

```
python -I -B mir2-rs/tests/parity/coverage/gm_registry.py summary
GM_REGISTRY total=88 reachable=0 unreachable=88 undetermined=0
imp: {"Unimplemented": 88}
perm_source: {"GameCommands.cs": 19, "None": 12, "conf[Permission]": 50, "cs[Command]": 7}
unreachable_breakdown: {"no_handler": 28, "perm_gt_0": 60}
table_total=185 imp_breakdown={"Implemented": 97, "Unimplemented": 88}
```

| 维度 | 数值 |
|---|---:|
| 整表（`gmcmd.rs` `COMMANDS`） | 185 |
| 已实现（G1 5 + 批次2/3 30 + 批次3 18 + 批次4 44） | 97 |
| 未实现（本登记表覆盖） | **88** |
| 其中 `unreachable`（perm>0，权限门挡住） | 60 |
| 其中 `unreachable`（结构性：现网无命令体） | 28 |
| 其中 `reachable`（perm==0，必须开实现卡） | **0** |
| 其中 `undetermined` | 0 |

`perm_min` 分布：{"10": 76, "null": 12}

## 2. 结构性不可达的 28 条（现网无命令体）

| 类别 | 条数 | 判据 |
|---|---:|---|
| conf 有改名但 `GameCommands.cs` 无该 `CmdName` | 12 | 现网无 `GameCmd`，命令不存在 |
| `GameCommands.cs` 有 `CmdName` 但无 `[RegisterCommand]` 处理类 | 16 | 命令已登记但无命令体 |

conf-only 样例：`fdgfdg56`、`gfh678dfg`、`ghjhg6`、`hjhg556`、`hjhgj67`、`MemberFuncEx`、`OffHorse`、`sadsa54`

## 3. 权限门挡住的 60 条（抽样）

| 命令 | CmdName | perm_min | 权限来源 | handler |
|---|---|---:|---|---|
| `+-jy` | DisableSendMsgList | 10 | conf[Permission] | `src/Modules/GameCommand/Commands/DisableSendMsgListCommand.cs:7` |
| `+jy` | DisableSendMsg | 10 | conf[Permission] | `src/Modules/GameCommand/Commands/DisableSendMsgCommand.cs:7` |
| `-jy` | EnableSendMsg | 10 | conf[Permission] | `src/Modules/GameCommand/Commands/EnableSendMsgCommand.cs:10` |
| `Announcement` | Announcement | 10 | conf[Permission] | `src/Modules/GameCommand/Commands/AnnouncementCommand.cs:8` |
| `ClearCopyItem` | None | 10 | cs[Command] | `src/Modules/GameCommand/Commands/ClearCopyItemCommand.cs:11` |
| `ClearItemMap` | None | 10 | cs[Command] | `src/Modules/GameCommand/Commands/ClearItemMapCommand.cs:6` |
| `ContestPoint` | ContestPoint | 10 | conf[Permission] | `src/Modules/GameCommand/Commands/ContestPointCommand.cs:10` |
| `DelNpc` | None | 10 | conf[Permission] | `src/Modules/GameCommand/Commands/DelNpcCommand.cs:9` |
| `DisableFilter` | DisableFilter | 10 | conf[Permission] | `src/Modules/GameCommand/Commands/DisableFilterCommand.cs:10` |
| `EndContest` | EndContest | 10 | conf[Permission] | `src/Modules/GameCommand/Commands/EndContestCommand.cs:8` |

## 4. 门禁与牙

```
$ python -I -B mir2-rs/tests/parity/coverage/gm_registry.py gate
GREEN: gm_registry 88 条全部通过（handler 存在或结构性缺省 ∧ 行号在界 ∧ verdict 与 perm_min 一致）

$ python -I -B mir2-rs/tests/parity/coverage/gm_registry.py selftest
  [ok] ① perm_min==0 改判 unreachable ⇒ gate 红
  [ok] ② handler 行号越界 ⇒ gate 红
  [ok] ③ StartPermission=1 ⇒ 全翻 reachable
gm_registry selftest: OK (3 teeth)
```

## 4b. 仓库门禁（本卡零 .rs 改动，逐项留证据）

```
cargo fmt --all -- --check                       => rc=0
python tools/whitelist/t_reservation_gate.py check => GREEN（rows=194 whitelist=194 max=T-205 fence_ok）
python mir2-rs/tests/parity/m3/castle_oracle_anchor_gate.py check => GREEN（222 锚命中冻结 src/ + 写侧在位）
python mir2-rs/tests/parity/m3/m3_whitelist_registry_audit.py audit => GREEN（无重复编号）
cargo run -p msgcodegen -- --check               => OK 604 条常量与 C# 源一致
```

## 5. 结论

1. **88 条未实现命令全部 `unreachable`，其中 0 条 `reachable`** ⇒ Rust 不实现它们与现网 oracle 行为等价；
   这不是「一句 GM 不可达」，而是逐条 `cmd → handler → perm_min → verdict` 的静态证据（见 `gm_registry.json`）。
2. 28 条是**结构性**不可达（现网根本没有命令体），与权限无关；60 条是权限门（`perm_min>0` + `StartPermission=0`）。
3. 判据依赖实值：把 `StartPermission` 假设改成 1，全部翻为 reachable（牙③）；把任一条 `perm_min=0` 改判
   `unreachable` 或把 handler 行号改到越界，门禁立即红（牙①②）。
4. 数据源缺一即 `undetermined`→门禁红：部署 `Command.conf` 不存在时 `sources.command_conf.state=absent` 显式登记。
