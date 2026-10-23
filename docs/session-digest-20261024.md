# 2026-10-24 会话摘要（截至 master `c4b66ddb1`）

> **范围**：`git log --first-parent --merges bb4317a01..c4b66ddb1` 的 **36** 次合并（L **28** / R **8**），外加今天关闭、今天新开仍在开的卡。
> **取数时点**：2026-10-24 **07:18:30 +08**（master=c4b66ddb1）；看板快照 `walgit collab board --repo . --format json`（503 张卡，快照 epoch `1792797510`）。
> **口径**：合并只数 **master 第一父链**上的 merge（`--first-parent`），分支内部把 master 合进卡片的 merge 不计；卡号由线程里 `merge_result.oid`（短 oid 前缀匹配 merge commit）确定，tier 取该 `merge_result.tier`（36/36 张都有）。
> **R 卡一律只写「已合并、窗口未跑」**，不写「已完成验收」；`tier=R` 的卡按策略保持 `in-progress`、不 closed。
> **数字冻结在上述快照**：§0 的复算片段用 `CUTOFF=1792797510` 从线程条目**重建当时状态**，所以任何时刻重跑都应得到同样的 `closed=38 new_open=11`；想数「现况」就把 `CUTOFF` 换成当前 epoch。

## 0. 数字与复算命令

| 数 | 值 | 复算 |
|---|---:|---|
| 第一父链合并 | 36（L 28 / R 8） | `git log --first-parent --merges --format=%h bb4317a01..c4b66ddb1 \| wc -l` |
| 今天关闭的卡 | 38 | §0 片段输出 `closed=` |
| 今天新开仍在开 | 11 | §0 片段输出 `new_open=` |
| 仍开着且已合并的 R 卡 | 8 | 看板里 `status!=closed` 且出现在 §2 的 R 表 |

看板列（同一快照）：已完成 **262** / 进行中 **234** / 待审 **5** / 受阻 **2** / 待认领 **0** / 需要人 **0**（共 503 张卡）。

```sh
# 复算 B/C：用 CUTOFF 从线程条目重建「当天」状态，一次遍历同时算两项
#   输出应为：CUTOFF=1792797510 closed=38 new_open=11
#   成本约 4 分钟（503 张卡各 1 次 `walgit collab thread`）
python -I - <<'EOF'
import json, subprocess, datetime
TZ = datetime.timezone(datetime.timedelta(hours=8))
DAY = int(datetime.datetime(2026, 10, 24, tzinfo=TZ).timestamp())   # 当天 00:00 +08
CUTOFF = 1792797510          # 本文件看板快照时刻（2026-10-24 07:18:30 +08）
b = json.loads(subprocess.run(["walgit", "collab", "board", "--repo", ".", "--format", "json"],
                              capture_output=True, text=True).stdout)
ids = [c["id"] for col in b["columns"] for c in col["cards"]]
closed = new_open = 0
for i in ids:
    t = json.loads(subprocess.run(["walgit", "collab", "thread", "--repo", ".", i],
                                  capture_output=True, text=True).stdout)
    hist = [(int(e["entry"]["ts"]), e["entry"]) for e in t if int(e["entry"]["ts"]) <= CUTOFF]
    st = [x for x in hist if x[1]["kind"] == "status"]
    cur = (st[-1][1].get("body") or {}).get("status") if st else None
    if st and cur == "closed" and st[-1][0] >= DAY:
        closed += 1
    if hist and DAY <= hist[0][0] <= CUTOFF and cur != "closed":
        new_open += 1
print("CUTOFF=%d closed=%d new_open=%d" % (CUTOFF, closed, new_open))
EOF
```

> 快速近似（**不可用于复算**）：`board` 里 `status==closed and last_ts>=今天` 会多算「今天只收到 comment 的旧收口卡」（快照时实测多 2 张：`openmir2-d2-m3-fixtures`、`openmir2-window-magic-fixture`）；且它数的是**现况**不是快照。所以「今天关闭」必须按**最后一条 status 的 ts**（且 `<= CUTOFF`）来判。

## 1. 合并表（36 次，按时间升序）

| 时间 | commit | 卡号 | tier | 一句话结果 | 新增窗口项 |
|---|---|---|---|---|---|
| 02:13 | `19009c6a0` | `openmir2-goal-gate-unmerged-window-bucket` | L | 条件②新增「有登记但未合并」报告分类 | 否 |
| 02:19 | `a30a6fa79` | `openmir2-window-criteria-template` | L | 窗口判据六字段模板 + 只告警的 lint（返工） | 否 |
| 02:22 | `a112e59b5` | `openmir2-scenario-recog-packing-fix` | L | 动作族 recog 打包口径订正（接管返工） | 否 |
| 02:32 | `059a03900` | `openmir2-window-criteria-backfill` | L | 窗口判据补齐首批 10/20（返工） | 否 |
| 02:39 | `b5b3c47cf` | `openmir2-goal-gate-unmerged-selftest` | L | 未合并窗口桶补仓内合成用例 | 否 |
| 02:42 | `a9455165f` | `openmir2-script-envir-root-host` | R | HostNpc envir_root 覆写 + 文件列表类脚本接线（返工） | 是（见 §2） |
| 02:44 | `196439400` | `openmir2-script-user-account` | R | 脚本宿主 user_account 接线 + SessionGround.account 生产缝修复 | 是（见 §2） |
| 02:55 | `95b169ecc` | `openmir2-param-discovery-oracle-text-fix` | L | COMPUTED_ORACLES 文案按 ident 订正（接管返工） | 否 |
| 03:06 | `7bb48239a` | `openmir2-spell-cast-unblock` | L | 解除 spell_cast 的 BLOCKED-FIXTURE（5 轮审查） | 否 |
| 03:14 | `d9cabfce7` | `openmir2-card-window-teeth-mg7` | L | 补 F5 四颗牙 + MG-7 + inventory 读判定 | 否 |
| 03:22 | `01d4f5877` | `openmir2-script-sendmsg-host` | R | SENDMSG 宿主接线 + @StdModeFunc* 全服扇出（返工） | 是（见 §2） |
| 03:28 | `8dcb8c8ca` | `openmir2-attack-weapon-wear-642` | R | 攻击武器耐久磨损 + SM_DURACHANGE(642)（返工） | 是（见 §2） |
| 03:59 | `05ca859bb` | `openmir2-whitelist-t247-642-text` | L | 白名单 T-247 的 642 状态订正 | 否 |
| 04:03 | `b3f48ab49` | `openmir2-sendmsg-login-qmanage-cases` | R | @Login/QManage 排水点 + @PlayDie 不可达登记 | 是（见 §2） |
| 04:14 | `c9a076bfe` | `openmir2-window-batch-rehearsal` | L | 开窗前的 20 场景彩排表（返工） | 否 |
| 04:31 | `061312242` | `openmir2-642-stale-text-sweep` | L | 642/643 过期断言的逐处订正（接管返工） | 否 |
| 04:40 | `9083f4863` | `openmir2-scenario-fixture-precondition` | L | 场景 requires 前置 fail-closed | 否 |
| 04:43 | `62db66c7e` | `openmir2-rehearsal-stale-lines` | L | 彩排表里已被合并推翻的两处表述 | 否 |
| 04:57 | `8b40443b6` | `openmir2-probe-waitfor-recog-filter` | L | wait_for/wait_idents/wait_frames 增加来源过滤 | 否 |
| 05:06 | `1f1a69c84` | `openmir2-rehearsal-magic-preview-count` | L | 彩排表 magic 预览计数订正 20/20 → 18/20 | 否 |
| 05:10 | `71420a145` | `openmir2-script-broadcast-tx-gating` | R | @Login 全服广播不再被触发者网关通道把关 | 是（见 §2） |
| 05:21 | `aaefd6d1e` | `openmir2-rehearsal-spellcast-classification` | L | spell_cast 由「假绿」改判「已 fail-closed」 | 否 |
| 05:31 | `39ec8a97f` | `openmir2-window-criteria-backfill-2` | L | 剩余 10 张窗口判据补齐（接管返工 v3） | 否 |
| 05:35 | `d38acf25b` | `openmir2-ability-weaponstrong` | R | AddAbil.WeaponStrong 建模 + Desc[7] 覆盖（接管返工） | 是（见 §2） |
| 05:37 | `55a8e7fca` | `openmir2-docs-state-drift-sweep` | L | 状态词扫除 v6（接管返工） | 否 |
| 05:54 | `e377d7058` | `openmir2-needs-human-refresh-20261024` | L | owner 决策清单刷新（第三版） | 否 |
| 06:01 | `08e8b318a` | `openmir2-broadcast-tx-source-order-gate` | L | 给「脚本广播不被自身 tx 把关」加源码序门 | 否 |
| 06:08 | `b1a558c7c` | `openmir2-rehearsal-combat-filter-stale` | L | recog 过滤已具备，两张 combat 场景的真实缺口是夹具 | 否 |
| 06:11 | `65d6e7134` | `openmir2-spell-cast-window-blockers` | L | spell_cast 的两条排除理由被逐条否掉，真挡项是比较器缺规则 | 否 |
| 06:19 | `c0f8d65a8` | `openmir2-card-window-var-passthrough` | L | card_window 的 --var 透传 | 否 |
| 06:26 | `89a3d02fd` | `openmir2-spell-frame-compare-rules` | L | 补 53/638/639 三条比较规则 | 否 |
| 06:36 | `9426be77b` | `openmir2-spell-cast-note-and-layout-pin` | L | 638 body 字节数订正 + 开窗 profile pin 说明 | 否 |
| 06:42 | `d54fa4581` | `openmir2-weaponstrong-dura-zero-skip` | R | 属性重算补 oracle 的 Dura==0 跳过 | 是（见 §2） |
| 06:46 | `478ff9a33` | `openmir2-batch-plan-spellcast-runnable` | L | spell_cast 移入「现在就能跑」 | 否 |
| 07:00 | `47fd96ca8` | `openmir2-needs-human-count-sync` | L | owner 决策清单同步到 11 行/16 场景 | 否 |
| 07:16 | `c4b66ddb1` | `openmir2-inventory-fixture-reachability` | L | inventory 夹具可达 + 两侧口径统一（接管返工） | 否 |

## 2. 今天新增/触及的 R 窗口项（8 条，全部**窗口未跑**）

每条的判据与窗口步骤在卡片线程里；这里只给一句话。合并后这些卡按 R 级策略仍在「进行中」。

- `openmir2-script-envir-root-host`（`a9455165f`，02:42）：CHECKNAMELIST 判定 + ADDUSERDATE/ADDNAMELIST 文件落盘 —— 线程：`walgit collab thread --repo . openmir2-script-envir-root-host`
- `openmir2-script-user-account`（`196439400`，02:44）：CHECKACCOUNTLIST/DELACCOUNTLIST 判定 + 账号名单文件落盘 —— 线程：`walgit collab thread --repo . openmir2-script-user-account`
- `openmir2-script-sendmsg-host`（`01d4f5877`，03:22）：SENDMSG 全服/附近广播的收件人集合 + 同 ident 多帧时序 —— 线程：`walgit collab thread --repo . openmir2-script-sendmsg-host`
- `openmir2-attack-weapon-wear-642`（`8dcb8c8ca`，03:28）：攻击磨损量 `(ushort)(Random(5)+2-WeaponStrong)` 与 642 帧/存档耐久 —— 线程：`walgit collab thread --repo . openmir2-attack-weapon-wear-642`
- `openmir2-sendmsg-login-qmanage-cases`（`b3f48ab49`，04:03）：@Login/QManage 排水点上的广播收件人集合与时序 —— 线程：`walgit collab thread --repo . openmir2-sendmsg-login-qmanage-cases`
- `openmir2-script-broadcast-tx-gating`（`71420a145`，05:10）：@Login 全服广播不再被触发者网关通道把关后的收件人集合 —— 线程：`walgit collab thread --repo . openmir2-script-broadcast-tx-gating`
- `openmir2-ability-weaponstrong`（`d38acf25b`，05:35）：同一条磨损量的取数站点（RNG 站点保位）+ Desc[7] 覆盖 —— 线程：`walgit collab thread --repo . openmir2-ability-weaponstrong`
- `openmir2-weaponstrong-dura-zero-skip`（`d54fa4581`，06:42）：`Dura==0` 整槽跳过对 `WeaponStrong` 的影响（决定磨损量） —— 线程：`walgit collab thread --repo . openmir2-weaponstrong-dura-zero-skip`

## 3. 今天关闭的卡（38）

| 卡号 | 关闭时间 | 收口者 |
|---|---|---|
| `openmir2-magic-fixture-selftest-gaps` | 10-24 00:11 | openmir2-orchestrator |
| `openmir2-scenario-combat-hit-waitfix` | 10-24 00:12 | openmir2-orchestrator |
| `openmir2-multi-session-probe` | 10-24 00:59 | openmir2-orchestrator |
| `openmir2-magic-fixture-assertion-strength` | 10-24 01:04 | openmir2-orchestrator |
| `openmir2-board-index-reconcile` | 10-24 01:11 | openmir2-orchestrator |
| `openmir2-r-window-registry-closeout` | 10-24 01:28 | openmir2-orchestrator |
| `openmir2-whitelist-review-status-audit` | 10-24 01:32 | openmir2-orchestrator |
| `openmir2-window-batch-plan-refresh` | 10-24 01:32 | openmir2-orchestrator |
| `openmir2-window-preflight-script` | 10-24 01:38 | openmir2-orchestrator |
| `openmir2-window-criteria-quality-audit` | 10-24 01:43 | openmir2-orchestrator |
| `openmir2-goal-gate-unmerged-window-bucket` | 10-24 02:14 | openmir2-orchestrator |
| `openmir2-window-criteria-template` | 10-24 02:19 | openmir2-orchestrator |
| `openmir2-scenario-recog-packing-fix` | 10-24 02:23 | openmir2-orchestrator |
| `openmir2-window-criteria-backfill` | 10-24 02:32 | openmir2-orchestrator |
| `openmir2-goal-gate-unmerged-selftest` | 10-24 02:39 | openmir2-orchestrator |
| `openmir2-param-discovery-oracle-text-fix` | 10-24 02:55 | openmir2-orchestrator |
| `openmir2-spell-cast-unblock` | 10-24 03:07 | openmir2-orchestrator |
| `openmir2-card-window-teeth-mg7` | 10-24 03:15 | openmir2-orchestrator |
| `openmir2-whitelist-t247-642-text` | 10-24 03:59 | openmir2-orchestrator |
| `openmir2-window-batch-rehearsal` | 10-24 04:14 | openmir2-orchestrator |
| `openmir2-642-stale-text-sweep` | 10-24 04:31 | openmir2-orchestrator |
| `openmir2-scenario-fixture-precondition` | 10-24 04:43 | openmir2-orchestrator |
| `openmir2-rehearsal-stale-lines` | 10-24 04:43 | openmir2-orchestrator |
| `openmir2-probe-waitfor-recog-filter` | 10-24 04:57 | openmir2-orchestrator |
| `openmir2-rehearsal-magic-preview-count` | 10-24 05:07 | openmir2-orchestrator |
| `openmir2-rehearsal-spellcast-classification` | 10-24 05:22 | openmir2-orchestrator |
| `openmir2-window-criteria-backfill-2` | 10-24 05:38 | openmir2-orchestrator |
| `openmir2-docs-state-drift-sweep` | 10-24 05:38 | openmir2-orchestrator |
| `openmir2-needs-human-refresh-20261024` | 10-24 05:55 | openmir2-orchestrator |
| `openmir2-broadcast-tx-source-order-gate` | 10-24 06:02 | openmir2-orchestrator |
| `openmir2-rehearsal-combat-filter-stale` | 10-24 06:08 | openmir2-orchestrator |
| `openmir2-spell-cast-window-blockers` | 10-24 06:11 | openmir2-orchestrator |
| `openmir2-card-window-var-passthrough` | 10-24 06:20 | openmir2-orchestrator |
| `openmir2-spell-frame-compare-rules` | 10-24 06:26 | openmir2-orchestrator |
| `openmir2-spell-cast-note-and-layout-pin` | 10-24 06:36 | openmir2-orchestrator |
| `openmir2-batch-plan-spellcast-runnable` | 10-24 06:46 | openmir2-orchestrator |
| `openmir2-needs-human-count-sync` | 10-24 07:00 | openmir2-orchestrator |
| `openmir2-inventory-fixture-reachability` | 10-24 07:16 | openmir2-orchestrator |

> 关闭数（38）≠ 合并数（36）：28 张对应 §1 表里的合并，其余 10 张的合并发生在 `bb4317a01` 之前（不在 §1 表内）；§1 里 8 张 R 卡按策略未 closed。

## 4. 今天新开、现在仍开着（11）

| 卡号 | owner | 状态 | 一句话 |
|---|---|---|---|
| `openmir2-attack-weapon-wear-642` | openmir2-orchestrator | in-progress | 已合入 master 8dcb8c8ca（tier=R，approve reviewer-6 dbcf22f8 第二轮）。攻击成功磨损 + 642 帧已接；尾桶门照 oracle 怪癖；equip_struck_save 场景 dry… |
| `openmir2-ability-weaponstrong` | openmir2-orchestrator | in-progress | 已合入 master d38acf25b（tier=R，approve reviewer-6 d3868d66 第二轮）。WeaponStrong 建模 + Desc[7] 覆盖 + RNG 站点保位。**R 卡不 closed**：… |
| `openmir2-sendmsg-login-qmanage-cases` | openmir2-orchestrator | in-progress | 已合入 master b3f48ab49（tier=R，approve reviewer-14 8a9b5dab）。@Login/QManage 排水点接线（复用既有扇出）、@PlayDie 不可达登记 T-267、T-224 三处陈… |
| `openmir2-script-broadcast-tx-gating` | openmir2-orchestrator | in-progress | 已合入 master 71420a145（tier=R，approve reviewer-14 831c7594）。@Login 全服广播不再被触发者网关通道把关；其余四漏斗已核一致。**R 卡不 closed**：真机窗口未跑。 |
| `openmir2-docs-stale-pointer-gate` | openmir2-worker-63 | needs-review | 过期指针门 + 首次接入：selftest 7/7、master 全量 OK（4 处真实命中已订正/白名单、2 条白名单含 rot 检测）；未改 src/规则/场景/whitelist，未起栈未连库。请 reviewer 独立复跑 -… |
| `openmir2-weaponstrong-dura-zero-skip` | openmir2-orchestrator | in-progress | 已合入 master d54fa4581（tier=R，approve reviewer-9 dc504cf8，findings=0）。**R 卡不 closed**：真机窗口未跑。 |
| `openmir2-stale-pointer-card-status-check` | openmir2-worker-23 | in-progress | 核对 stale pointer 卡状态并留证据 |
| `openmir2-gate-baseline-snapshot` | openmir2-worker-24 | needs-review | 已交付 head `3bd8c6580`（patch `9654badb0`，base `478ff9a33`，tier=L）：`docs/gate-baseline-478ff9a33.md` 记录 19 项门禁现取结果——fmt/… |
| `openmir2-leader-process-lessons-20261024` | openmir2-worker-24 | needs-review | 已按 reviewer-6 d7bc2322 返工 I-1/I-2/I-3 + 两条 Minor：规则改 14–20 并写明与规则 20 同源（订正 patch 64525f8e5 的事实错误）；dead claim 四证据语义改「c… |
| `openmir2-session-digest-20261024` | openmir2-worker-24 | needs-review | 已交付 docs/session-digest-20261024.md：35 次合并表（L27/R8，tier 取 merge_result 35/35）+ 今天关闭 37 张 + 今天新开仍开 11 张（含 owner/状态）+ 8… |
| `openmir2-rehearsal-refresh-478ff9a33` | openmir2-worker-58 | needs-review | 20 场景三态矩阵已在 master 47fd96ca8 上重跑并刷新；分类 16 可跑 / 2 blocked / 2 假绿；spell_cast 三态写全且前置写死 D-9 pin；gate rc 全 0 |

> 另有 **3 张 R 卡是昨天开的、今天合并**，按策略同样留在 `in-progress`、不在上表：`openmir2-script-user-account`、`openmir2-script-envir-root-host`、`openmir2-script-sendmsg-host`。所以「仍开着的已合并 R 卡」一共 **8** 张。

## 5. 对 owner 而言，今天变了什么

1. **8 张 R 卡合入、一条窗口没跑**：攻击磨损与 642 帧、`WeaponStrong`（含 `Dura==0` 跳过）、SENDMSG 全服/附近广播两条、@Login 排水点、脚本账号/名单落盘两条、脚本广播不被自身 tx 把关。真机窗口现在是唯一能推进条件② 的事（排期见 `docs/needs-human-20261024.md` §1）。
2. **窗口从「没有判据」变成「照表跑」**：20 场景彩排表 + 20/20 判据补齐（六字段模板、只告警 lint）合入；`spell_cast` 由「假绿」改判为**可跑**，但必须 profile 带 `magic` 段且 `sides.rust.env.MIR2_CLIENT_ABILITY_LAYOUT="0"`，「今天就能跑」的枚举在 `docs/window-batch-plan-20261023.md` §7.4（现为 **11 行 / 16 场景**，`478ff9a33`）。
3. **循环能自己判的更多了**：未合并窗口桶分类、`goal_gate` 合成用例、场景 `requires` 前置 fail-closed、probe `--var` 透传、比较器补 53/638/639 规则、`card_window` 四颗 F5 牙 + MG-7 + inventory 读判定、inventory 夹具可达（两侧口径统一，`c4b66ddb1`）。留给人的仍是 `needs-human` §0 的五件事（窗口授权、`rules.json` PENDING 裁定、`T-161..T-222` 口径、三张卡替代判据、私有 C# 栈去留）。
4. **收口 38 张卡、36 次合入，11 张今天新开还在跑**：其中 3 张是文档交付（gate 基线、流程复盘、本摘要）在待审/在写，1 张（`openmir2-stale-pointer-card-status-check`）在核「stale pointer 卡状态」。
5. **R 卡在看板上仍显示「进行中」是设计如此**，不是卡壳：`merged but window_required` 一律不 closed，看板的「进行中」里因此长期压着 R 卡（这也是 `goal_gate` 条件② 的输入）。

## 6. 交叉链接与尚未合入的件

- `docs/needs-human-20261024.md`（**已在 master**）：owner 决策清单，§0 五件事；窗口枚举引用 batch-plan §7.4。
- `docs/gate-baseline-478ff9a33.md`：**在办**，卡 `openmir2-gate-baseline-snapshot`（head `3bd8c6580`，待审/返工中），未合入 master。
- `docs/leader-process-lessons-20261024.md`：**在办**，卡 `openmir2-leader-process-lessons-20261024`（head `744023506`，待审/返工中），未合入 master。
- `docs/inventory-fixture-reachability-20261024.md`：**已合入 master**（`c4b66ddb1`，接管返工，tier=L）。
- 本文件只写 `docs/`，未改 `src/**`、`tools/**`、场景 JSON、`rules.json`、`whitelist.md`、任何卡片状态。
