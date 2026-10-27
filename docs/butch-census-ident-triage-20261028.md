# butch_two_draws 逐 ident 普查分诊（2026-10-28）

卡：`openmir2-butch-census-ident-triage`；级别 **R**，`window_required=true`。

本文只分诊 run4 的 `10 / 41 / 11 / 14`，不把差异笼统归给 656。656 归
`openmir2-sm-changenamecolor-656`；14 归 `openmir2-ignore-rule-count-compare`，本文只引用其
PENDING 结论，不转正、不重判。201 只用于解释对齐级联，归
`openmir2-bagitems-201-ordering`。

**结论边界**：本轮没有确认新的 Rust 实现回归，因此没有开实现修复卡；但也**没有证明
逐坐标、逐次数或 RNG 流位等价**。run4 没有 `--rng-log`，这些面仍为 R 级未决。随附的
`tools/window/rules-butch-census-ident-triage.draft.json` 是证据与判据草案，
`active=false`，不是 `compare_runs.py` 可加载的规则表，不改变任何对拍结果。

## 1. 证据与复现

主证据：

- `E:/tmp/window/run4/butch_two_draws/{csharp,rust}/frames.ndjson`；
- `E:/tmp/window/run4/butch_two_draws/report.json`；
- 重复运行对照 `E:/tmp/window/run3/butch_two_draws/`；
- 全量逐帧时间线、参数频次、输入 SHA 与判据草案：
  `tools/window/rules-butch-census-ident-triage.draft.json`。

run3/run4 的场景 SHA 都是
`23558a94d8955a31c6eede719900c52a6eed7796d5b0371dd977b52ac8f0c065`；两侧二进制 SHA
也逐项相同（C# `ab655008…392f`，Rust `fca9e8cd…a31`）。因此两轮计数变化不是换二进制或
换场景造成的。

离线重算（在仓库根执行；不写窗口产物）：

```sh
python -I -B tools/conformance/compare_runs.py compare \
  --rust E:/tmp/window/run4/butch_two_draws/rust \
  --oracle E:/tmp/window/run4/butch_two_draws/csharp \
  --rules tools/conformance/rules.json \
  --out E:/tmp/dev-2/butch-run4-recompare.json --quiet
```

结果 `rc=2 / UNDECIDABLE`，`frames.compared=28`、`diffs=90`，与 run4 报告一致；两个
`undecidable` 是 656 UNCLASSIFIED 与 14 PENDING。规则 SHA
`36ee6446c1caf3e39255f5f722d27593fcb11b69bbddedd6c632acfa3b1d4a13`，56 条 ident 规则。

逐帧清单生成命令与文本输出：

```sh
python -I -B E:/tmp/dev-2/analyze_butch_census.py \
  > E:/tmp/dev-2/butch_census_timelines.txt
python -I -B E:/tmp/dev-2/build_butch_triage.py
```

## 2. A1：逐 ident 时间线与参数分布

下表的 seq/t_ms 是各侧驱动器自己的观测坐标，**不拿两侧墙钟值逐值比较**。完整的逐帧
`seq,t_ms,phase,recog,param,tag,series,body_len,body_sha256` 和全部频次在随附 JSON 的
`timeline` 键；此处保留首末、间隔和主要分布。

| ident | 侧 | 条数 | 首帧 seq@t_ms | 末帧 seq@t_ms | 主要参数分布 |
|---|---|---:|---:|---:|---|
| 10 SM_TURN | C# | 40 | 27@2427 | 146@10146 | body_len `19:28 / 11:12`；series `0:4,1:2,2:5,3:7,4:7,5:5,6:7,7:3` |
| 10 | Rust | 28 | 30@1824 | 84@1825 | body_len `19:28`；series `0:2,1:1,2:2,3:2,4:3,5:3,6:6,7:9` |
| 41 SM_FEATURECHANGED | C# | 42 | 15@2419 | 148@10548 | param `0:1,11:40,512:1`；tag `2:2,160:40`；series 全 0 |
| 41 | Rust | 30 | 15@822 | 99@2341 | param `0:1,11:28,512:1`；tag `2:2,160:28`；series 全 0 |
| 11 SM_WALK | C# | 12 | 92@2696 | 155@11643 | body_len 全 11；x=325..334，y=325..335；9 个 recog |
| 11 | Rust | 19 | 27@822 | 166@19961 | body_len 全 11；x=323..339，y=322..333；14 个 recog |
| 14 SM_HIT | C# | 1 | 120@7697 | 120@7697 | `(param,tag,series,body_len)=(330,331,0,0)` |
| 14 | Rust | 4 | 107@6032 | 148@16855 | 四条均 `(329,330,2,0)`，同一 recog |

间隔：

- **10/C#**：27 个 0ms 后是 `1173,388,2027,0,774,299,1351,611,488,0,608,0`；
  **10/Rust**：26 个 0ms、一个 1ms。
- **41/C#** 非零间隔为 `8,1173,388,2027,774,299,1351,611,488,608,402`，其余为 0；
  **41/Rust** 非零间隔为 `1002,1,516`，其余为 0。
- **11/C#**：`295,297,2727,2424,312,1100,0,697,0,814,281`。
- **11/Rust**：`0,1201,4009,202,2404,1803,200,2405,1204,2005,600,602,802,0,894,107,701,0`。
- **14/Rust**：`3607,3607,3609`；C# 只有一条，无间隔。

补充时间面：C# 捕获末帧 t=11643，Rust 末帧 t=19961；C# 击杀用 2 次 `CM_HIT`，Rust 用
7 次。随机游走被采到的时长和击杀链长度不同，裸计数 `12 vs 19` 不能单独当回归判据。

## 3. A2：逐 ident 定性、发射点与去向

### 3.1 ident 10（SM_TURN）：随机对条数差 + 既有 201 对齐差；具名坐标仍 R

拆解不是“C# 多 12 条普通 TURN”：

| 形状 | C# | Rust |
|---|---:|---:|
| 具名出现帧（body_len=19） | 28 | 28 |
| 无名随机转身（body_len=11） | 12 | 0 |
| 合计 | 40 | 28 |

12 条无名 TURN 每条都紧随同 recog 的 41，命中既有
`rules.json.random_branch_pairs`；报告明确给出 `oracle_pairs=12 / rust_pairs=0 /
shape_mismatch=false`。摘出后 28 条具名出现帧两侧都在，body SHA 也同为
`d82d8b52…ad73`。

发射点：

- oracle AOI 具名出现 `src/M2Server/Player/PlayObject.cs:3240`，RM_TURN 下行
  `src/M2Server/Player/PlayObject.Message.cs:1524-1560`；
- Rust 对应 `mir2-rs/bins/game-svr/src/downlink.rs:336-348`（具名）与 `:358-364`（无名）。

但 28 对初始帧的**顺序和坐标仍不能直接豁免**：

1. run4 的 Rust 201 在初始 28 对之前，C# 201 在之后，造成后续 ident 对齐级联；201 的
   T-184 判据补登合并于 `086889f54`，而 run4 是 `1d9098cfa`，不含该补登，归既有 201 卡。
2. 两侧 fixture 的 MonGen 后 SHA 都是 `82f63f0a…331c5`，原始刷怪格都在 329..331；但具名
   TURN 已有 C# 17/28、Rust 19/28 离开这些格。坐标是登录/AOI 前随机游走后的落点，
   run4 无 RNG 流水，不能据此判等价，也不能据此开确定性坐标回归。

**去向**：随机对次数差按既有规则；201 由专卡复拍；具名坐标保留 R，需 RNG 流位或静止怪
对照。禁止整类 ignore 10。

### 3.2 ident 41（SM_FEATURECHANGED）：10 的配套帧，无独立缺帧证据

41 可完整拆成：怪物配套 `param=11,tag=160` 为 C# 40 / Rust 28（= 28 具名 + 12/0 随机对），
玩家自身 `tag=2` 为两侧各 2；series 全 0、body_len 全 0。多出的 12 条与 10 一一成对，
没有独立的第三类 41 差异。

- oracle：`src/M2Server/Player/PlayObject.Message.cs:1552-1560`；
- Rust：`mir2-rs/bins/game-svr/src/downlink.rs:336-364`，编码器
  `mir2-rs/bins/game-svr/src/smframe.rs:144`。

**去向**：随机对归一化与 10 共用；具名出现次序/坐标余项随 10 保留 R。禁止整类 ignore 41。

### 3.3 ident 11（SM_WALK）：随机移动来源成立；形状可比，坐标/存在性未决

两侧所有 11 的 body SHA 都同为
`315462a487746ddb31a1e403c2ae61322a454431530d53fffba9d7b9be48365e`。同一组二进制和场景
下，C# 的 11 从 run3 的 4 条变为 run4 的 12 条，无名 TURN 对从 7 对变为 12 对；这直接
反驳“固定实现必出固定条数”的读法。

随机源与移动支：

- oracle `src/M2Server/Actor/AnimalObject.cs:425-441`：`Random(20)` 后 `Random(4)`，
  一支 `TurnTo(RandomByte(8))`，另一支 `WalkTo(Dir)`；
  `src/OpenMir2/RandomNumber.cs:35` 是墙钟播种 `new Random()`；
- Rust `mir2-rs/crates/world/src/monai.rs:232-245` 同分支，种子在
  `mir2-rs/crates/world/src/world.rs:1343-1344`；跨进程流位并不相同；
- oracle 下行 `src/M2Server/Player/PlayObject.Message.cs:1403-1410`；
- Rust 位置差还原与下行 `mir2-rs/bins/game-svr/src/lib.rs:15443-15558`，编码
  `mir2-rs/bins/game-svr/src/downlink.rs:384-397`。

当前 ident 11 规则只掩 series，仍按顺序比较帧存在性和 param/tag；在本场景随机游走下，
这个口径会把 RNG 结果当帧差。**这只证明现有裸比较口径不适用，不证明 Rust 的移动次数和
落点等价。** 另两侧各有 1 个 WALK actor 没有具名 TURN，不能拿“出现过具名 TURN 的
recog”当白名单吞帧；广播范围/AOI 边界尚未分开。

**去向**：只交付 `DRAFT_NOT_ACTIVE` 的形状多重集草案（§4）；激活前必须补 RNG 流水/流位
锚或静止怪对照，以及 Wondering→生产 drain 的存在性构造牙。坐标与存在性继续 R，不整类
ignore 11。

### 3.4 ident 14（SM_HIT）：专卡保持 PENDING

run4 为 C# 1 / Rust 4，形状也不同；比较器按既有 PENDING 规则正确给出
`pending_ignore_rule`，没有被静默吞掉。本文不重复 dev-1 的归因与转正裁决。

- oracle 攻击发射 `src/M2Server/Actor/AnimalObject.cs:169-172` →
  `src/M2Server/Actor/BaseObject.Attack.cs:173-176`，下行
  `src/M2Server/Player/PlayObject.Message.cs:1433-1440`；
- Rust 怪物 RM_HIT 下行 `mir2-rs/bins/game-svr/src/lib.rs:19966-19976`。

**去向**：`openmir2-ignore-rule-count-compare`；本场景维持 UNDECIDABLE，不用
`--ignore-idents 14` 图绿。

## 4. 判据草案与负对照（未生效）

机读原文在随附 JSON 的 `draft_criteria` / `negative_controls_required`。核心约束：

1. **10/41 随机对**复用既有 `random_branch_pairs`；提取后未分类残差必须做**两侧对称差**，
   具名 10 和玩家 41 不得被提取。
2. **11 只允许草案级形状比较**：匹配 `ident=11 + phase=gamegate + body_len=11`，比较
   `body_sha256` 多重集；`(oracle-rust) ∪ (rust-oracle)` 必须为空。未匹配帧留在普通流，
   单侧残差必红。
3. **任一侧空集 ⇒ UNDECIDABLE**，不允许空输入 vacuous PASS；不允许只查“oracle 的键是否
   仍在 rust”这种单向包含。
4. 改坏一侧 body SHA ⇒ 形状对称差非空、必红；加入 `body_len!=11` 或非 gamegate 的 11 ⇒
   不得被提取、必须留普通流红。
5. 删除一条已匹配随机 WALK 只登记次数差，不得冒充形状差；但激活判据仍需 RNG 证据。
   本草案**不判**条数、param/tag、series/Dir、recog、t_ms，也不能拿它给这些面收口。
6. 具名初始 TURN 保持普通比较且当前为 R 未决；不得为了“新判据全绿”豁免 param/tag。

这些负对照是后续实现/审查必须重放的要求，**本卡没有声称它们已在生产比较器实现**。

## 5. A3：诊断性重算（不是放行规则）

为拆开“随机帧本身”与“顺序对齐级联”，额外做了两次诊断，只写到 `E:/tmp/dev-2`：

| 诊断 | 输出 | compared / diffs | 读法 |
|---|---|---:|---|
| 原规则 | `E:/tmp/dev-2/butch-run4-recompare.json` | 28 / 90 | 复现 run4 |
| 仅诊断 `--ignore-idents 11` | `E:/tmp/dev-2/butch-run4-recompare-no11.json` | 35 / 71 | 10/41 仍各 28 个 ident 差 ⇒ 11 不是唯一原因 |
| 仅诊断 `--ignore-idents 11,201` | `E:/tmp/dev-2/butch-run4-recompare-no11-no201.json` | 91 / 55 | 10/41 的 ident 错位消失，露出 10 的坐标差 ⇒ 201 对齐是独立因素 |

后两条命令只是负对照，**不是交付判据，也不证明被剔除帧等价**。它们防止把 90 条级联
diff 误读成 90 个独立实现缺陷。

## 6. 剩余 R 窗口项与交付边界

- 用当前 master 重跑 `butch_two_draws`，携带 RNG 流水/流位锚；另用静止怪或等价可控对照
  分开“随机存在性”与“确定性移动/坐标”。没有这类证据时，坐标与存在性仍是 UNDECIDABLE。
- 复跑需同时核对既有 201 新判据、656 专卡、14 专卡；本卡不替它们收口。
- 后续若实现 §4 的比较器草案，必须逐条重放 §4 负对照，并证明生产构造牙真实执行；草案
  未实现前不得把 `active=false` 改成 true。
- 本批只新增本文与随附 JSON；不改 `src/**`、`mir2-rs/**`、场景 JSON、现行
  `tools/conformance/rules.json` 或任何生效 ignore。
