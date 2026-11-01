# 夜间交付报告：2026-11-01 傍晚 → 2026-11-02 凌晨

- 卡：`openmir2-night-report-20261102`（tier=L，纯文档汇总）
- 范围：`walgit/master` **`13112da07de8b358f4acf876f5cf720c41f1de76`**（2026-11-01 15:37:11 +0800）
  → **`1634fff4bd9f237e2197a311d8400e2ab4c12cea`**（2026-11-02 00:41:47 +0800）
- 规模：区间内 **17 次 first-parent 合并**、**40 个提交**（17 个 merge + 23 个被并提交）。
  其中 **tier=R 1 次**（`aa2749620`），tier=L 16 次。
- 复算：

  ```sh
  git merge-base --is-ancestor 13112da07 master && echo ancestor-ok
  git rev-list --count --merges 13112da07..master              # 17
  git rev-list --count 13112da07..master                       # 40
  git log --first-parent --merges --format='%H|%ad|%s' --date=iso 13112da07..1634fff4b
  ```

> **时点约定**：本报告里的门禁计数、工具条数、锚数都是 **`1634fff4b` 树上的实测值**，
> 并注明实测命令；引用其他卡的合并记录时写清是「当时值」。历史数字只作追溯，不作现况。
> 所有「真机项」一律标 **NOT_RUN**——**未跑 ≠ 通过**。

---

## 0. 一页摘要

这一轮做了四类事：

1. **MCP 功能落地**：新增 `mir2-mcp` 本地 stdio MCP 服务（slice-1：185 条 GM 目录 + 开区配置只读/白名单写），
   再接 game-svr 的本机回环 admin 通道做 `gm_execute` 真执行（slice-2，**tier=R**，真机项未跑），
   最后收口 admin 通道的 Minor、生产准入接缝的编译期牙，以及三条真机项的执行单。
2. **签名与门禁链路加固**：把当天真实发生的两类写侧误签（空 parent、陈旧 body）变成 `sign_entry` 的
   fail-closed 行为；让裸 `oid_gate check` 默认加载仓内 baseline 并 fail-closed；登记并复核历史坏 parent 豁免；
   修掉自测临时目录泄漏。
3. **登记表普查**：全表核 `whitelist.md` 里「待独立 principal 复核」的 85 行；普查「已合并但无独立审查」的卡；
   按 owner 裁决把 approve 口径（`decision`/`verdict`/legacy `body.review`）与作者口径（patch ∪ issue）定死。
4. **窗口工具修复**：`gm_gate_check.py` 的红字 Param 常量由反序 `65336` 订正为 `0x38FF(14591)`，
   G1 按 param 过滤、G3 按冻结 schema join；补字面量绝对锚；probe 场景散文同步订正。

**最重要的边界**：MCP admin 的三条**真机**判据一条都没跑（等独占窗口）；`T-273` 的 R 卡窗口项保持待办。
本轮所有可合并的 `tier=R` 卡都只完成了**构造级/单元级**证据，不能当成真机等价已验收。

---

## 1. 17 次合并逐条

取法（**不要抄旧列表**，直接跑）：

```sh
git log --first-parent --merges \
  --format='%H|%h|%ad|%s' --date=iso 13112da07..1634fff4b
```

合计 17 条，按时间升序如下。表内 approve 的 oid 是 `walgit` 协作**条目 oid（blob）**，
不是 commit；需要核 commit 时用 patch 条目的 `refs.head` 或 merge 提交本身。

| # | merge 提交 | 时间（+0800） | 卡 | approve（条目） | tier |
|---:|---|---|---|---|---|
| 1 | `dcea680f9` | 2026-11-01 18:01:23 | `openmir2-mcp-gm-conf` | reviewer-20 `fb212299` | L |
| 2 | `5f14fca15` | 2026-11-01 18:50:46 | `openmir2-oid-baseline-night-report-parent` | reviewer-21 `217a4f27` | L |
| 3 | `aa2749620` | 2026-11-01 18:57:45 | `openmir2-mcp-gm-execute` | reviewer-20 `434d5e16` | **R** |
| 4 | `151b5ab17` | 2026-11-01 19:19:08 | `openmir2-collab-sign-entry-hardening` | reviewer-21 `6a3e1c3f` | L |
| 5 | `81c8fd6a2` | 2026-11-01 19:24:42 | `openmir2-oid-gate-default-baseline` | reviewer-20 `0b6d1608` | L |
| 6 | `ceacecb3c` | 2026-11-01 19:58:24 | `openmir2-collab-tool-tails` | reviewer-20 `6a035cf8` | L |
| 7 | `cf604f260` | 2026-11-01 20:03:56 | `openmir2-mcp-admin-review-minors` | reviewer-21 `896a783a` | L |
| 8 | `8256e16d0` | 2026-11-01 20:11:29 | `openmir2-whitelist-merged-row-backfill` | reviewer-20 `9335d731` | L |
| 9 | `54ed3795b` | 2026-11-01 20:37:33 | `openmir2-sign-entry-selftest-tmp-leak` | reviewer-21 `a29f342b` | L |
| 10 | `5f5ec4243` | 2026-11-01 20:45:05 | `openmir2-admin-loopback-wiring-tooth` | reviewer-20 `3d5eaf7e` | L |
| 11 | `945df8dd7` | 2026-11-01 22:49:56 | `openmir2-merge-without-review-audit` | reviewer-20 `3dbcdd9d` | L |
| 12 | `f40235b1b` | 2026-11-01 22:58:16 | `openmir2-whitelist-status-audit` | reviewer-21 `86ef3156` | L |
| 13 | `a1a2ebf46` | 2026-11-01 23:32:21 | `openmir2-census-caliber-owner-decision` | reviewer-21 `071d33b1` | L |
| 14 | `4d0e1c875` | 2026-11-02 00:05:12 | `openmir2-mcp-admin-window-sheet` | reviewer-20 `8f56c48c` | L |
| 15 | `c8629cb0f` | 2026-11-02 00:23:32 | `openmir2-gm-gate-red-param-fix` | reviewer-21 `b1f51834` | L |
| 16 | `61efafed1` | 2026-11-02 00:35:54 | `openmir2-probe-scenario-redparam-prose` | reviewer-20 `f90c18e8` | L |
| 17 | `1634fff4b` | 2026-11-02 00:41:47 | `openmir2-gm-gate-red-param-abs-anchor` | reviewer-21 `46268224` | L |

计数核对：17 条 = **1 R + 16 L**；按主题分组为 **5 + 5 + 4 + 3**（下四节）。

### 1.1 MCP 功能（5 次合并）

| merge | 做了什么 | 关键结论（来自该卡 merge_result） |
|---|---|---|
| `dcea680f9` | slice-1：新 crate `mir2-mcp`（`rmcp 3.8.0`，stdio，**不监听端口**），6 个工具：`gm_list` / `gm_help` / `conf_get` / `conf_keys` / `conf_set` / `gm_execute`（此片为结构化 `not_wired`） | 185 条 GM 目录（176 implemented / 9 unimplemented / perm0=5），条数断言 `catalog_total == summary.table_total`；`conf_set` 白名单只有 `[Setup] GenMonRate` / `MonsterPowerRate`（1..=1000），**`ExpRate` 是 oracle 不存在的键**，工具列出但拒绝写入；写路径「备份 → 临时文件+rename → 回读校验 → 不符回滚」。 |
| `aa2749620` | slice-2（**tier=R**）：game-svr 新增 `[Admin]` **本机回环**通道（`AdminPort` 缺省 0=不监听；`AdminBind` 非回环拒绝启动），`GateEvent::AdminGm` 投 sim 线程，`execute_admin_gm_command` **复用 `gmcmd::execute` + `command_event_tail_gm_drain`**，抬权 10 只覆盖单次调用并立即还原；MCP `gm_execute` 投递与结构化透传 | 与客户端 `CM_SAY` / 脚本 `GMEXECUTE` **同一执行收口**，无第二套解析；T-273 登记；**真机项未跑**（见 §4）。 |
| `cf604f260` | admin 通道三处 Minor：`frames` 措辞统一为「本次调用的直接应答帧数」、accept 层回环守卫补牙、MCP 客户端 `admin_addr_not_loopback`（非回环**连接前**拒绝） | 未改 `frames` 计算；reviewer 用 0.0.0.0 监听对照证明测试组 accept +0；三条非阻塞尾项另开卡。 |
| `5f5ec4243` | admin 生产准入接缝**编译期牙**：裸 `fn(SocketAddr)->bool` 收紧为 `enum AdminPeerPolicy`，生产构建只有 `LoopbackOnly` 一个臂（`DenyAll` 仅 `#[cfg(test)]`）；`[::ffff:127.0.0.1]` 用例；IPv6 无括号写法文档；T-273 条数 8→9 | 把生产实参换恒真闭包 ⇒ `cargo check` rc=101 `E0308`；生产引用 `DenyAll` ⇒ `E0599`；`unsafe transmute` ⇒ `unsafe_code=forbid` + `E0512`。运行期语义不变（两侧都是 `ip.is_loopback()`）。 |
| `4d0e1c875` | `docs/mcp-admin-window-sheet-20261101.md`（847 行）：MCP admin 三条真机项的可照做**执行单**（启用/还原、命令、证据四件套、前置纪律），全部 `NOT_RUN` | 事实订正：`@怪` 的参数与权限、`@清除怪物` 恒回红且不抽数、`@LoadAdmin` **只回绿文本不改权限**、红字 Param=`0x38FF(14591)`；reviewer 实网 525 帧复核 14591×49 / 65336×0 / 65499×2058。 |

### 1.2 签名与门禁链路加固（5 次合并）

| merge | 做了什么 | 关键结论 |
|---|---|---|
| `5f14fca15` | `oid_gate_baseline.json` 逐条新增 1 条历史坏 parent 豁免：`principal=openmir2-orchestrator` / `thread=openmir2-night-report-morning-append` / `entry_oid=d1e7214d…` / `field=parent` / `value=2113986cd0e8…` | `oid_gate.py` 零改动；既有 9 条零改动；豁免键**含 entry_oid**，同 principal/thread/value 换一条新条目不会被旧豁免命中。 |
| `151b5ab17` | `sign_entry` 写侧 fail-closed：`--parent` 缺席/空 ⇒ 拒签；`--root` 只允许线程首条目；`--body-file` mtime > 1800s ⇒ 拒签并打印 `body_sha256`+mtime；签后打印 `body_sha256` | 自测 5→13 例；作者 7 项变异 + reviewer 复放 3 项 + 自造 1 项全部变红；reviewer 用 shim 证明事故①三种形态下 `walgit` **调用 0 次**。尾项 F1（`--kind issue` 早退）留下。 |
| `81c8fd6a2` | `oid_gate check/gate` 不传 `--baseline` 时缺省加载仓内 `tools/collab/oid_gate_baseline.json`（与 `--audit-baseline` 口径合一）；新增 `--no-baseline`；缺失/空/坏 JSON ⇒ **rc=2 fail-closed** | 自测 52→56；reviewer 自造 5 态全部 rc=2；主线验证「新坏 oid 仍判红」：换 `entry_oid`/换 `thread` 均 rc=1，只有键完全匹配才豁免。 |
| `ceacecb3c` | `sign_entry --kind issue` 不再早退，任何 kind 都探测线程历史（仅空线程允许 `--root`）；`oid_gate` selftest 临时目录清理；baseline 文档加 as-of 注 | 13→16 例；真实 4 条线程上 `--kind issue --root` rc=1 且 refs 13790→13790 未落条目；`oid-gate-*` 1065→1065、`oid-audit-*` 505→505（连跑两次不增）。 |
| `54ed3795b` | `sign_entry.py` 自测夹具清理改为 `rmtree(tmp, onerror=_rmtree_force)`（与 `oid_gate._rmtree_force` 同形）；文档/`.sh` 头注释同步现行语义 | reviewer 用集合差为权威口径：受控基线 49 → 跑 1 次 49 → 跑 3 次 49 → 全量门禁后 49；变异换 `pass` ⇒ 49→50 复现泄漏。尾项 F3（`onerror=` 在新 Python 上 deprecated）。 |

### 1.3 登记表普查（4 次合并）

| merge | 做了什么 | 关键结论 |
|---|---|---|
| `8256e16d0` | `whitelist.md` 已合并却仍写「待审」的行回填；T-273 两条不存在的 GM 命令名订正 | T-273 末列改为「已审已合：approve `434d5e16` + merge `aa2749620`」且保留「真机项未跑」；`@刷怪`/`@召唤` 不在 185 条表（grep 0 命中），改为 `@怪`（Imp::MobSpawn）/`@清除怪物`（Imp::ClearMapMonster）。卡面 W3 前提被作者纠正、reviewer 裁决「卡面错、作者正确」。 |
| `945df8dd7` | 只读普查工具 `tools/collab/merge_without_review_audit.py`（selftest 38/38，git 子命令白名单 fail-closed，写子命令抛 `ReadOnlyViolation`）+ 报告 | 快照中线程 986、已合并 870；有非作者 approve 845；零 review 13；仅作者自审 5；有非作者 review 但无 approve 7（9 条 review，8 条结论写在 `body.review`、1 条 request_changes）；账本 `merge_result.oid ≠ 真实合并提交` 7 张。**两种口径留给 owner 定**。 |
| `f40235b1b` | `whitelist.md` 的「待独立 principal 复核」85 行普查落定；只读工具 `tools/whitelist/wl_status_audit.py`（selftest 24/24）入工具门 | 最终 **85 = 81 已审已合 + 2 已合未审（T-2/T-3）+ 2 行引用块散文 `PROSE_UNCLOSED`**；`TRACE_UNCLOSED=0`。返工把首版的 61「证据不足」桶中 **57 个 T 行闭合为已审已合并改写**（见 §2 P6）。 |
| `a1a2ebf46` | 按 owner 2026-11-01 裁决把普查口径定死并重跑 | **线程 987 / 已合并 872 / ① 非作者 approve 854 / ② 零 review 13 / ③ 仅自审 5 / ④ 有 review 无 approve 0**。legacy `body.review` 救回 7 卡 / 8 条（`gm-residual-round2` 两条），另 1 条 legacy `request_changes` 不计；作者口径 = patch ∪ issue。 |

### 1.4 窗口工具修复（3 次合并）

| merge | 做了什么 | 关键结论 |
|---|---|---|
| `c8629cb0f` | `gm_gate_check.py`：`RED_PARAM` 由反序 `(255<<8)|56 = 65336` 改为 **`0x38FF = 14591`**；G1 先按 `param==14591` 过滤再要求恰 1 条；G3 删 `characters_ablity.ChrName`（冻结 schema 无此列），改 `characters.Id = characters_ablity.PlayerId` join，缺行/未知角色 fail-closed 为 UNDECIDABLE | reviewer 独立扫 530 个真帧：`ident==100` 中 14591×49、65336×0、65499×2066；夹具可持续性已核（外部路径只出现在注释，selftest 不读文件）。 |
| `61efafed1` | probe 场景 `gm_level0_gate.json` 两处散文把红字 Param 由 `65336` 改为 `0x38FF(14591)`，补 MakeWord 位序说明（低位=前景色、高位=背景色） | reviewer 深度递归比较 base/head：`total_diffs=2`，仅 `description` 与 `steps[1].note`；`steps`/`params` 全同（机器字段零改动）。 |
| `1634fff4b` | `gm_gate_check` selftest 新增**字面量绝对锚**（`const_val != 0x38FF`，字面量写在测试内、不派生自常量），对 `RED_PARAM` 与 `REAL_CAPTURE_RED_PARAM` 两个常量都断言 | 复放上轮绿掉的 M5（两常量同改 9999）⇒ 现在 rc=1；「只改一个常量」也红；既有断言行 21→23，无删除。 |

---

## 2. 发现的问题

每条给出 **症状 / 根因 / 当场证据（命令 + rc）/ 修复卡**。证据命令都在本 worktree
`E:/Users/gxh/Documents/GitHub/OpenMir2/.worktrees/dev-1` 的 `1634fff4b` 树上重跑过。

### P1 空 parent 误签（事故①）

- **症状**：`openmir2-mcp-gm-conf` 线程上出现一条 `merge_result` 条目 `ce176bce`，`parent` 为**空串**，
  落成「根条目」；所有三视图都不报错，门禁也没拦。
- **根因**：`walgit collab thread <id> --format json` 不支持该 flag，管道取到空串；
  `sign_entry.sh` 把空 parent 打印成 `parent=<root>` 后照签；`oid_gate` 对空 parent 是**跳过**而非判红。
- **当场证据**（修复后行为，2026-11-02 实测）：

  ```sh
  python -I -B tools/collab/sign_entry.py --precheck --repo . --kind status \
    --id openmir2-night-report-20261102 --body-file /tmp/.../fresh.json
  # rc=1  sign_entry: REFUSED: --parent 缺席或为空：拒绝静默签「根条目」

  python -I -B tools/collab/oid_gate.py verify-entry 2113986cd0e8e1b9c1e3a8ac7a3f0d8b3e6c8c7a
  # rc=1  FAIL 拒绝解析不到的 parent（不是存在的对象）
  python -I -B tools/collab/oid_gate.py verify-entry 2113986c0a8962d1e3ba9f06338c7e159c931ed9
  # rc=0  PASS parent 全部可解析（1 个）
  ```

- **修复卡**：`openmir2-collab-sign-entry-hardening` → merge `151b5ab17`；
  尾项 F1（`--kind issue` 早期早退绕过历史探测）由 `openmir2-collab-tool-tails` → `ceacecb3c` 收口。

### P2 陈旧 body 误签（事故②）

- **症状**：dev-1 给 `openmir2-mcp-gm-execute` 签出的 patch `f0bf7f2e3d` / status `826dc73574`，
  内容与 head（`b1163a0d`）指向**已合并的旧分支** `card/mcp-gm-conf-dev1`；靠人工发现 + comment 声明取代。
- **根因**：`--body-file` 指到 `/tmp` 下 slice-1 的残留文件；`sign_entry` 当时没有 body 新鲜度校验，
  也没有签后可回读的 `body_sha256`。
- **当场证据**（2026-11-02 实测）：

  ```sh
  # 有效 parent + mtime=2020-01-01 的 body
  python -I -B tools/collab/sign_entry.py --precheck --repo . --kind status \
    --id openmir2-night-report-20261102 --parent <40-hex> --body-file /tmp/.../stale.json
  # rc=1  REFUSED: body 文件陈旧：… mtime=2020-01-01T00:00:00+08:00（已 215744861s > 阈值 1800s）
  #        body_sha256=98ff49fc…

  # 同一路径、新鲜 body
  # rc=0  打印 body_sha256=98ff49fc…
  ```

- **修复卡**：`openmir2-collab-sign-entry-hardening` → `151b5ab17`。
  遗留 F2（非阻塞）：30 分钟内的残留 body 不会被拦；`--allow-stale-body` 放行后条目本身不留痕迹。

### P3 `oid_gate` 裸跑不加载 baseline

- **症状**：`oid_gate.py check` 不传 `--baseline` 时在真仓库上永远 rc=1（历史坏 parent），
  看起来像门坏了；而 `--audit-baseline` 缺省却会取仓内 baseline，两条路径口径不一致。
- **根因**：`real_check` 里 `if baseline_path else []` —— 没给路径就当空清单；调用方必须记得手写路径。
- **当场证据**（2026-11-02 实测，**注意窗口效应**）：

  ```sh
  python -I -B tools/collab/oid_gate.py check              # rc=0
  # PASS … 扫描最近 200 条 …；baseline 共 10 条（命中 0，未命中 10）
  python -I -B tools/collab/oid_gate.py check --no-baseline # rc=0（坏条目已滑出 200 条窗口）
  ```

  `--no-baseline` 现在 rc=0 **不代表豁免可用**：默认只扫最近 200 条，登记的那条历史坏 parent
  在当前库规模下不在窗口内（见 §4 观察 O1）。绕过窗口直接核 oid 仍是坏的：
  `verify-entry 2113986cd0e8…` rc=1（见 P1）。
- **修复卡**：`openmir2-oid-gate-default-baseline` → `81c8fd6a2`；baseline 缺失/空/坏 JSON 一律 rc=2 fail-closed。

### P4 admin 生产实参无牙

- **症状**：把 `run_admin_listener` 里的 `admin_peer_is_allowed` 换成 `|_peer| true`，
  `admin_channel_tests` 仍 **8/8 全绿**——回环过滤在生产路径上只靠人工审计。
- **根因**：接线牙只测「判据函数」本身，不观测生产调用点传入的实参；裸 `fn(SocketAddr)->bool`
  可以被任意闭包替换而类型系统不拦。
- **当场证据**：tester-1 独立复核 `b1242001`（tree `5f5ec4243`）实测把生产实参改为闭包
  `|_peer: SocketAddr| true`，`cargo check -p game-svr --lib` **rc=101**，
  `error[E0308]: mismatched types — expected AdminPeerPolicy, found closure`；还原后源文件 sha256
  前后相等 `b2079661…`、`git status` 空。reviewer 另加两道绕行：生产引用 `DenyAll` ⇒ `E0599`；
  `unsafe transmute` ⇒ `unsafe_code=forbid` + `E0512`；判据体改恒真 ⇒ 7 passed / 2 failed。
- **修复卡**：`openmir2-admin-loopback-wiring-tooth` → `5f5ec4243`（`enum AdminPeerPolicy`，生产唯一 `LoopbackOnly`）。

### P5 `gm_gate_check` 红字常量反序（窗口门会永远判红）

- **症状**：G1 要求红字 `SM_SYSMESSAGE(100)` 的 `Param = MakeWord(255,56)`，工具写成 `65336`；
  现网真实帧是 `0x38FF = 14591` ⇒ **该门对真实帧永远不可能 PASS**。工具自己的 selftest
  用同一个错常量造夹具 ⇒ 自洽却与现实相反。
- **根因**：`MakeWord` 位序反了——工具用 `(f<<8)|b`，实现/oracle 是 `f | (b<<8)`
  （低位=前景色、高位=背景色）；oracle 锚：`src/OpenMir2/HUtil32.cs:66-69`、
  `mir2-rs/crates/protocol/src/smframe.rs:39-51`、`bins/game-svr/src/lib.rs:46536` 的单测。
  同类问题还有 G1 把登录/公告的 `65499` 帧也计入、G3 用冻结 schema 不存在的 `characters_ablity.ChrName`。
- **当场证据**：

  ```sh
  python -I -B tools/window/gm_gate_check.py selftest   # rc=0  SELFTEST PASS
  ```

  reviewer 独立普查 525/530 个真 `frames.ndjson`：`ident==100` 中 `14591 × 49`、`65336 × 0`、
  `65499 × 2058/2066`（两次统计的扫描集不同，均记录在 merge_result）。
- **修复卡**：`openmir2-gm-gate-red-param-fix` → `c8629cb0f`；
  绝对锚尾项 `openmir2-gm-gate-red-param-abs-anchor` → `1634fff4b`（两常量同改 9999 现在 rc=1）。

### P6 普查工具漏判 61 行中的 57 行

- **症状**：`wl_status_audit.py` 首版把 61 行归入「证据不足（保持原样）」，
  但其中大量行其实能找到交付卡、非作者 approve 和合并记录，按卡自己的判据应改写为「已审已合」。
- **根因**：`resolve_cards()` 只认行内 canonical 卡号 + `CARD_HINTS`，没有 git 溯源；
  文档又把「行内无卡号 / 看板无对应工作项」写成「没有任何可复核的行级证据」，断言失实。
- **当场证据**：reviewer `d9fda1af` 用 `request_changes` 抽 18 行（15 张卡）全部找到交付卡 +
  非作者 approve + `merged=True` + merge 是 base 祖先，而 merge 时该行仍写「待独立 principal 复核」。
  返工 patch `9a8bd9c9` 加 `git log -S "| T-N |"` 引入 commit + first-parent merge 窗口 diff 溯源后重跑：
  **61 桶中 57 个 T 行闭合为已审已合并改写**，T-2/T-3 归「已合未审」，2 行引用块散文保持 `PROSE_UNCLOSED`。
  最终 reviewer `86ef3156` approve；85 行 = 81 + 2 + 2。
- **修复卡**：`openmir2-whitelist-status-audit` → `f40235b1b`。

### P7 GM 命令名与权限事实订正

- **症状**：执行单/登记行里写了 `@刷怪`、`@召唤`，但这两条**不在** 185 条 GM 命令表；
  抬权还原判据原本想用 `@LoadAdmin` 验证，实际它**只回绿文本、不改权限**。
- **根因**：按俗名书写，没有对 `gmcmd.rs` 的 `COMMANDS` 与 oracle 行为逐条核。
- **当场证据**：reviewer `9335d731` 用 Python 解析 `mir2-rs/bins/game-svr/src/gmcmd.rs` 的 `COMMANDS`
  （5651..6775 行）= **185 条、名字唯一 185**；`name='怪'` ⇒ `Imp::MobSpawn`、
  `name='清除怪物'` ⇒ `Imp::ClearMapMonster` 均存在；`'刷怪'`/`'召唤'` 零命中。
  另确认 `@清除怪物` **恒回红且不抽数** ⇒ 只能作「同一红字 + 零新增抽数」的反向判据；
  红字 Param=`0x38FF(14591)`。
- **修复卡**：`openmir2-whitelist-merged-row-backfill` → `8256e16d0`（命令名与登记行）、
  `openmir2-mcp-admin-window-sheet` → `4d0e1c875`（执行单判据）、
  `openmir2-probe-scenario-redparam-prose` → `61efafed1`（场景散文）。

---

## 3. 证据索引

### 3.1 每条合并的 merge_result 关键结论

下表「账本条目」是 `kind=merge_result` 的协作条目 oid（blob，**不是 commit**）。
`git cat-file -t` 对它返回 `blob` 是正常的；要核提交请用 merge 列。

| merge 提交 | 账本条目 | 关键结论（摘要） |
|---|---|---|
| `dcea680f9` | `ce01211b1f` | slice-1 六工具；185 目录；`conf_set` 白名单写+回滚；tier=L 无窗口项。（`ce176bce` 为空 parent 误签，已由 `ce01211b1f` 勘误取代） |
| `5f14fca15` | `1317a9ea64` | baseline +1（entry_oid 绑定）；既有 9 条零改动；全量 tool_selftest 改前 58 绿/1 红 → 改后 59 绿/0 红 |
| `aa2749620` | `6cb185988d` | slice-2 回环 admin 通道 + `gm_execute`；T-273；**tier=R，window_required=true**；真机三项转台账 |
| `151b5ab17` | `289facbd3914` | `--parent`/`--root` fail-closed、陈旧 body、`body_sha256`；13 例；shim 证 `walgit` 0 次 |
| `81c8fd6a2` | `cb17f4cf5752` | 裸 check 默认加载 baseline；`--no-baseline`；缺失/空/坏 JSON rc=2；56 例 |
| `ceacecb3c` | `f1a52aa9ce` | issue 也探测历史；tmp 目录不增（1065/505 连跑两次不变）；16 例 |
| `cf604f260` | `e0d735d599` | frames 措辞；accept 层守卫牙；`admin_addr_not_loopback`；145 targets / 3524 passed |
| `8256e16d0` | `6da71f8626cb` | T-273 回填；`@怪`/`@清除怪物` 订正；卡面 W3 前提被纠正 |
| `54ed3795b` | `6b4213e81f` | `_rmtree_force`；集合差 49→49→49；变异 `pass` ⇒ 50 |
| `5f5ec4243` | `0bfd270acbf2` | `AdminPeerPolicy` 编译期牙；`[::ffff:127.0.0.1]` 拒绝；T-273 条数 8→9；**145 targets / 3525 passed / 0 failed / 69 ignored** |
| `945df8dd7` | `1e8405bcb5` | 只读普查：已合并 870；零 review 13；仅自审 5；7 张账本 oid ≠ 合并提交 |
| `f40235b1b` | `a4e2070608a0` | 85 = 81 已审已合 + 2 已合未审 + 2 `PROSE_UNCLOSED`；57 行返工闭合；REGISTRY 61、认树锚 61/66 |
| `a1a2ebf46` | `cc76ef0c00` | 口径定稿重跑：872 / 854 / 13 / 5 / 0；legacy 救回 7 卡 8 条 |
| `4d0e1c875` | `a2bcee03d7` | admin 三条真机执行单（全 NOT_RUN）；四条事实订正；525 帧复核 |
| `c8629cb0f` | `dd3c09005e9a` | `RED_PARAM=14591`；G1 按 param 过滤；G3 join characters；530 帧复核 |
| `61efafed1` | `0174286c08` | 场景散文两处；`total_diffs=2`；机器字段零改动 |
| `1634fff4b` | `4dc20d2964` | 字面量绝对锚；M5（两常量同改 9999）现在 rc=1；断言行 21→23 |

### 3.2 tester-1 独立复核（`b1242001`，tree `5f5ec4243`）

- 覆盖提交：`ceacecb3c` / `cf604f260` / `8256e16d0` / `54ed3795b` / `5f5ec4243`。
- **cargo**（该树实测）：`fmt` rc=0、`clippy --workspace --all-targets -D warnings` rc=0、
  `test --workspace` rc=0 —— **145 targets（126 bins + 19 doc）/ 3525 passed / 0 failed / 69 ignored**、
  `msgcodegen --check` rc=0（604 条常量）。
- 功能复核：admin 准入接缝换闭包 `cargo check` rc=101 `E0308`（还原后 sha 相等）；
  `sign_entry --precheck` 四态（缺 parent rc=1 / 有历史 `--root` rc=1 / 陈旧 body rc=1 / 空线程 `--root` rc=0）；
  裸 `oid_gate check` rc=0（豁免 1 处）。
- 门禁复核：`whitelist_anchor` **611 条 ALL PASS**；`t_reservation` rows=261 / live=70 / landed=191 /
  malformed=0 / max=T-273；conflict 0 across 7701 files；`toolgate_full` 59 绿 / 0 红。
- **数字对账**：`cf604f260` 的 merge_result 记 **3524** passed（该提交时点）；
  `5f5ec4243` 新增用例后 **3525**，tester 判为**合法增量**，不是差异。

### 3.3 工具门与锚（`1634fff4b` 实测）

```sh
python -I -B tools/hygiene/tool_selftest_gate.py --inventory
# 清点：66 个带 selftest 的工具；门禁内 61；已分类出域 5；未分类 0
cd mir2-rs && python -I -B tests/parity/m3/whitelist_anchor_gate.py check
# RESULT: ALL PASS（611 条锚：行号锚 ±3 命中声明符号；高 churn 文件无行号锚；仓外件登记 EXTERNAL）
```

- 工具门：**61 项在门 / 清点 66 = 61 + 5 + 0**（`--inventory` rc=0）。
- whitelist 锚：**611 条** ALL PASS（`whitelist_anchor_gate check` rc=0）。
- 以上两个数字与 tester-1 `b1242001` 的 **59/64** 不同，原因是之后又并入了
  `merge_without_review_audit` 与 `wl_status_audit` 两个工具（`945df8dd7` / `f40235b1b` 的合并期 REGISTRY 取并集），
  并在合并期把认树锚同步为 61/66。**59/64 是 2026-11-01 20:45 的时点值，不是现况。**

---

## 4. 未完成与待办

> **未跑 ≠ 通过**。以下项目没有真机证据，不能写成已验收。

### 4.1 MCP admin 三条真机项（NOT_RUN，等独占窗口）

执行单：**`docs/mcp-admin-window-sheet-20261101.md`**（847 行，全部 `NOT_RUN`）。三条判据：

1. `@怪`（`Imp::MobSpawn`）/ `@清除怪物`（`Imp::ClearMapMonster`）经 **admin 通道**执行，
   与脚本 `GMEXECUTE` 路径的**抽数序列与帧面**等价（真机双栈对拍）。
   注意 `@清除怪物` 恒回红且不抽数，只能作「同一红字 + 零新增抽数」的反向判据。
2. 真栈上 `AdminPort` 开/关两种配置下客户端帧面 **diff = 0**。
3. 抬权只作用于单次调用、不落盘：基线 / 单次调用后 / 重登后三条都要一致。
   角色名与角色配置以执行单为准（执行单使用角色 `aaa` 这一类专用夹具角色）；
   证据根预定 `E:/tmp/admin-window-20261101/<RUN>/`（**窗口未开，该目录当前不存在**）。

状态：`T-273` 是 R 级卡，按判据分级规则**保持 in-progress**，不得在真机项未跑时按已验收收口。

### 4.2 T-2 / T-3「已合未审」事实

- T-2（`whitelist.md` 第 227 行）与 T-3（第 228 行）来自 `openmir2-s1-oracle-defects`；
  该线程只有 issue/comment/status×5/patch/merge_result，**零 `kind=review`** ⇒ 无独立 principal approve。
- 实际合并提交 `1894ccd50`（父 `a978da5c` + `607e9b77`）；账本 `merge_result.oid` 记的是部署对账时的
  master tip `cc252146`（`git cat-file -t` 为 commit，首父是 `1894ccd50`，**不是该卡的合并提交**）。
- 处理：登记表按事实保留「已合未审」，**不能写成已审**。

### 4.3 两条 Minor（abs-anchor）

- 注释里 `make_word16(f,b)` 与 Rust 形参名 `low,high` 措辞不一致（`1634fff4b` 的 reviewer 留）。
- 字面量锚可被「连测试字面量一起改」绕过——属**固有属性**：锚只保证「常量值与 0x38FF 一致」，
  不能阻止有人同时改实现与测试字面量；防这类改动要靠审查与 oracle 对照，不是单测。

### 4.4 `sign_entry` 的 F3（Python 3.12 `onerror` deprecated）

`rmtree(..., onerror=_rmtree_force)` 的 `onerror=` 在 Python 3.12 起 deprecated、3.14 移除；
`sign_entry.py` 与 `oid_gate.py` 同形，**不是本轮引入**。当前工具链仍可跑（selftest rc=0），
升级 Python 前需改成 `onexc=` 或别的清理方式。

### 4.5 本轮新发现的观察（写进报告，供后续开卡）

- **O1（重要）：baseline 豁免在默认扫描域内没有被行使。** `oid_gate check` 默认只扫最近 200 条；
  当前库 13889 条，登记的历史坏 parent 已不在窗口内，于是 `baseline 命中 0 / 未命中 10`，
  `--no-baseline` 也 rc=0。**这不代表坏条目已修**：`verify-entry 2113986cd0e8…` 仍 rc=1。
  建议：定期跑一次更大 `--limit` 的全库检查，或给审计路径一个「至少覆盖 baseline 全部 entry_oid」的断言。
- **O2：`%TEMP%/mir2-mcp-backups/` 当前 155 个 `.bak`**（`conf_set` 测试留下，单文件 76–143 B），
  文档 §5 已登记「本片不做自动清理」，属已知未变项，不是泄漏回归。
- **O3：`sign_entry` F2 仍开**：30 分钟内残留 body 不会被拦；`--allow-stale-body` 放行无痕迹。

---

## 5. 残留物清单

### 5.1 本线（dev-1）留下的东西

| 路径 | 状态 | 说明 |
|---|---|---|
| `/tmp/dev1_tmpdir.txt`、`/tmp/threaddump.py`、`/tmp/td.py`、`/tmp/tm.py`、`/tmp/e_*.txt` 及 `mktemp -d` 目录 | **交付前已删除** | 本报告取证用的临时 body / 脚本 / 日志；删后 `git status` 干净 |
| `E:/UserTemp/tmp.R8H9xXeOy4/`（fresh/stale body） | **交付前已删除** | `mktemp -d` 生成；不得留在 UserTemp |
| `E:/tmp/oid-gate-dev1/` | **不存在** | `openmir2-oid-gate-default-baseline` 卡的临时夹具，卡收口时已删 |
| `E:/tmp/admin-window-20261101/` | **不存在** | 执行单预定的窗口证据根；**窗口未开，未创建** |

### 5.2 其他卡/车道留下、需要保留或另案处理的

| 路径 | 归属 | 处理 |
|---|---|---|
| `E:/tmp/mwr-audit/`（286 KB） | `openmir2-merge-without-review-audit` | **保留**：该卡文档 §8 已登记；`report.json` 是普查表的机器可读证据。复核后可删 |
| `E:/UserTemp/oid-gate-*`（**1065** 个） | 历史 `oid_gate --selftest` | **止血未清**：`ceacecb3c` 后连跑两次 1065→1065 不增；既有目录属旧运行，删除需另行批处理，本轮不动 |
| `E:/UserTemp/oid-audit-*`（**505** 个） | 历史 `oid_gate` 审计运行 | 同上，505→505 不增 |
| `E:/UserTemp/sign-entry-selftest-*`（**50** 个，as of 2026-11-02 00:5x） | `sign_entry` 自测旧运行 | `54ed3795b` 已修复清理逻辑（reviewer 基线 49）；现值 50 的 +1 属修复前/旧代码运行，未删除 |
| `E:/UserTemp/mir2-mcp-backups/`（**155** 个 `.bak`） | `mir2-mcp` `conf_set` 测试 | 文档已登记「不做自动清理」；不是泄漏回归 |
| `E:/tmp/issue_*.json`、`E:/tmp/oid-*.json`、`E:/tmp/dev3_sign_*.py` 等 | 各卡作者/协调者 | 不入库；owner 可批量清理，本轮不代删他人文件 |

### 5.3 `E:\MirServer` 与运行栈：全程未被本线触碰

- `E:/MirServer/M2GameSvr/Server.conf`：
  `sha256 = ce5e3ce98b9bb71cbb18e6be1501e20e05642fd3eeaaffd9326caef8d9a4c0e2`，
  mtime `2026-10-09 23:38`（本轮无写入；与开窗前的 `ce5e3ce9…` 一致）。
- 十端口（3306 / 5000 / 5100 / 5500 / 5600 / 5700 / 6000 / 7000 / 7100 / 7200）
  在本报告生成时 **全部处于监听**，缺失端口为空（只读查询 `Get-NetTCPConnection -State Listen`）。
- 本线**没有**启动/停止任何服务、没有开窗口、没有跑构建；所有 cargo 数字均引用各卡
  merge_result / tester-1 的**当时实测**，不是本次重跑。

---

## 6. 本报告自身的门禁（rc 表）

在 `card/night-report-20261102-dev1` 的交付 head 上直取 rc（详见 `status: needs-review` 条目）：

| 门 | 命令 | rc |
|---|---|---|
| docs_delivery_gate | `python -I tools/hygiene/docs_delivery_gate.py` | 0 |
| docs_anchor_gate | `python -I tools/hygiene/docs_anchor_gate.py` | 0 |
| dynamic_count_stamp_gate | `… --ref HEAD` | 0 |
| stale_pointer_gate | `… --ref HEAD` | 0 |
| conflict_marker_gate | `… --ref HEAD` | 0 |
| t_reservation_gate | `python -I tools/whitelist/t_reservation_gate.py check` | 0 |
| git diff --check | `git diff --check $(git merge-base HEAD walgit/master) HEAD` | 0 |

**未跑**：`cargo fmt/clippy/test/msgcodegen`——本卡是纯文档、无代码改动，按任务卡要求不跑构建。
