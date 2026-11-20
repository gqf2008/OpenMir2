# eat-probe —— `CM_EAT`（使用药品）oracle 实测向量生成器

引用**真身** `src/M2Server`（反射调私有 `ClientUseItems` / `WinLottery`），把
「即时帧 + `MsgQueue` 出队通知 + 通知文本尾」录成 `mir2-rs/tests/parity/m3/fixtures/eat_vectors.json`，
Rust 侧 `bins/game-svr/src/useitem.rs` 的 `matches_oracle_eat_fixture` /
`it16_lottery_fixture_rng_and_deltas` 回读逐字段、逐字节核对。

## 生成命令（在仓库根运行）

```text
dotnet build mir2-rs/tests/parity/m3/eat-probe -c Release
MIR2_RNG_SEED=20261120 dotnet mir2-rs/tests/parity/m3/eat-probe/bin/Release/net8.0/eat-probe.dll \
  mir2-rs/tests/parity/m3/fixtures/eat_vectors.json
```

**`MIR2_RNG_SEED` 不是可选的**：不设时探针不跑彩票批，落盘的 `lottery.enabled=false`、
`lottery.cases=[]` —— 它会把 master 上已有的 **12 例彩票向量静默抹掉**（改动的 diff 只有
"删掉一大段"，极易混进别的批）。提交夹具前先核 `lottery.cases` 长度为 12。

配置文件来源：`EATPROBE_CONF_DIR`（缺省 `E:\MirServer\M2GameSvr`）下的 `Server.conf` /
`Exps.conf`，拷进探针 bin 目录后由 `SystemShare` 装载；缺失 ⇒ rc=2 不落盘。

## 夹具里的两个硬约束

1. **不得出现绝对路径**。配置身份只用文件名 + sha256（如 `config.exps_conf = "Exps.conf"` +
   `exps_conf_sha256`）——绝对路径是"某台机器某 worktree"的产物，进仓即不可复现。
   Rust 侧 `fixture_free_of_absolute_paths` 会把手写/换机生成的绝对路径判红。
2. **通知的文本尾逐字节记录**：`Drain()` 的 `TextHex()` 读 `SendMessage.Buff`，
   存 `EDCode.EncodeString(Buff)` 的**原始字节**（hex）；空 `Buff`（无文本通知）⇒ `null`。
   Rust 侧按字节比，不经任何二次转换。别改成存原文或再编码一次。

## 已知登记缺口

- `RM_MENU_OK` 也带文本尾（oracle `Operate.cs:685` 的 `MessageSettings.CanotUseItemMsg`，
  drop/脚本另有同 ident 两处）。夹具照实记录，Rust 比较器按**集合白名单**
  （`{RM_MENU_OK}`）钉住，不建模但也不静默放过。
