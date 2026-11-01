# 本夜真机轮交付报告（2026-11-01）

- 卡片：`openmir2-night-window-delivery-report`（tier=L，纯文档）
- 事实来源：真机台账线程 `openmir2-real-machine-verification-master`、四张卡线程、测试车道线程 `openmir2-tester-integration-loop`。文中 oid 与 run id 均为线程实读，交付前逐条 grep 对照。
- 交付时点 master：`aad30901ff994557806e8203533efc5e66d1b963`
- 口径：只陈述已取证事实；未取证项显式标「未取证/未覆盖」。本报告是交付记录，不是验收书。

## §1 四张卡

| 卡片 | tier | 审查者（approve 条目） | merge oid | 一句话目标 | 关键牙或变异 |
|---|---|---|---|---|---|
| `openmir2-noop-audit-stale-row` | L | reviewer-21（`5cf5a170`） | `b999f41397846dbcb622cdeec81914599e0b4b39` | 覆盖台账 `use_items_upper_bound` 行由 `needs_card` 同步为 `safe/overridden`（npc.rs:2355 已有覆写），并重钉 5 条 `evidence.line`，清合并门基线红 | 改回 `needs_card` ⇒ `check` RED 1；`evidence.line` 2358→2357 ⇒ `--strict-lines` RED 1；selftest 三颗牙各自期望红 |
| `openmir2-save-allowgroup-writeback` | R | reviewer-20（`918e0702`；首轮 `0ab26e05` request_changes） | `a46d3e3c3eb3ae805b88627bb95157ef56413e3d` | `build_save_record` 回写 live `Entity.allow_group` → `rec.data.allow_group`，使 `characters.AllowGroupReCall` 取实时值（oracle WorldServer.cs:1294），SQL 绑定保持 bug-for-bug | 删回写行 ⇒ 新单测 FAILED（left 0 / right 1）；删 `EntityDerived.allow_group` 采集 ⇒ E0063；审者自造「写反」「采集错字段」两条变异均红 |
| `openmir2-gm-residual-teeth` | L | reviewer-21（`65f66e30`） | `38439f539fd82fa0c88c62a197621960c37818b5` | GM 残余牙补强：m5 `@Binduseitem` 13 名槽逐值、N1 `flame_ring_in`/`revival` 正支、N2 差集断言不被审计删除掩蔽、F4 文档 97262 订正 | m5 3 条变异红（衣服 0→1、头盔 4→7 两条独立复验）；N1 删 `e.revival=true` ⇒ 红；N2 自造 else 支 `list.clear()` ⇒ 红（left `[5,6]` vs right `[5]`） |
| `openmir2-gm-residual-round2` | L | reviewer-20（`06a157e0`；首轮 `4900aeb2` request_changes） | `aad30901ff994557806e8203533efc5e66d1b963` | X2 R3 顺序牙（bo 先于 `SM_RECONNECT`）+ registry provenance 方案 A（指纹改被解析表 payload）+ F4 引用订正 | M-A（bo 挪到 send 后）新牙①红；M-B（bo 只进 route 失败 else）既有牙绿、新牙②红 ⇒ 独立咬合成立；表项变异 `ab565da4`→`68ba9d94`；build 两次逐字节一致 `42b1c123…` |

四张卡的收口 `status: closed` 条目依次为 `b54260c8`、`9bba0960`、`524c5d02`、`7099849b`。第二张是 R 级：按 owner 2026-10-28 规则「已合并卡一律 closed，真机项统一挂台账」，合并后即 closed；真机 DB diff 项另挂台账 `V-allowgroup-save-writeback`（条目 `ae88cf12`），后续在本夜窗口取得 PASS（见 §2）。

## §2 真机窗口结果

| 项 | 结论（run id） | 关键证据一行 |
|---|---|---|
| S8 买药背包（V-bag-layout / V-buy-stock） | REAL-MACHINE PASS（`shop-20261031-225838-246`） | 服务端 `ident=200` apply 后 `bag_total=2`，随后 81 重发回 `201 series=2`；对照满包轮 `shop-20261031-225554-575` 为 `bag_total=25` |
| V-bag-layout 收口 | RECLASSIFIED→L（条目 `7e75f5b2`） | 服务端 124B 布局=C# 契约（200/201/652 同一 `ClientItem` 结构）；冻结 oracle 栈上背包同样错位（条目 `714cf921`）；客户端侧缺陷不属本仓 |
| DEALTRY 定案（V-dealtry-e2e） | REAL-MACHINE PASS（`social-20261101-003010-861`；更早一轮 `social-20261101-001610-811` 同样两端 673） | 前置周期行 `nearest=1` 后两端 `ident=673`（解码长−12=3/8，尾名互指）；此前「零帧」复盘为发 1025 时已隔 2 格，属 oracle null-pose 原样 |
| 659 全分支（V-groupmode-659-g2） | REAL-MACHINE 全分支 PASS（`social-20261101-003739-026` + `lmz-20261101-004104-240`） | 成员/团长开组 ⇒ 659 `param=1`、建组 `ident=660`、成员关 ⇒ 659=0 且双方 666；团长 MODE=0 ⇒ 659 `param=1` + SysMsg、无 666 |
| 交易链四步（V-dealtry-e2e） | REAL-MACHINE 四步全 PASS | T2/T3 `social-20261101-003010-861`、`013200-652`；T4 `social-20261101-020223-688`（`Ident=1026 Recog=14296908`）；T5 `social-20261101-025625-807`（`Ident=1029 Recog=1`）；T6/T7 `social-20261101-021820-433`（1030→687、1028→681） |
| V-allowgroup-save-writeback | REAL-MACHINE PASS（`social-20261101-024918-770` 开 / `social-20261101-024938-486` 关） | 断线存档 queryId=20（Recog=1）⇒ `characters.AllowGroupReCall=1`；queryId=22（Recog=1）⇒ 0；`AllowGroup` 列四查均 0（与 oracle UPDATE 清单一致） |

窗口纪律：窗口期内我方 cargo 零运行、生产栈不重建；窗口结束后现场已还原（条目 `3cd25bd5`）。已登记的栈口径：S8 窗口为 master `a46d3e3c3` 重建（条目 `0c248e08`）；DEALTRY/659 轮为 master `38439f539`、exe sha256 `45f02a27…`（条目 `26e1dd41`）；allowgroup 轮为 HEAD `aad30901`、exe sha256 `593de33a…`（条目 `d9c1d09e`）；交易四步条目未登记栈 stamp（未覆盖）。

## §3 台账状态

**我方真机台账已清零**：条目 `d9c1d09e` 明写「至此我方真机台账全部清零」，条目 `caf66033` 复核「真机台账维持清零状态」。

仅余 owner 三项决定（均非新的真机失败项）：

1. **D-* GM 权限**：oracle `GetHumPermission` 唯一调用点整段注释 ⇒ 现网 GM 权限恒 0（条目 `0203f7c5`）；GM 命令族按 L 级构造证据收口（条目 `c9c8356c`）。若 owner 日后批准 D-* 偏差卡（接权限或另设 GM 认证），这些项可重开真机验证。
2. **G11 跨服**：卡 `openmir2-gm-batch-5b-cross-server` blocked-by G11（条目 `8d7abba3`）；owner 2026-10-25 裁定「先跳过」（条目 `ab2a1cf8`），该裁定只豁免条件②归类，卡本身仍 blocked（条目 `d1d117d0`）。
3. **窗口口径**：共享 RNG 跨线程交错是否非契约仍 pending（条目 `c182a7c0`）；证据候选见条目 `83cffb96`（同 seed C# 两次 Dura 3995/3998，Rust 两次 3994）。

## §4 当前状态

- **master**：`aad30901ff994557806e8203533efc5e66d1b963`。四张卡按序合并为 `b999f4139` → `a46d3e3c3` → `38439f539` → `aad30901f`（均为 master 祖先）。
- **goal_gate**：完整模式四条件 PASS（总体 PASS，RC=0；条目 `9aeda4fa`，基线 master `aad30901f`）：① 脚本派发 PASS（parse-stats 106/106 去重目标、调用 11829、未实现 0）；② R 卡证据 PASS（R 卡 291 张 @ aad30901f：已完成 280 @ aad30901f / 三者皆无 0）；③ B 类清零 PASS（tracked=0 @ aad30901f）；④ 门禁恒绿 PASS（8 项 8 绿 0 红 0 需人工，含 cargo test workspace 全量实跑）。
  - 说明：`842da407e` 之后到 `aad30901f` 的合并含一张 R 卡（`a46d3e3c3`，存档回写；真机窗口已在条目 `d9c1d09e` 取得 PASS），其余为 L 级；`aad30901f` 的完整模式复跑见条目 `9aeda4fa`。
- **tester**：GREEN，last-tested 推进到 `aad30901f`（条目 `a333d0ca`）：running 3550 / passed 3481 / ignored 69 / failed 0（143 suites）；fmt、clippy、msgcodegen 全 rc=0；卫生门 9/9 rc=0。
- **生产栈 exe**：sha256 `593de33a50f23e12176721a2f1af3ea4296e076eea1c5c95c43d7e523c37c333`（交付时实测 `mir2-rs/target/release/mir2-server.exe`，mtime 2026-11-01 02:01；台账 `d9c1d09e` 记前缀 `593de33a…` 与之一致，其 HEAD 为 `aad30901`）。
- **cargo**：空闲（交付时 tasklist 无 cargo/rustc 进程）。

## §5 已知遗留

1. **交易链 T4 的 02:56 轮 FAIL = 夹具耗尽**（条目 `caf66033` note）：上一轮交易把腰带物品转给 aaa、商店补货未及存档 ⇒ trade 轮 `ItemList=0`，非服务端问题；PASS 证据取 02:02 那轮（`social-20261101-020223-688`）。
2. **对方侧客户端工具债**：早期轮次 T4–T7 曾被对方客户端 T238/T239（点击坐标/背包格索引）阻塞（条目 `26e1dd41`）；后续交易四步已 PASS（条目 `caf66033`），但客户端工具面遗留仍归对方仓处理。
3. **窗口遗留（客户端/工具面，条目 `3cd25bd5` note）**：S9 卖出、S15 关窗拖放坐标未校准；659 双端点击吞事件。
4. **我方已登记说明（不阻塞）**：
   - F2 牙强度（review `06a157e0` Info）：重复置位变异 M-C（失败 else 一次 + send 后一次）两条用例仍绿；L 级接受，保持登记。
   - F4 registry provenance（review `65f66e30` Info）：97262B 逐字节可复现只在钉住的 `53973b7e9` 成立；HEAD 重跑 86974B，与仓库 JSON 仅差 `sources.rust_table.sha256` 一行。第二轮方案 A 已把该指纹改为被解析表 payload，加测试行不再致 HEAD 不可复现。
   - noop N1（review `5cf5a170` Minor）：5 条 `evidence.line` 漂移的根因叙述少点名 `e73ca8ab3`，交付数据正确。
   - noop N2（review `5cf5a170` Info）：未挂 `subcard`、改写入 reason，属门规则⑧（非 `needs_card` 挂子卡即判红）要求的必要偏离。

## §6 证据索引

真机台账线程 `openmir2-real-machine-verification-master`：

| 条目 oid | 内容 |
|---|---|
| `7e75f5b2094f59db82ed2958de53fd8979a61520` | V-bag-layout 收口（RECLASSIFIED→L） |
| `ae88cf1238f12cf68ec4bb01b490cef3160a09f0` | V-allowgroup-save-writeback 立项登记 |
| `0c248e084c6c9c3e1f43106b22b3237627e32b47` | S8 买药背包 REAL-MACHINE PASS |
| `3cd25bd546efa5bdca9a8c23c4dd2c8627df1d45` | 659 复验 1/3 与窗口关闭/现场还原 |
| `26e1dd412d5dfd68ba18128c56926bc6d17a0587` | DEALTRY 定案 + 659 全分支 PASS |
| `d9c1d09efe654f0c031ad6a29c7ad36b30f3d5e1` | V-allowgroup-save-writeback 1/0 PASS、台账清零 |
| `caf66033b0e7dfa9e14a376d9d6de98286128a8a` | 交易四步全 PASS、T4 夹具耗尽说明 |
| `c9c8356c96e3fe74bf5f1b8dd3025f3c99b45ade` | GM 族按 L 级构造证据收口 |
| `0203f7c5d973a12b4f6841850100e8d0c50a9088` | GM 权限 oracle 原样恒 0 |
| `c182a7c00708cef8308aa44f38eae953770012de` | 窗口口径 pending（共享 RNG） |
| `83cffb96b94d3841927a1d56698aece5ff2232ca` | 共享 RNG 结论候选与证据 |
| `714cf9219a201cdd2326078a5b7c544fb47ea43f` | V-bag-layout oracle 运行时订正 |
| `f10093cd800880772e9fb99be813e81ce9be8b61` | goal_gate 完整模式四条件 PASS |

四张卡线程（issue / review / merge_result / closed 条目）：

| 卡片 | issue | review | merge_result | closed |
|---|---|---|---|---|
| `openmir2-noop-audit-stale-row` | `4072d021aa6c7ee83657901847de82e8158376e7` | `5cf5a1705720a8b0e60fdfc45102cd28a7da84b4` | `a3975114afce6f0041e532de316125c861119e18` | `b54260c884359b4385c206f38a2dc8b6f3a7635e` |
| `openmir2-save-allowgroup-writeback` | `7dabcbeaa130f1014a8b86bc1214444d19dad021` | `0ab26e058dd6ff9c9f0c12068cfcccfb986e4c98`（首轮）/ `918e0702e3145c9b2baa2bdff41840c0c5323eba`（复审） | `514d42171be3b271152b388244352958d7f9ad14` | `9bba0960147c1e4685af98eea033617e4b67b71d` |
| `openmir2-gm-residual-teeth` | `4ad858480f722664e0c2d36b6b97c9a01558f4bf` | `65f66e3011bc21476ca7239273ec58490e5f40a6` | `5729914fdb6ccc142957597c5698606859cecdb1` | `524c5d02370eb2f3973916dc17c75d9781a311e3` |
| `openmir2-gm-residual-round2` | `931a74f2ffda8c1bd8b585154f6d1e91587bd79d` | `4900aeb298dc0a7b7daecf5f60e41f38cd498bae`（首轮）/ `06a157e0c2f1f761bc8c948bfcaca371d13894e2`（复审） | `2fe8ac2b8bd77ac2505f2fbb6807802d7332544e` | `7099849b60c3280f7c05eb04450c46747bc81af5` |

其他：

| 来源 | 条目 oid |
|---|---|
| tester GREEN / last-tested `aad30901f` | `a333d0ca7ebf925f960eb8cd8b2e107496c10c15` |
| G11 卡 blocked 状态 | `8d7abba32e27cd12528195bba50be74ffd85dd21` |
| G11 owner 裁定「先跳过」 | `ab2a1cf8d37cc3ae02dc1297d4520193fbcaffe8` |
| G11 豁免落地说明 | `d1d117d067d3593c947d8e71895434d6943c3c6f` |

run id 清单（逐字取自台账条目；`013200-652` 为台账原文缩写）：`shop-20261031-225838-246`、`shop-20261031-225554-575`、`social-20261101-003010-861`、`social-20261101-001610-811`、`social-20261101-003739-026`、`lmz-20261101-004104-240`、`013200-652`、`social-20261101-020223-688`、`social-20261101-025625-807`、`social-20261101-021820-433`、`social-20261101-024918-770`、`social-20261101-024938-486`。

## §7 11-01 上午追加（whitelist 复核与工具面）

- 覆盖区间：昨夜报告交付点 master `aad30901f`（§4）→ 本节收口的 master `3994ce468`。本节只补记上午工作，**不改 §1–§6 的任何结论**。
- 口径：oid / run id / 数字均从对应线程与台账实读；未取证处显式标注。本节涉及的卡片全部 tier=L，无真机窗口项。

### 7.1 ① whitelist 独立复核 10 批（`openmir2-whitelist-independent-review-1..10`）

收口 status `a9ce30ca`（review-10）：**10 批共核 176 条**（D/B 7 + S4B/M1-EXIT 12 + T 157）；**T-* 非 legacy 待复核清零**；`legacy` 82 条按 owner 裁定保留历史留痕、不做独立复核。

| 批 | worker | 范围 | 结论 | review（reviewer） | merge oid |
|---|---|---|---|---|---|
| 1 | dev-3 | D-9/D-10/D-13/D-14/B-116/B-117/B-16（7 条） | 6 CONFIRMED + B-16 STALE-ANCHOR（正文锚订正 1263-1266 / 1398-1401 / 1650-1653） | `a5283ecf`（reviewer-21） | `729f1e385` |
| 2 | dev-1 | S4B-1..10 + M1-EXIT-1/2（12 条） | 12/12 CONFIRMED（S4B-1 范围缩窄、S4B-4 引用订正后收口，DISCREPANCY 清零） | `62a675cf`（reviewer-21） | `4d57f4f62` |
| 3 | dev-2 | T-* 非 legacy 前 20 条 | 18 CONFIRMED + T-65/T-47 STALE-ANCHOR（10 处锚订正、T-31 残余句订正） | `3f37a4f0`（reviewer-20） | `ad6872a0d` |
| 4 | dev-1 | T-* 第 21–40 条 | 19 CONFIRMED + T-72 STALE-ANCHOR（T-79/T-120 订正） | `f057d433`（reviewer-21） | `4c78241ef` |
| 5 | dev-2 | T-* 第 41–60 条 | 17 CONFIRMED + 2 STALE-ANCHOR（T-137/T-140/T-162 三处订正） | `ad3dec62`（reviewer-21） | `c743e34ce` |
| 6 | dev-1 | T-* 第 61–80 条 | 11 CONFIRMED + 9 处接线/引用订正（T-174/180→T-199、T-178→T-205、T-182→T-202、T-183→T-206 等） | `d7c20788`（reviewer-20） | `40b456042` |
| 7 | dev-2 | T-* 第 81–100 条 | 20/20 CONFIRMED（T-223 引用、T-210 接线状态、T-206 哈希快照三处订正） | `e9286f7d`（reviewer-21） | `83d6c53b8` |
| 8 | dev-1 | T-* 第 101–120 条 | 20/20 CONFIRMED（T-214 oracle 顺序、章节引用 §1→§3、承接卡状态订正） | `65a71e72`（reviewer-20） | `fcd8db521` |
| 9 | dev-2 | T-* 第 121–140 条 | 20/20 CONFIRMED（含 13 处 §1→§3、T-117/T-208/T-242 订正）+ T-212 锚订正 | `b9f8724d`（reviewer-21） | `09b2c2328` |
| 10 | dev-2 | T-* 非 legacy 剩余 17 条 | 17/17 CONFIRMED（§1→§3、T-262 §4 节锚、T-266 bag.rs:667-668、T-270 形状 5→7）；F7/F8 minor 收口后复审 | `1651dd9a` 首轮 / `8cb60ffe` 复审（reviewer-21） | `3994ce468` |

代表性发现（逐条可在对应 review 内定位）：

- **「登记落后于代码」批量订正**：第 4 批 T-79 的 ①武器升级 / ④LAMPCHANGEDURA 已由 T-199（`d99d4badc`）接线、⑤SM_PLAYDICE 由 T-142（`4dce0717e`）接线；第 6 批 T-174/T-180→T-199、T-178→T-205（`aoi::search_view_range_events`）、T-182→T-202、T-183→T-206 的「登记不接线 / 生产点零命中」全部改记为已接线；第 7 批 T-210 接线状态按 T-211 五路入口订正。
- **T-268/T-269 自审标记改双 principal**：原「已复核」的作者与验证同为 `openmir2-dev-2`；第 10 批复审 F7 指出后改为「openmir2-dev-2 验证 + openmir2-reviewer-21 独立复核」（status `1be40b43`，复审 `8cb60ffe` G2 核实）。
- **T-270 枚举补全**：`oracle_extra_shapes` 实际 7 形状，逐形状 decision 细目补 `327,327,3→449cd0c9…`、`327,327,4→811aff9f…`（F8）。
- **T-212 锚订正**：`CommandAttribute.cs:56-58` 订正为 `:53`（:56-58 实为下一类的 XML 注释，oracle :53 = CommandHelp 属性）；第 8 批 review 记 minor，第 9 批落地，anchor gate 仍 611（`b9f8724d`）。

### 7.2 ② 工具面：`mir2_probe._check_blocked` 接受结构化 `blocked.not_in_run3_reason`（`openmir2-probe-blocked-schema-align`）

- 来源：whitelist 第 9 批 T-242 记录的工具 schema 漂移 —— `scenario_manifest_gate` 接受该结构化键，而 `mir2_probe._check_blocked` 只接受 `reason` / `missing_fixture`，同一场景一个门判合法、另一个判死。
- 症状（master 基线）：`clearmapmon_t118` / `combat_hit` / `combat_strike` / `p21_unequip` / `pickup_item` 五场景 `--check-scenario` **rc=2**，错误 `scenario.blocked has unknown key(s): ['not_in_run3_reason'] (fail-closed)`（patch `8423ecc3` 的 `repro_before`）。
- 修法：`_check_blocked` 的 optional 键集合加入 `not_in_run3_reason`，要求**非空**（非空字符串或非空对象）（`tools/probe/mir2_probe.py:945`、`:958-963`）。
- **未放宽**：`reason` 仍必填、未知键仍拒、`not_in_run3_reason` 空串 / 空对象 / `null` / 其它类型仍点名本键 rc=2；负例 10 坏例 + 3 正例；5 支变异（删 optional 键 / 删非空校验 / 仅放开空对象臂 / 仅放开类型臂 / 去 `reason` 必填）均使 selftest rc=2，恢复后 blob `2c3fc68e…`；selftest 29 checks OK（status `568cc8e3`）。
- 参照方订正：卡面原写 `scenario_manifest_gate`，实际读场景 `blocked.not_in_run3_reason` 的是 **`tools/window/blocked_sources_check.py:591`**（`tools/hygiene/scenario_manifest_gate.py:159` 读的是豁免 TOML，不解析场景 blocked）——审者 `4b5cf9ed` 复核确认。
- 复跑：5 个场景 + `login_only` / `softclose_relogin` 全 rc=0（`568cc8e3`）。
- 收口：review `4b5cf9ed`（reviewer-20，approve，tier=L；2 条 nit 非阻塞：docstring 措辞比参照方宽、显式 `null` 被当缺省放过，深判仍在 `blocked_sources_check`）→ merge `1f23c0c9e` → closed `44246b49`。

### 7.3 ③ 注释卡两张（whitelist 复核中的非阻塞观察开卡，均 tier=L、零代码行）

- **`openmir2-castle-tax-anchor-sync`**：第 1 批复审发现 `mir2-rs/crates/world/src/castle_tax.rs:12` 模块文档仍引旧行号（anchor gate 不扫 Rust 注释，故未红）。三处 `Merchant.cs` 锚由 1250-1259 / 1385-1393 / 1637-1645 同步为 **1263-1266 / 1398-1401 / 1650-1653**（544-553 不变），并注明「锚已随 B-16 复核订正 2026-11-01」。审者逐行过滤 12 条 +/- 行全为 `//!` / `///`（`08388344` F1）；`cargo test -p mir2-world --lib castle` 105 passed。patch `bd21310e`（head `2cbde5b9`）→ review `08388344`（reviewer-20）→ merge `fceaa6ba7` → closed `448ee0ea`。
- **`openmir2-entity-safezone-comment`**：第 4 批复审观察 `mir2-rs/crates/world/src/entity.rs:625-630` 原注释称「接线层会写 `in_safe_zone`」，而生产代码无任何写入点。注释改为「生产代码当前无写入点（该字段恒 false），仅测试夹具赋值；若将来接线写入，须同步 whitelist T-75」。审者核全部赋值点位于 `#[cfg(test)] mod tests` / 集成测试（`fac17fb9`），diff 2+/2- 全为 doc 注释。patch `42f53074`（head `f834fdcf`）→ review `fac17fb9`（reviewer-20）→ merge `6b66e17f2` → closed `eecb75a0`；`cargo test -p mir2-world --lib safe_zone` 1 passed。

### 7.4 ④ 窗口证据回登 8 卡（`openmir2-window-evidence-backfill`，issue `c20127aa`）

边界：只对 8 张 `window_required` R 卡加**签名 comment** 引用既有真机证据，不改卡片 status、不改代码/测试/台账。

8/8 covered（汇总 comment `39d282a3`）：

| # | 卡 | 证据引用 |
|---|---|---|
| 1 | `openmir2-groupmode-659-always-reply` | 台账 `26e1dd41` + run `social-20261101-003739-026` / `lmz-20261101-004104-240` |
| 2 | `openmir2-login-srv-bind-addr` | 台账 `00ccdfa9` + `2e439135` / smoke-recut6-login；`docs/cutover-report-20261030.md:38,107-108` |
| 3 | `openmir2-login-srv-dsn-source` | `ef440c86` / `59e15ac6` + `2e439135` |
| 4 | `openmir2-login-srv-illegal-addr-filter` | `9807cc4e` + `ef440c86` 529 + `2e439135` |
| 5 | `openmir2-login-srv-reply-chain` | `668050b6` + `ef440c86` 529 + `2e439135` |
| 6 | `openmir2-login-srv-select-route-match` | `69e3014a` + smoke-final-login_only `frames.ndjson:2` + `2e439135` |
| 7 | `openmir2-selgate-dbsrv-handshake` | 生产栈 `d3a22e63` 的 5100 面长连（`db-srv.out.log:13-14/35/49` 03:49→08:55、`sel-gate.out.log:3`；C# 基线 review `cc715978`） |
| 8 | `openmir2-selgate-dbsrv-query-chain` | `21294f00` + smoke-recut6-login `frames.ndjson:3` + recut8 |

两条披露（台账 `3ad8654e`）：

1. **handshake 卡面 A3 端口不符**：卡面写 7100↔6000，部署为 `ServerPort0=5100` ⇒ 字面 6000 面的长观测未单列；是否修改卡面措辞属 owner 裁决。
2. **`sel-gate.log` 4 条断开是探针会话**：属探针客户端会话断开，不是上游链断开。

机制说明（台账 `3ad8654e`）：`goal_gate` 的 `r_category` 把 `window_required` 判在 `evidence_entries` 之前 ⇒ 回登 comment **不会改变分桶**（设计如此）；不为改桶而把 `window_required` 裁定为 false（原审查设该标记时所要求的窗口面与现有证据并非逐项一一对应，强行裁定会弱化标记原意）。

收口：dev-1 汇总 comment `39d282a3` → review `bd99621d`（reviewer-21，approve，抽核 3 条引用可定位、确认 8 卡 status 未改）→ 卡 `status: closed` `cee51dcf`（本卡无代码合并）。

### 7.5 ⑤ 最终状态（master `3994ce468`）

- **master**：`3994ce468`（review-10 的合并节点）；自 `aad30901f` 起共 13 次合并（whitelist / 工具 / 注释面），台账 `c3f98adc`。
- **goal_gate --full 四条件 PASS**（台账 `c3f98adc`，基线 master `3994ce468`）：①脚本派发 106/106（调用 11829，未实现 0）；②R 卡 291 张 @ 3994ce468（三者皆无 0）；③B 类 tracked=0 @ 3994ce468；④门禁 8 项 8 绿。
- **cargo test**：`3994ce468` 上 goal_gate 完整模式实跑（台账 `c3f98adc`）与合并树门禁各一次（merge_result `d8b1dd22`），running 3550 / passed 3481 / ignored 69 / failed 0（143 suites，rc=0）；tester 独立复核的三元组实测点为 `aad30901f`（条目 `a333d0ca`，verdict GREEN）。
