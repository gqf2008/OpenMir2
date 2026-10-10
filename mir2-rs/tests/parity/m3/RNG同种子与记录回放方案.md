# RNG 同种子注入与记录-回放方案（M3 掉落/经验逐次一致）

D2 线交付（对应设计文档 §6 M3「固定种子下 100 次击杀掉落列表逐项相同 / 经验逐级相同」的准备项，
解决白名单 **D-3**：C# 生产路径 `new Random()` 无参（.NET 6+ 为 xoshiro256\*\*）无法固定种子）。

## 一、能对拍什么、不能对拍什么

| 层次 | 判据 | 现在能不能做 | 依据 |
| --- | --- | --- | --- |
| **L1 数值级** | 同种子 + 同一输入 → 两侧数值逐项相同 | ✅ 已有门禁 | `tests/parity` 门禁②③④（掉落/经验/等级属性全表 diff=0） |
| **L2 取数序列级** | 两侧"取数的形态/顺序/次数"逐条一致 | ✅ 本轮新增（记录-回放） | `rng_replay_parity.rs`：C# 真实进程 1828 次取数，Rust 复刻逐项重算一致 |
| **L3 端到端级** | 同一操作序列 → 两侧出包/DB 字段 diff=0 | ⚠️ 部分：需先冻结非 RNG 熵源 | 见下"实测边界"与 `README.md` 已知阻塞 |

**不能对拍的范围（明确写清）**：

1. **无参 `new Random()` 的真实取值**——.NET 6+ 用 xoshiro256\*\*，且种子来自运行时熵，
   **两次运行本就不同**，不存在"真值"可对。我们的做法是**注入固定种子**（带种子构造走 legacy
   减法算法，与 Rust 复刻同算法），把"不可对拍"变成"可对拍"。
   副作用须知：注入后的手感与线上无参序列不同（线上是 xoshiro），**这是测试口径，不是行为等价**。
2. **依赖墙钟/线程调度的路径**——`HUtil32.GetTickCount()`、`DateTime.Now`、NPC 处理线程的
   遍历顺序、网络到达顺序。这些不进 RNG，但会改变"谁在什么时候取数"。
3. **多线程并发的取数交错**——C# 世界侧 NPC 处理是多线程的；Rust 侧设计文档 §4.2 要求单线程串行 ⇒
   两侧在 L3 上的可复现性天生不对称（这也是"记录-回放"存在的理由）。

## 二、L2 方案：Startup Hook（同种子 + 记录），已实测跑通

工具：`m3/rng-hook/`（net8.0 类库，`RngSeedHook.dll`）。**不碰冻结基线**——
只把 DLL 放进**影子副本**目录（robocopy `E:\MirServer\M2GameSvr` 到会话目录），
通过环境变量启用：

```bat
set DOTNET_STARTUP_HOOKS=E:\tmp\m3-rng-shadow\gamesvr\RngSeedHook.dll
set MIR2_RNG_SEED=42
set MIR2_RNG_LOG=E:\tmp\m3-rng-shadow\rng.log
set MIR2_RNG_SITE=1
E:\tmp\m3-rng-shadow\gamesvr\GameSrv.exe
```

要点（都踩过）：

- 影子副本的 `GameSrv.runtimeconfig.json` 里 `System.StartupHookProvider.IsSupported` 是
  **false**（trimmed 发布），必须改成 `true` 才会加载钩子；
- 钩子类型必须叫**全局命名空间的 `StartupHook`**（带命名空间报 `TypeLoadException`）；
- 影子 `Server.conf` 的 `GatePort` 改到 15000，避免与在跑的 GameSvr 抢 5000；
- 钩子用反射把 `OpenMir2.RandomNumber` 的私有静态字段 `random` 换成
  `RecordingRandom`（继承 `System.Random`，其取数方法都是 virtual ⇒ 能完整拦截）；
- **失败必须显式非零退出**（钩子内 `Environment.Exit(97)`），否则"以为注入、其实没注入"
  会污染整批结论。

实测证据（`MIR2_RNG_SEED=42`）：

```
SEED_PROBE 1434747710 302596119 269548474 1122627734 361709742 563913476 1555655117 1101493307
```

与 Rust `DotNetRandom::new(42)`（`crates/shared/src/rng.rs`，门禁②的金标准同源）**逐项相同** ⇒
"C# 进程内的随机源"与"Rust 复刻"是同一个算法、同一条序列。
一次运行的记录规模：4,254 条取数（世界初始化阶段），形态全部是 `Next(max)`。

## 三、L2 方案之二：记录-回放（补上"顺序/次数"这半边）

同种子只保证"取值相同"；**调用顺序错、次数多/少**仍可能整体一致地看着"对"。
`crates/shared/src/replay.rs` 的 `ReplayRandom` 逐条消费 C# 记录：
形态或参数不符即记入 `divergences`（带记录里的 `site`，直接指到 C# 调用点），
返回记录值，于是"公式是否等价"变成纯比较。

Rust 侧接入方式（对既有调用点**不破坏**，均为泛型化）：

```rust
// crates/shared/src/rng.rs
pub trait RandomSource { fn random(&mut self) -> i32; /* random_below/random_range/... */ }
impl RandomSource for RandomNumber { /* 同种子实现 */ }
impl RandomSource for DotNetRandom { /* 原始 PRNG */ }
// crates/shared/src/replay.rs
impl RandomSource for ReplayRandom { /* 记录-回放实现 */ }
// crates/formula：rng: &mut impl RandomSource（原 &mut RandomNumber 调用点无需改动）
```

门禁（`tests/parity/tests/rng_replay_parity.rs`，4 条全绿）：

| 测试 | 判据 |
| --- | --- |
| `csharp_process_stream_regenerates_from_seed` | C# 真实进程前 1828 次取数，Rust 从 seed=42 重算**逐项相同**（该区段经两次同种子运行实测逐字节一致） |
| `replay_harness_consumes_csharp_stream` | 4000 条记录逐条消费：零分歧、恰好用尽 |
| `redcheck_tampered_result_must_differ` | 改掉记录里一个结果 → 必须能发现 |
| `redcheck_wrong_call_shape_must_diverge` | 调用上限 +1 → 必须记为分歧 |

夹具：`m3/fixtures/rng_stream_seed42_head1828.tsv`（确定性区段）、`..._seg4000.tsv`。

## 四、实测边界：种子注入**不等于**逐次一致

同一 seed=42 跑两次影子 GameSvr，**前 1828 次取数逐字节相同**，第 1829 次起分岔：

```
run1: 1823  Next(max) 50 28  GameSrv.Npc.Merchant.Run@Merchant.cs:132
      1824  Next(max) 50 29  GameSrv.Npc.Merchant.Run@Merchant.cs:138
run2: 1823  Next(max) 40 22  GameSrv.Npc.GuildOfficial.Run@GuildOfficial.cs:41
      1824  Next(max) 30 17  GameSrv.Npc.GuildOfficial.Run@GuildOfficial.cs:47
```

两侧消耗的样本数相同（各 2 次），故随机流位置**自愈**，但缩放上限不同（50/50 vs 40/30）
⇒ 数值不同。根因是**非 RNG 熵源**：NPC 处理线程的遍历顺序/时钟门控
（`Merchant.Run` 里有 `GetTickCount()` 驱动的补货/清理分支）。

⇒ 结论与对策（M3 落点）：

1. **L2 用记录-回放，不强求 C# 侧确定**：C# 跑一次、记录整条流，Rust 侧必须复现同一条
   "取值 + 形态 + 次数"序列。这样 C# 的墙钟不确定性不再是对拍的障碍——它只决定"记录长什么样"。
2. **L3 若要逐次一致**，必须先把非 RNG 熵源逐条冻结（影子副本内）：
   ① 冻结时钟（`HUtil32.GetTickCount`/`DateTime.Now`；静态方法无法像 `Random` 那样用子类拦截，
   需 IL 补丁或影子重编译——C 线已有"影子副本 + 字节级 IL 常量替换 + 命中数预检"的先例）；
   ② NPC/对象遍历顺序固定；③ 操作序列由确定性驱动端给出（假人 RNG 同样注入种子）。
   在此之前，L3 的报告只能写成"**同一操作序列下、固定播种后的差集**"，并把不达 0 的项
   逐条登记白名单——不许写成"基本一致"。

## 五、M3 落点清单

| M3 验收（§6） | 用本方案的哪一层 | 具体做法 |
| --- | --- | --- |
| 战斗：固定种子下 100 次攻击伤害序列逐项相同 | L1 | 已有 `crates/formula` + 门禁②同款"固定种子 + 固定输入"对拍 |
| 掉落/经验：固定种子下 100 次击杀掉落逐项相同、经验曲线逐级相同 | L1（已具备）+ L2（本轮） | 数值用 L1 门禁④；"取数顺序"用 `ReplayRandom` 消费记录流 |
| 同序列对拍（出包差异数 = 0） | L3 | 先用 `run_pair.ps1` 产出两侧基线（`baseline.json` + 快照 hash），
再做 `scoped_diff.py diff`；不达 0 的项进 `whitelist.md` 并挂上本方案的熵源清单 |

## 六、复现命令（本方案全部证据）

```powershell
# 1) 影子副本 + 钩子（不碰 E:\MirServer）
robocopy E:\MirServer\M2GameSvr E:\tmp\m3-rng-shadow\gamesvr /E
#    改影子 runtimeconfig: System.StartupHookProvider.IsSupported -> true
#    改影子 Server.conf:   GatePort=5000 -> 15000
dotnet build -c Release mir2-rs/tests/parity/m3/rng-hook/RngSeedHook.csproj
copy mir2-rs\tests\parity\m3\rng-hook\bin\Release\net8.0\RngSeedHook.dll E:\tmp\m3-rng-shadow\gamesvr\

# 2) 带种子启动影子（另开窗口，跑够 30s 后 Ctrl+C / 结束进程）
set DOTNET_STARTUP_HOOKS=E:\tmp\m3-rng-shadow\gamesvr\RngSeedHook.dll
set MIR2_RNG_SEED=42 & set MIR2_RNG_LOG=E:\tmp\m3-rng-shadow\rng.log & set MIR2_RNG_SITE=1
E:\tmp\m3-rng-shadow\gamesvr\GameSrv.exe

# 3) 对拍
cargo test -p mir2-parity-tests --test rng_replay_parity
```

影子进程用完请结束（`Get-Process GameSrv | Where-Object { $_.Path -like 'E:\tmp\*' } | Stop-Process -Force`），
**不要**动 `E:\MirServer\M2GameSvr\GameSrv.exe`（线上 oracle）。
