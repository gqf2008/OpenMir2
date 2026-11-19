# openmir2-deps-build-config-pinning

> 状态：**已实施（2026-11-19，分支 `card/deps-build-config-pinning` → M9）**。

## 现象（今天的实例）

`tools/deploy/deps_manifest.py` 生成 manifest 时用 `detect_output_dir()` **按 mtime 取最新的
`bin/<config>`**（`Release` 优先遍历、但最终由 mtime 决定），把结果写进 `output_dirs`；
`check` 再**重新探测一遍**并与 manifest 记录的目录比对，不等即 `BUILD_OUTPUT_MISMATCH`。

于是：

- `m1-stack/deps/*.deps.json` 两份最初是在 **Debug** 口径下生成的；
- 部署走 Release（`start-m1-stack.ps1` 从 `src\DBSrv\bin\Release` 刷新影子），只要主仓库出现
  一次 Release 构建（今天 M7 的门禁构建就是），探测结果翻成 Release；
- `check` 立刻判红：

```
DEPS_RED BUILD_OUTPUT_MISMATCH ...DBSrv.csproj manifest=...\bin\Debug now=...\bin\Release
DEPS_RED_TOTAL 1                                       (rc=1)
```

而 `start-m1-stack.ps1` 的 3c 段 fail-closed ⇒ **整栈起不来**，与被检内容是否真有问题无关。

## 要求（opc-07）

1. `check` 增加**显式** `--build-config <Debug|Release>`（或从 manifest 读 `build_config` 并**强制**
   用它解析输出目录），不再让"最新输出目录"这种隐式行为成为判据；
2. **含糊时 fail-closed**：`--build-config` 与 manifest 不一致、或两者都缺/无法确定 ⇒ 红（rc≠0），
   不许静默挑一个；
3. 生成侧同步：`generate --build-config` 写死口径；manifest 里 `build_config` 与 `output_dirs`
   必须自洽；
4. 卡面写清今天的实例（Debug 构建落地 ⇒ 旧 manifest 立刻 `BUILD_OUTPUT_MISMATCH`、整栈起不来）。

## 边界

- 只动 `tools/deploy/deps_manifest.py`（+ 自测/牙）；**启动器无需改**（不传 `--build-config`
  时按 manifest 记录的 `build_config` 解析，正是现有调用形态）；
- 不改 manifest 的包字段口径、不重下包、不动戳门禁。

## 验收（实施时）

- 牙：① Debug 目录 mtime 更新但传 `--build-config Release` ⇒ 判据不变、不红；
  ② `--build-config Debug` 对 Release 口径 manifest ⇒ 红；
  ③ 缺参且两目录并存 ⇒ fail-closed；
- `deps_manifest selftest` 扩充并进工具门（TOTAL 变化按口径记）；
- 真实两份 manifest（DBSrv 46 包 / LoginSrv 48 包）复跑 `DEPS_GREEN`。

## 连带要求：构建与产戳必须同一步（2026-11-19 窗口作废事故）

同一天 `RUN-START aoi-default-verify` 窗口**作废**（零轮次数据），根因不是本卡的
`build_config`，而是它的邻座问题——**"重建未重产戳"**：

- 主检出 `src/DBSrv/bin/Release` 在 18:22 的 Release 构建后被替换（C# 件嵌 `HEAD` 的
  `InformationalVersion`，M7→M8 换 head 即变字节，`OpenMir2.dll` 245,248 → 245,760 B）；
- 但 `.build.json` 仍是 03:45/18:02 的旧戳（sha/size 对不上新件）；
- `-RefreshRuntime` 把**未盖新戳**的件灌进影子，启动闸门按 `SHA_MISMATCH` 正确拦下 ⇒ 窗口作废。

**要求（与本卡同批落实）**：
1. 任何"产出部署件"的构建，**构建与产戳必须是同一步**（同一次调用内先 build 后 stamp，
   或构建后立刻跑 `stamp-binaries` 并把"戳齐"作为构建成功的判据之一）；
2. 不许只补戳不重建（等于给来历不明的件盖章）——重建用 `--no-incremental`；
3. 闸门保持 fail-closed，不要为了"让栈起来"放宽 `SHA_MISMATCH`/`SIZE_MISMATCH`；
4. 若做自动化，回执里必须给出"构建 head == 戳 head == 期望 head"三处对齐的原文。

## 背景实例（M7，2026-11-19）

- 已在 M7 上用 Release 口径重生成两份 manifest 并双双 `DEPS_GREEN`（DBSrv 46/1030、LoginSrv 48/1063）；
- 该处置只是把口径拨正，**脆弱点仍在**：下一次 Debug 构建又会翻回去。

## 实施（2026-11-19）

### 代码（`tools/deploy/deps_manifest.py`）

- **删掉 `detect_output_dir()` 的 mtime 语义**，改为 `resolve_output_dir(proj_dir, config)`：
  按**显式配置**确定性解析——`bin/<config>` 本身含 DLL ⇒ 取它；否则必须**恰好一个**含 DLL 的
  子目录（`bin/<config>/<tfm>`）；其它情况返回 `None`，由调用方 fail-closed 判红。
- `generate`：新增 `--build-config Release|Debug`；缺省按**固定优先级** `Release → Debug`
  取**存在者**（只看存在性，不看 mtime）；解析不到 ⇒ `DEPS_RED MISSING_OUTPUT_DIR`（rc=2）；
  无法从布局确定口径 ⇒ `DEPS_RED BAD_BUILD_CONFIG`（rc=2）。
- `check`：新增 `--build-config`；manifest 的 `build_config` 必须是 `Release`/`Debug`，
  否则 `BAD_BUILD_CONFIG` 红；命令行与 manifest 不一致 ⇒ `BUILD_CONFIG_CONFLICT` 红；
  按口径解析不到目录 ⇒ `MISSING_OUTPUT_DIR` 红；解析结果与 manifest `output_dirs` 不一致 ⇒
  `BUILD_OUTPUT_MISMATCH` 红（现在两边都是**确定性**解析）。**判据里不再有任何 mtime 比较。**
- 自测 21 → **28 例**（+7：口径冲突、反 mtime 探测、坏/缺口径 ×3、目录缺失、生成侧显式 Debug）。
  牙钉的是**诊断码**而不只是 rc——否则 `MISSING_OUTPUT_DIR` 也能把 rc 弄成 1，冒充"冲突红"。

### 验证（真实 manifest，Release 口径）

```
check  （缺省，走 manifest 口径）→ DEPS_GREEN packages=46 files=1030   (DBSrv)   rc=0
check  （缺省，走 manifest 口径）→ DEPS_GREEN packages=48 files=1063   (LoginSrv) rc=0
check --build-config Debug        → DEPS_RED BUILD_CONFIG_CONFLICT manifest=Release args=Debug
                                    DEPS_RED_TOTAL 1                        rc=1
```

### 变异（每次恢复后源文件 sha 相等，`RESTORE_SHA_OK`）

| 变异 | 结果 |
|---|---|
| `resolve_output_dir` 退回按 mtime 挑最新 | `buildcfg: no mtime probing (newer Release must not flip)` 等 3 例红（`newer=True cfg='Release' rc=1`） |
| 去掉 manifest 口径合法性判据 | `buildcfg: bad/absent build_config red ('')` 与 `('unknown')` 红；原文显示落到了 `MISSING_OUTPUT_DIR` ⇒ 证明牙钉的是诊断码 |
| 去掉 cli/manifest 冲突判据 | `buildcfg: cli config conflict red` 红（`DEPS_RED MISSING_OUTPUT_DIR … 按 Release 口径解析不到`） |

## 事故与交叠（2026-11-19，按 opc-07 要求留证）

三条实例（本卡的直接动因）：

1. **`BUILD_OUTPUT_MISMATCH`（Debug/Release 并用）**：两份 manifest 在 Debug 口径生成，M7 门禁
   的 Release 构建落地后，`detect_output_dir` 翻成 Release ⇒ 检查判红、`start-m1-stack.ps1`
   的 3c 段 fail-closed ⇒ **整栈起不来**，与被检内容无关。
2. **「重建未重产戳」两次使窗口作废**：18:22 主检出 `bin/Release` 被 Release 构建替换，
   侧车戳仍是旧件的 sha/size；`-RefreshRuntime` 把未盖新戳的件灌进影子，闸门按 `SHA_MISMATCH`
   正确拦下 ⇒ `RUN-END aoi-default-verify`（零轮次）作废；重建重产戳后 `aoi-default-verify-b`
   才跑成。
3. **C# 产物内嵌 HEAD**：`OpenMir2.dll` 字节里能直接找到 `a5643481` 字面量；换 head 即换字节，
   甚至换尺寸（245,248 → 245,760 B）⇒ 每次换 head 的构建**必须重建重产戳**。

**交叠（留证、不判污染）**：C# 重建的构建在 **18:31:49** 结束（早于 `RUN-START aoi-default-verify-b`
的 18:32），产戳/影子拷贝在 **18:32:10**、卡提交在 **18:32:26**，跨过了窗口起点；botfleet 的
窗口起点快照是 **18:33:06 = `(none)`**，栈未起 ⇒ **交叠只覆盖准备阶段，没有覆盖任何压测帧**。

**规程**：重建 C# 并产戳这类活**只指派一个 owner**；另一会话收到「已由 X 完成」即停手。
（当天我与 openmir2-ca 同时执行了同一件事，产出 9 件 sha 完全相同 ⇒ 确定性构建、无副作用。）
