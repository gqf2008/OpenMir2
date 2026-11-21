# 装置缺陷：`EnsureConfFiles` 只在目标缺失/0 字节时拷贝（换 conf 目录会静默沿用旧配置）

> **状态：仅登记（docs-only），本卡不改代码。** 修复落在
> `mir2-rs/tests/parity/csharp/MagicGolden/Program.cs`，与 `items-it13-37` 的 MG-42 正在改同一文件，
> 必须串行（等 MG-42 合入后再开分支）。
> 发现于 [`magic-serverconf-nondefault-arm-plan.md`](magic-serverconf-nondefault-arm-plan.md) §5：
> 策划"非缺省值区分支"时发现，任何**换 conf 目录**的对照臂都会被这条缺陷变成假绿。

## 1. 现状（`MagicGolden/Program.cs` 的 `EnsureConfFiles`）

```csharp
foreach (var n in ConfNames)
{
    var src = Path.Combine(confDir, n);
    var dst = Path.Combine(baseDir, n);
    if (!File.Exists(src)) { missing.Add(src); continue; }
    // 冻结配置必须非空：0 字节的旧副本（构建产物/读回写坏）会让  全 0 ⇒ 重拷。
    if (!File.Exists(dst) || new FileInfo(dst).Length == 0) { File.Copy(src, dst, overwrite: true); copied.Add(n); }
}
```

`ConfNames = { Server.conf, String.conf, Exps.conf, Global.conf, setting.conf }`；
`confDir = MAGICGOLDEN_CONF_DIR ?? E:\MirServer\M2GameSvr`；`baseDir = AppContext.BaseDirectory`。

**判据只看"存在 + 非零长度"**：目标文件一旦存在且非空，就**永不重拷**，哪怕源目录换了、
源文件内容变了。

## 2. 失效条件

任意一条即可触发：

1. 同一 `bin/Release`（`AppContext.BaseDirectory`）先以目录 A 跑过一次，再以
   `MAGICGOLDEN_CONF_DIR=<B>` 跑（**本批的非缺省臂正是这个形态**）；
2. 生成源文件被就地修改（同路径、新内容），目标副本仍是旧的；
3. 两个 case 族需要不同的冻结配置，却共享同一输出目录。

## 3. 为什么现有夹具没暴露

- 既有全部 `--case` 都走**同一个** `MAGICGOLDEN_CONF_DIR`（缺省 `E:\MirServer\M2GameSvr`），
  源从不改变 ⇒ "存在即跳过"与"每次比对 sha"等价。
- 唯一换源的历史用法是 `spacemove` 的 `MAGICGOLDEN_MAP_DIR`（那是 **map 文件**，不走
  `EnsureConfFiles`，且在 `EmitGolden` 里现读现算 sha）。
- 因此在本卡之前，这条分支从未被任何夹具区分过。

## 4. 危害形态（比"红"更糟的是假绿）

1. **配置面假绿**：非缺省臂会静默用部署配置生成 golden，`server_conf` 落的是部署值，
   而 C# 侧 `LoadOrThrow` 若按"夹具件 sha"钉，会**立刻红在 sha**——但如果实现方把 sha 也
   按"目标副本"算，就会**自洽地绿**（`ServerConfSha256()` 的现状正是"优先读目标副本"）。
2. **行为面假绿**：两支（部署 vs 非缺省）如果读的是同一份旧副本，配置差异根本不存在，
   "区分支"退化成第二个同值臂。
3. **审计面**：`Fixture.server_conf_sha256` 记录的是**副本**的 sha，不是声明源文件的 sha，
   golden 无法自证"我来自哪个源目录"。

## 5. 修法（保持签名兼容）

把"存在即跳过"改成**内容身份比较**：

```csharp
var same = File.Exists(dst) && Sha256File(dst) == Sha256File(src);
if (!same) { File.Copy(src, dst, overwrite: true); copied.Add(n); }
```

- `missing` 语义不变（源不存在 ⇒ 仍加 `missing`、返回 false）。
- `copied` 变成"本轮真正写过的文件名"（同源重复运行 ⇒ 空列表）。
- 为了可测，建议把 `EnsureConfFiles(out copied, out missing)` 内部改为调用
  `EnsureConfFilesFrom(string confDir, string baseDir, out copied, out missing)`；
  公共入口仍用当前的 `AppContext.BaseDirectory` + 环境变量。
- `ServerConfSha256()` 的"优先读副本"保持不变（修复后副本与源同身份，两者等价）；
  **不要**反过来改成只读源——那会让 golden 无法证明"生成时用的是哪份副本"。

**副作用评估**：正常流程（同源不变）只多一次小文件 sha（5 个文件，`Server.conf` 58KB），
`copied` 从"首次非空、之后为空"变成"仅内容变化时非空"；对既有 39 份 golden 的生成结果
**零影响**（源与副本内容相同 ⇒ 拷贝与否输出一致）。

## 6. 两把牙（必须真的咬）

放在新增的 `--conf-selftest` 模式里（与 `--rng-selftest`/`--case fixture-selftest` 同风格），
用 `E:/UserTemp` 下的临时目录做 A/B 源与临时 base 目录，**不碰部署目录**：

| 牙 | 布置 | 期望 |
| --- | --- | --- |
| ① 换源必须重拷 | base 目录空 → 源 A 跑一次（`copied` 含 5 项）→ 改源为 B（内容不同、文件名同）再跑 | 第二次 `copied` **非空**，且 `base/Server.conf` 的 sha == B 的 sha |
| ② 改内容必须重拷 | 源仍是 B，但就地改 `Server.conf` 一个字节（sha 变化）再跑 | 第三次 `copied` 含 `Server.conf`，副本 sha == 新内容 |
| 对照（防"永远重拷"） | 源不变连跑两次 | 第二次 `copied` **为空**（证明比较的是身份，不是"每次无条件覆盖"） |

**验证修复的牙时**：把比较改回 `!File.Exists(dst) || len==0` ⇒ 牙①第二次拿到的是 A 的内容
（断言副本 sha == B 失败）⇒ 红；把比较改成无条件拷贝 ⇒ 对照片红（`copied` 非空）。
两条都咬住后再还原并复跑。

## 7. 影响面与写集

- 代码：`MagicGolden/Program.cs` 单文件（`EnsureConfFiles` + 新 `--conf-selftest` 分支）。
- 与各批次的交互：所有走 `EnsureConfFiles` 的 case 共用该入口；修复对**同源**流程等价。
- 与 MG-42（`items-it13-37`）**同文件** ⇒ 串行落地；本卡不抢写。

## 8. 关联

- [`magic-serverconf-nondefault-arm-plan.md`](magic-serverconf-nondefault-arm-plan.md)（§5 前置）
- [`magic-serverconf-batch-impl.md`](magic-serverconf-batch-impl.md)（配置批现状与 sha 语义）
- 记忆条目：`fixture-must-self-verify-invariants`（夹具要自证不变式）、
  `soften-channel-needs-mask-tooth`（软化通道要有牙）
