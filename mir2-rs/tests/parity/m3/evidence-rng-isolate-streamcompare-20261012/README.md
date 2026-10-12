# M3②③ 隔离口径 · 两侧专用流逐次比对 —— 证据包（2026-10-12，m3cs / openmir2-worker-5）

- 卡：`openmir2-m3-combat-skills-align`（issue `a2ef74aa`）· worktree `m3cs` / branch `worktree-m3-combat-skills`
- 本片回答 owner 2026-10-12 **22:15** brief 顶部第 ①–④ 条（离线段，**零端口、零栈、零 `.rs`**）。
- 被审/引用 head：C# 半 = `7d3fb333`（同卡 m3bag，待审未合）；Rust 半 = `b9e37389`（已合 master `50fc02bd`）。
- 工具：`mir2-rs/tests/parity/m3/rng-isolate-attack/compare_dedicated_streams.py`（本片新增；口径唯一事实源 = 同目录 `attack_sites.tsv`）。
- 命令：`commands.txt`。

## 结论（一句话/条）

1. **① 隔离真的把两侧放在同一条流上 ✓**：两侧攻击路径专用流的**逐次结果值**对**各自的 `(kind,arg)` 请求序列**
   用逐行复刻的 .NET legacy `System.Random(40545)` 自复算，**各 0 mismatch**（`07`）。⇒ 种子公式/分流谓词/算法都对齐。
   逐 k ≠ 0 的根因**不是**隔离不等价，而是**两侧请求序列不同**（第一处分歧 index=2）——即被测行为差。
2. **② 攻方属性判别：无「输入差」**（`04`）。命中门 `HitPoint < RandomByte(target.SpeedPoint)`
   （oracle `BaseObject.Attack.cs:41`）的两侧输入：`SpeedPoint=15`（相同）；`HitPoint`：
   **Rust 严格反解 = [5,6) ⇒ 5**；C# 3 个门样本区间 [4,10) 含 5（stack1 交叉印证 [3,7) 亦含 5）。
   ⇒ 命中门本体两侧等价；分岔在**「到达命中门的攻击数」**（Rust 38/120 vs C# 3/120）= 目标解析/Dir 入口差
   （m2-svr 已在 `bins/game-svr` 写集登记）。
3. **③ `atk_power_base`（C# 16 / Rust 0）不是夹具攻击路径**（`05`）：oracle 里基版 `GetAttackPower`
   （`BaseObject.Base.cs:886/892`）只被**非 PlayObject actor** 命中（`PlayObject.cs:2909` 是 override）。
   0137 无怪/无 NPC，但 C# 影子 hook 的 divert 是**调用点级、进程级**（覆盖整个 GameSvr 与所有地图）
   ⇒ 这些单元来自**并发实体**（多目标 SpeedPoint∈{8,13,15}、基版攻方 HitPoint≥13，且计数随运行变化 16/23/34）。
   ⇒ **夹具/仪器口径**（m3bag 写集）：C# 专用流需按会话/actor 收敛，或比对时只取 player 单元。
4. **④ `-AttackVictimHp 30000` 恒被登录序重算覆盖**（`06`）：pin 写的是 DB `characters_ablity.Hp/MaxHP`，
   但进世界 `RecalcLevelAbilitys()`（`PlayObject.cs:5295`，战士 L1 公式 + 默认 4 / 4.5）把 `Abil.MaxHP` 重算为
   **19**，`RecalcAbilitys()` 再写 `WAbil.MaxHP = Abil.MaxHP + AddAbil.HP` ⇒ 受害者 maxhp=19（真机读数一致）。
   ⇒ 属夹具 bug（m3bag）；本片只登记 + 给 oracle 行号。

**判据口径未动、未放宽**：DoD② 仍是真机 `attack_diff.py diff` 逐 k=0；本片**不宣布达成**。

## 文件

| 文件 | 内容 |
|---|---|
| `01-selftest.txt` | 工具牙齿（值/请求序列/HitPoint 区间，GREEN 9/9） |
| `02-analyze-stack2.txt` + `.json` | 主口径（cA vs rA，各 120）：计数 / 自复算 / unit / HitPoint / 第一处分歧 |
| `03-analyze-stack1.txt` + `.json` | 交叉印证（含 20 击冒烟） |
| `04-attribute-discrimination.txt` | owner 22:15-② 逐门读数与 HitPoint 区间 |
| `05-base-attacker.txt` | owner 22:15-③ base 单元逐条 + 判读 |
| `06-victim-hp-pin-oracle.txt` | owner 22:15-④ oracle 归因 |
| `07-same-stream-proof.txt` | owner 22:15-① 「同一流」机判（自复算 0 mismatch） |
| `08-unit-structure-stack2.txt` | 每击取数结构（unit → ops，含每击第几次） |
| `provenance/` | 四条原始流水（C# 专用流 ×2、Rust 流水 ×2）+ 00-provenance 溯源 |
| `sessions/` | cA / rA 会话件（attack_seq / attack_isolation / baseline / bot.log）用于 ④ 的读数 |

## 溯源

- 四条流水 sha256 / 字节见 `provenance/00-provenance.txt`；原件与 `E:/tmp` 的对应关系见 `commands.txt`。
- `SHA256SUMS.txt` 按**仓库字节**（LF 归一）口径；`verify_sha256sums.py --check-extra` GREEN。
