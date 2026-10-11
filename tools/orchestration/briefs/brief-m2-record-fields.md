# 任务：openmir2-m2-record-fields —— LoadPlayerDataPacket 全字段解析（逐字段组 + oracle 字节基准）

仓库：`E:\Users\gxh\Documents\GitHub\OpenMir2`。你已在**隔离检出** `.claude/worktrees/recfields`
（分支 `worktree-m2-record-fields`，**基于** `dc33c1ca` = `worktree-m2-gamesvr-host` 的当前 head，因为是**叠在 m2-svr 之上**的工作）。
身份：新 principal **openmir2-worker-2**（先 `walgit collab join --repo . --principal openmir2-worker-2 --key C:\Users\gxh\.walgit\keys\openmir2-worker-2.ed25519 --push walgit`）。
卡：`openmir2-m2-record-fields`（**待认领**，先 `git fetch walgit '+refs/collab/*:refs/collab/*'` 读线程，再发 `status: in-progress`）。

## 0. 为什么有这张卡（别自己推格式）

S2b-3 只解析到 `CharacterData.Dir`（头部窗口，见 `bins/game-svr/src/record.rs` 与已入库基准
`mir2-rs/bins/game-svr/tests/fixtures/player_data_oracle.txt`）。剩下 52 个成员决定**可玩性**（属性/背包/技能/任务），
也是入图帧里 `SM_ABILITY`/`SM_SUBABILITY`/物品与技能帧的数据来源。

## 1. 方法（照 S2b-2b/S2b-3 的做法，**不许猜 MemoryPack 布局**）

- 字段表与顺序：`src/OpenMir2/Packets/ServerPackets/PlayerDataInfo.cs`——**`CharacterData` 的成员顺序就是 MemoryPack 的写入顺序**，
  别按字母序/直觉分组；嵌套 `RecordHeader`/`Ability`/`NakedAbility`/`ServerUserItem`/`MagicRcd`。
- 布基准：用**部署件自己的序列化器**生成字节基准（模板 `E:\tmp\openmir2-gamegate-imp\dbgen\`，引用
  `E:\MirServer\M2GameSvr\OpenMir2.dll` + `MemoryPack.Core.dll`）：造一份**每字段可区分的样例**（不要全 0/全 1，否则偏移错了也看不出来），
  把 dump 入库到 `mir2-rs/bins/game-svr/tests/fixtures/`（新基准，别覆盖已有的 `player_data_oracle.txt`）。
- 逐组推进，**每组一个 patch**：`Abil`/`BonusAbil` → 点数字段 → `StatusTimeArr` → `HumItems/BagItems/StorageItems` →
  `Magic` → `QuestUnit*/QuestFlag`。**每加一组都要重新生成基准**（偏移会变）。
- 数组语义照抄：`HumItems[13]`/`BagItems[46]`/`StorageItems[50]`/`Magic[20]`/`StatusTimeArr[15]`/`Quest*[128]` 的长度、
  空位表示（注意 `ServerUserItem` 的空位）、以及 C# 末尾 `[OnSerializing] Serialized()` 对 null 数组的兜底——全部单测。
- 双向一致性：C# 序列化 → Rust 解出的值 == C# 对象的值；**Rust 重编码 → 与 C# 字节逐字节相同**。

## 2. 写集（§0b，**必须**守住——这是叠在 m2-svr 分支上的并行工作）

- **允许**：`mir2-rs/bins/game-svr/src/record.rs`（把新字段与单测都放在这个文件里）、
  `mir2-rs/bins/game-svr/tests/fixtures/**`（新增基准）、你自己的 `docs/`。
- **禁止**：`mir2-rs/bins/game-svr/src/main.rs`（**m2-svr 车道正在写它**：入图帧接线）、
  `src/**`（C# oracle）、`crates/**`、`bins/game-gate/**`、`MirClient`、表结构/脚本语法/数据文件格式。
- 需要接线到生产路径时**不要自己动 main.rs**：把「解析 API 签名 + 待接点」写进卡内 comment，留给 m2-svr 车道/owner 排。

## 3. 判据

- 每个字段组：oracle 基准（入库，含生成命令与 sha256）+ Rust 单测逐字节对照；
- **红检**：把某一组的字段序或数组长度写错 ⇒ 对应用例必须红（留原始输出）；
- 硬门禁（隔离 target，例如 `CARGO_TARGET_DIR=E:\tmp\openmir2-recf-target`）：`cargo fmt --all -- --check`、
  `cargo clippy --workspace --all-targets --offline -- -D warnings`、`cargo test --workspace --offline`、
  `cargo run -p msgcodegen -- --check`；
- 未做的字段组在卡里逐条登记（别写「留作后续」了事）。

## 4. 交付

`patch --base refs/heads/worktree-m2-gamesvr-host --head refs/heads/worktree-m2-record-fields`
（**base 显式写成 m2-svr 分支**——这是叠栈工作，评审只看你自己的差量；合入顺序：m2-svr 先合，再合本卡）
→ `status: needs-review`（parent 用该线程最后一条 entry 的 oid）。**不 merge、不签 review。**
收工写 `E:\tmp\openmir2-watch\last-recf.md`（做了哪些组、判据原始输出、卡在哪、剩余物清单）。

## 5. 红线

- 不起服务栈（端口由审查线按窗口用）；不改 `src/**` 行为；不动别人的卡状态。
