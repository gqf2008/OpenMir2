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
- **goal_gate**：完整模式四条件 PASS（总体 PASS，RC=0；条目 `f10093cd`，基线 master `842da407e`）：① 脚本派发 PASS（parse-stats 106/106 去重目标、调用 11829、未实现 0）；② R 卡证据 PASS（290 张：已完成 279 / window_required 8 / 三者皆无 0 / merged 未 closed 无登记 0）；③ B 类清零 PASS（tracked=0）；④ 门禁恒绿 PASS（8 项 8 绿 0 红 0 需人工，含 cargo test workspace 全量实跑）。
  - 说明：`842da407e` 之后到 `aad30901f` 的合并含一张 R 卡（`a46d3e3c3`，存档回写；真机窗口已在条目 `d9c1d09e` 取得 PASS），其余为 L 级；`aad30901f` 上未记录新的 goal_gate 完整模式复跑（未取证）。
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
