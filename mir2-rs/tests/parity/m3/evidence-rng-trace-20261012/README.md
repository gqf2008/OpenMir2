# M3②③ 决策片证据：Rust 侧同形 RNG trace + 抽数流交错定位（owner 2026-10-12 18:00）

- 卡：`openmir2-m3-combat-skills-align`（issue `a2ef74aa`）／车道 `m3cs`／principal `openmir2-worker-5`
- base：`refs/heads/master` = `324a182b`（本片提交后 head 见 patch body）
- 任务来源：brief 顶部 owner 2026-10-12 18:00「决策片」——本片只做 **trace + 定位 + 选项**，不要求逐 k=0；
  **不 merge、不自签**；未起栈、未占端口。

## 0. 本片交了什么

1. **Rust 侧 opt-in 逐次 RNG trace**（`crates/shared/src/{rng_trace.rs,rng.rs}` + `crates/shared/tests/rng_trace.rs`）：
   与 C# `rng-hook/StartupHook.cs` 落盘**逐列同形**（`seq \t kind \t arg \t result \t site`）；开关 =
   环境变量 `MIR2_RNG_LOG`（+ 自证头读 `MIR2_RNG_SEED`）；**关闭时值序列逐字节不变**（用例 + 改坏必红）。
2. **交错分析器**（`tests/parity/m3/rng-trace-probe/analyze_rng_trace.py`）：按 marker 逐击分段，给出
   「噪声是否夹在攻击路径两次取数之间」的机判（selftest 5/5）。
3. **决策片结论**（下 §2）+ **修法选项 (a)/(b)/(c)**（下 §3）。

## 1. 读数（原始件 → 文件）

| 文件 | 内容 |
|---|---|
| `01-csharp-ledger-summary.txt` | m3bag `attack_rng_ledger.py summary`：1,670,754 次取数，攻击路径 275，噪声 99.98% |
| `02-csharp-ledger-segments.txt` | 同上 `segment`：120 段 |
| `03-csharp-interleave.txt` / `04-csharp-interleave.json` | 本片分析器：**intra_noise_attacks=3/120；inter_attack_noise median=1777** |
| `05-csharp-noise-sites.txt` | 噪声 Top 站点（`AnimalObject.Wondering` 1.06M、`MonGetRandomItems` 195k、`Merchant.Run` 254k…） |
| `06-csharp-attack-interleave-excerpt.txt` | 攻击 #2/#3 原始行——噪声插在 `GetAttackPower@2934` 与 `_Attack@41` 之间 |
| `07-rust-trace-off-on-test.txt` | Rust trace 用例：off/on 值逐项相同 + 同形 + 改坏必红 |
| `08-rust-trace-single-threaded-interleave.txt` | Rust 离线单线程攻击路径：**intra_noise_attacks=0/2996，噪声 0** |
| `09-rust-trace-shape.txt` | Rust trace 前 14 行（site = `crates\world\src\combat.rs:NNN`） |
| `10-attack-rng-ledger-selftest.txt` | m3bag 账本 selftest 8/8 |
| `11..14-gates-*.txt` | 四门禁（见 §5） |

## 2. 决策片结论（owner 要的那一问）

**问**：噪声抽取有没有夹在攻击路径的两次取数之间？

**答：有，而且两个层次都有**（原始件 `03`/`06`）：

- **同一击内部**：120 击里 **3 击**（`#2`/`#3`/`#55`）在两次攻击取数之间夹了噪声。
  最极端 `#2`：`223799 GetAttackPower@2929 → 223800 GetAttackPower@2934 → 223801 AnimalObject.Wondering@427 → 223802 _Attack@41 → … → 223815 GetHitStruckDamage@792`，即**同一击内被怪 AI 插了 12 条取数**；
  `#3` 更典型：`225618/225619/225621/225623/225625/225627` 六步之间各夹 1 条 `AnimalObject.Wondering@427`。
- **击与击之间**：每两击之间的噪声取数 **min 807 / median 1777 / max 3284**（`>1000` 的 114/119）——
  由**其它地图**的 `AnimalObject.Wondering`（1.06M）、`MonGetRandomItems`（195k）、`Merchant.Run`（254k）
  等**并发消费者**产生（0137 本身零刷怪，但全局随机源被整服其它系统共享）。
- **对照（Rust 离线单线程攻击路径）**：`intra_noise_attacks=0/2996`、噪声 0、攻击取数全连续
  （`08`）。⇒ Rust 生产路径**没有**这些消费者，两侧 RNG 流位置必然分叉（`attack_diff.py` 120 击差 83 项）。

**⇒ 对 (a)「攻击窗口前两侧同步重播种」的判定**：**不足**。重播种能消掉「击与击之间」的 ~1777 条噪声，
但**消不掉同一击内部那 3/120 的交错**（怪 AI 与玩家攻击在 C# 是不同线程并发抽同一个全局 `System.Random`
——`GetAttackPower` 后到 `_Attack` 之间就有别的线程插入）；且重播种本身是**对 oracle 语义的偏离**
（真 C# 服务器从不重播种），需要两侧在「同一逻辑时刻」触发，属夹具级改造而非实现级修复。

**⇒ 对 (b)/(c) 的判定**：要真正让**逐 k 可比**，只有两条路——(b) 把 oracle 侧全部非攻击消费者
1:1 建模（下 §3 缺口清单），或 (c) 把测量窗口隔离到「全局随机源只有攻击路径在抽」。

## 3. 修法选项（各写代价与影响面，不替 owner 决定）

### (a) 攻击窗口前两侧同步重播种
- **可行形态**：C# 侧 `rng-hook` 是**夹具/工具链写集**（非冻结 `src/**`）⇒ 可加一个「哨兵文件/管道触发重播种」
  （钩子已有 `RecordingRandom`，加一个 `Reseed(seed)` 即可）；Rust 侧 `crates/shared/src/rng.rs` 加
  「运行时重播种」也容易。触发点需两侧同步（例如每击前 bot 发一条哨兵，两侧各自 reset）。
- **代价/影响面**：属**夹具语义改造**（登记 `T-*`），不是实现修复；需保证双方在**同一逻辑时刻**触发。
- **硬结论**：**不足以把 DoD② 拉到 0**——`#2/#3/#55` 这类同击内交错仍会让该击取值分叉（实测 3/120）。
  除非同时把所有并发消费者静默（那已经是 (c)）。

### (b) 把 oracle 侧非攻击消费者在建世界时 1:1 建模
- **缺口清单（按量级，`05`）**：
  | 消费者 | 次数 | Rust 现状 |
  |---|---|---|
  | `AnimalObject.Wondering@AnimalObject.cs:427/432/434` | 1,060,321 + 53,169 + 13,384 | 未建模（Rust 只跑 pin 图 0137；其它图 AI 未对齐） |
  | `WorldServer.MonGetRandomItems@MonGen.cs:451/467` | 195,913 + 9,183 | `crates/formula::drop` 有掉落公式，但**刷怪/地图生成**的消费次序未对齐 |
  | `Merchant.Run@Merchant.cs:132/138` | 128,717 + 126,096 | 未建模（NPC 商人 AI） |
  | `WorldServer.RegenMonsters@MonGen.cs:815` / `CreateMonster@MonGen.cs:740` | 13,774 / 6,936 | 未建模（刷怪 tick） |
  | `AnimalObject..ctor@AnimalObject.cs:75`、`BaseObject..ctor@BaseObject.cs:438/441` | 14,180 / 7,090 / 7,090 | 未建模（构造期抽数） |
- **代价/影响面**：等于把「哪些东西会抽随机数、按什么 tick 顺序抽」**全对齐**——是**数周到数月的独立里程碑**，
  且要复刻 C# 的**多线程交错次序**（同击内也有并发插入，见 §2）。
- **硬结论**：原则上能做到，但成本远超 ②③ 收口本身；不建议为收口一条 DoD 而开此口子。

### (c) 隔离测量窗口（本片倾向）
- **(c1) 离线单线程探针口径**：`tests/parity/m3/attack-hit-probe` / `combat-probe` 已是
  「固定种子 → 反射直调 oracle 攻击路径」的**零噪声、单线程**口径；Rust 侧对应用例
  `crates/world/tests/combat_skill_vectors.rs`（本片实测 trace 全连续）。⇒ 把 DoD② 的
  「逐 k 取值」判据落在**探针口径**（而不是真机全局 RNG），既确定性又可 1:1 对拍。**注意**：这等于
  **改判据口径**，按 §6.0-3 必须 **owner 签字登记**，不许自行改。
- **(c2) 真机零消费者**：需让整服（不只是 pin 图）在测量窗口内没有其它 RNG 消费者——GameSvr 默认加载
  全部地图/NPC/刷怪，内容冻结下只能靠运行期开关「静默其它系统」，属对 oracle 的运行期偏离；
  且仍不解决「同击内跨线程插入」（若玩家线程与其它系统线程并行）。可行性低。
- **(c3) 保留真机逐 k、但接受非 0**：按 §6.0-3 登记「真机逐 k 因全局 RNG 共享而不可达，以 (c1) 探针口径为准」。

## 4. 两侧账本 `diff` 的当前可跑性

m3bag 的 `attack_rng_ledger.py` 的 `is_attack` 正则是 **C# 站点名**，`segment --marker` 需按侧给 marker。
本片已补 Rust 同形 trace（`09` 实测 site = `crates\world\src\combat.rs:NNN`）。要让 m3bag 账本直接
`diff` 两侧，只差**两处小改**（都属 m3bag 写集，本片只读不写）：
1. `ATTACK_PATH_RE` 支持 Rust 站点（或加 `--attack-re` 参数）；
2. `--side {csharp,rust}` 给缺省 marker（C# = `…GetAttackPower@PlayObject.cs:2929`；Rust = `crates\world\src\combat.rs:109`）。

在此之前，本片的 `rng-trace-probe/analyze_rng_trace.py` 已按 `--attack-re`/`--marker` 参数化，两侧都能跑
（`03` 为 C#，`08` 为 Rust）。⇒ 请 owner 决定：是**转 m3bag 加参数**，还是**把本片分析器收编为判据**。

## 5. 四门禁（隔离 `CARGO_TARGET_DIR=E:/tmp/m3cs-rng-trace-target`，`--offline`）

- `cargo fmt --all -- --check` → rc=0（`11`）
- `cargo clippy --workspace --all-targets --offline -- -D warnings` → rc=0（`12`）
- `cargo test --workspace --offline` → **82 suites / 669 passed / 0 failed / 16 ignored**（`13`）
- `cargo run -p msgcodegen --offline -- --check` → **604**（`14`）

## 6. 残留物清单

- 仓外原始输入（未入库，可复算）：`E:/tmp/m3cs-tick/csharp-stack/rng-stream.tsv`
  （135 MB，sha256 `718c23df…`，源自 m3bag 会话 `csharp-20261012-170110`，seed=42）；
  `E:/tmp/m3cs-trace-single.tsv`（本片 Rust 离线 trace，可复算）。
- m3bag 账本脚本（未合入 master，本片只读执行）：`E:/tmp/attack_rng_ledger.py`
  （sha256 `70b0620d…`，= m3bag commit `40166a82` 的 `attack_rng_ledger.py`）。
- 隔离 target：`E:/tmp/m3cs-rng-trace-target/`。
- 未起栈、未占端口（收工十端口实拍见 patch body）。
