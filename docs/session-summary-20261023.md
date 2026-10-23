# 2026-10-23 全天收尾快照（供下一会话恢复）

> 快照：master `4518ec04` @ 2026-10-23 19:2x +0800。**合并 20 张卡**、关闭约 16 张。
> 恢复循环：读 `docs/leader-goal-20261023.md`（终点四条件）→ 本文件（今日战果与欠账）→ `docs/leader-queue-20261023.md`（纪律 1–13 条）。

## 1. 四条件现状

| 条件 | 状态 | 依据 |
|---|---|---|
| ① 脚本派发 100% | **PASS** | parse-stats 106/106 |
| ② R 卡证据 | **FAIL（唯一实质缺口）** | **173 张**欠真机窗口；分批见 `docs/window-batch-plan-20261023.md`（第 1 批 52 可闭环） |
| ③ B 类清零 | **PASS** | `recon_gates dispatch` → `tracked = 0`（counts 键完整） |
| ④ 门禁恒绿 | **PASS** | 141 suites / 3057 passed / 0 failed；anchor 515；gm_registry 85；T 预约表 202；noop audit 三档全绿 |

## 2. 今日合并的 20 张（按主题）

**协议/帧面**：pickwire4（三条脚本帧 + 四叉堆排水）、frame-drain（5 处同 ident 分叉统一）、sm-skeleton（屠宰效果体）、showhp53（53 广播）；**GM 面**：gm-registry（88 条四分类）、gm-shutup、showeffect、startquest、exec-seams（商人/QFunction/组队三路装配）；**脚本宿主**：changexp（经验写回）、action-var-text、dyn-var-host（返工中）；**工具链**：card_window 五连（初版→加固→账本→值断言→接线）、param-discovery、discovery-rules、char-placement；**清点/文档**：recon-unreachable、host-noop-audit、window-batch-plan。

## 3. 今日抓到的「假绿」类型（这是今天最值钱的产出）

> **晚场又抓到 3 类**（自检位置、假红、工具弄脏树），清单已从 6 类扩到 **9 类**并给每类补了
> 可机判的 30 秒自查信号 —— 见 §7.3。下面这 6 类是白天口径，原样保留。

1. **空集合判绿**：`check-all` 空目录 rc=0、`goal_gate` 缺 `counts.tracked` 键、`param_discovery` 空 `--var` rc=0。
2. **假 PASS 账本**：`--post-comment` 在产物缺件时仍写 PASS（已修，账本解禁但需先跑两道真窗口门禁）。
3. **断言了函数没断言调用点**：5 次（sm-skeleton 宿主门、pickwire 排水、CHANGEEXP 接线、smake/clearbag 的 CM_SAY-only 落地）。
4. **判据前提错**：`GMEXECUTE` **不展开** `<$…>`（三张 GM 卡窗口判据全栽）、`..\` 路径未剥离（部署 238 行恒假）、`tracked>0 ≠ FAIL`。
5. **证据哈希不是从提交树取**：4 次。
6. **行号漂移误红/漏红**：默认档只 WARN（有意），`--strict-lines` 才是「台账没欠账」的机判点。

**纪律 1–13 条**已全部落在 `docs/leader-queue-20261023.md`，其中 11（环境噪声三型）、12（签名演化）、13（strict-lines 触发条件）是今天新增。

## 4. 窗口开跑前还差的（按顺序）

1. `openmir2-window-rust-binary-evidence`（**在做**）：`alt` profile 的 `rust.binaries` 指向 .NET 程序集 ⇒ 不修则第一批 173 张的 Rust sha256 全记错。
2. `openmir2-discovery-bagitems-201`（**在做**）：解锁 `eat_item`；否则 `exec-seams` ② 路恒 rc=2。
3. `openmir2-scenario-npc-script-chain`（**在做**）：R1，覆盖第 3 批 9 张。
4. `openmir2-window-gate-two-runs`（**待认领，需栈权限**）：两道真窗口门禁，过后才算「接线可用」。
5. `openmir2-script-sendmsg-host`（**待认领**）：部署 806 行 SENDMSG 里 **349 行目前零帧**（case 0-3 全服广播 318 + case 4 NPC 喊话 31），玩家直接可见。

## 5. 需要 owner 的五件事

见 `docs/needs-human-20261023-evening.md`：① 独占栈窗口；② 35 张不可对拍的裁决；③ 外部车道返工吞吐；④ whitelist T-161..T-222 复核；⑤ 私有 C# 栈长期保留。

## 6. 资源

`E:` 余 **644G（84%）**，`/e/tmp` 下 **129 个 target**（单个满量约 15G）。**明早开工前先清已收口卡对应 worktree 的 `target/`**（只清 `*-target`，绝不碰 worktree）。

---

## 7. 晚场（20:00–21:45）

> **快照戳**：合并账 `git log --oneline --merges 6665e1178..HEAD` **现取**于 master **`1893470a0`** @ **2026-10-23 21:44:09 +0800**；
> 线程/看板取自 `git fetch walgit '+refs/collab/inbox/*:refs/collab/inbox/*' '+refs/collab/meta/*:refs/collab/meta/*'` @ **21:47 +0800**（7176 条签名条目）。
> 身份一律写**签名条目 `actor` + entry `oid`**；`git log` 的 author 是共享工作副本的配置噪声，**不作为作者依据**（纪律 16，`docs/leader-queue-20261023.md:95-98`）。

### 7.1 晚场合并账（8 条，`git log --oneline --merges 6665e1178..HEAD` 现取）

| # | merge oid（时间） | 一句话 | tier | 审查：签名 actor @ review oid（verdict） |
|---|---|---|---|---|
| 1 | `f7a9deee1` 20:21 | GM 四卡整合：VV踢人 / MobFireBurn / 改属性 / 清理垃圾（返工后一次合入；`merge_result` 四条都指同一 oid `269ddcf7`/`48c2adc1`/`60ef19a5`/`be9b0a26`） | **R** | `openmir2-reviewer-14` @ `b1dcb1767d`（approve，整合复审）；四卡单审 `3056d59af0`/`b064828aa`/`126150ae4`/`ec205d7828`（均 reviewer-14，approve） |
| 2 | `57495e25f` 20:41 | `.idea/workspace.xml` 取消跟踪（唯一一处被跟踪的冲突标记）+ 新增冲突标记门禁并机判 | L | `openmir2-worker-66` @ `cc251485ea`（approve；作者 `openmir2-worker-70`，不同 principal）；`merge_result` `ff468876` |
| 3 | `599bf0478` 20:51 | 脚本动作/条件参数里的 `$` 变量展开宿主实现（T-213） | **R** | `openmir2-reviewer-1` @ `7b07a4580e`（approve）；早前 `openmir2-reviewer-5` @ `f6509d9a65`（approve）；`merge_result` `ec97f651` |
| 4 | `57302d3df` 21:07 | `SM_STRUCK(31)` 按字段窄掩码：只掩 `param`，`recog` 走会话 ActorId 全局掩码（不是整帧 ignore） | L | `openmir2-reviewer-1` @ `38b40efce0`（approve；前轮 `926d1b6714` request_changes）；`merge_result` `552610cf` |
| 5 | `302038193` 21:09 | R3 `combat_strike` 场景，交付形态 = **BLOCKED-FIXTURE 登记**（见 §7.4） | L | `openmir2-reviewer-3` @ `0e1912fdb7`（approve；前两轮 `3c15d4edaa`/`ed4633b9a7`）；`merge_result` `c09caecd` |
| 6 | `dd0c2c6f6` 21:33 | `--check-scenario` 形状/语义校验加固 R1–R6（R6 降为 WARN） | L | `openmir2-reviewer-3` @ `1078fc1098`（approve；前轮 `7c8f3f2e16`/`914636f648` reviewer-5、`c36e852bae` reviewer-3）；`merge_result` `c7efafad` |
| 7 | `d5cbb7abf` 21:34 | R2 `upgrade_retrieve` 场景（沙巴克铁匠升级/取回全链） | L | `openmir2-reviewer-6` @ `9d3f9b140e`（approve）；`merge_result` `aad4258c` |
| 8 | `712a948e3` 21:43 | Rust 侧窗口证据：pid 对账接线 + 路径/BOM 修复 + 自检前移计入 rc | L | `openmir2-reviewer-5` @ `e04f9454ff`（approve；前轮 `6a483d573b` request_changes）；`merge_result` `caddb11ca0` |

同一区间还在 first-parent 上的**非合并 docs 提交（4 条，无卡/无 review）**：`ada004320` 20:26 纪律 14（派活前三查）、
`04366adc8` 20:31 车道死活判据、`a9beb4c44` 20:53 纪律 15/16、`1893470a0` 21:44 纪律 17。

> 计数口径：`git log --oneline --merges --first-parent 6665e1178..HEAD` = **8**；同区间 `--no-merges` = **31**（含被合并分支内部提交，不逐条列）。
> `f7a9deee1` 是**一次整合树合并**、四条 `merge_result` 记账，不是四次合并。

### 7.2 三类新教训（卡 / 命令 / 真实 rc / 修法）

#### 教训 A（假绿）：自检加在 `if fail_n: return 1` **之后** ⇒ 旁路整段实现仍打印 GREEN、rc=0

- **卡**：`openmir2-window-inventory-fixture`（head `b8e5f0691`）与 `openmir2-window-rust-binary-evidence`（坏 head `a7cd30f37`）。
- **机判证据（结构，先看这个）**：
  - `git show b8e5f0691:tools/conformance/card_window.py` 的 `if fail_n:` 在 **4393**，`_selftest_inventory(tmp)` 调用在 **4396**（之后）；
  - `git show a7cd30f37:tools/conformance/card_window.py` 的 `if fail_n:` 在 **4168**，`_selftest_pid_binaries(tmp)` 调用在 **4171**（之后）。
- **真实 rc（我自己在树外副本复跑，`E:\tmp\w67-scratch\{fake,fake2}`，未碰本 worktree）**：
  - 把 `if res["stack_rc"] == 0 and sc.get("require_pid_binaries"):` 改成 `if False and …`（旁路整段 pid 对账）：
    - `a7cd30f37` 版：`GREEN: card_window.py selftest 169/169`，**rc=0**；
    - `1893470a0` 版（同旁路）：`RED: card_window.py selftest 173/176`，**rc=1**（3 条接线用例红）。
  - 另在 `_selftest_pid_binaries` 里塞一条必失败项（`return [(False, "SYNTHETIC-FAIL")]`）：
    - `a7cd30f37` 版：打印 `FAIL SYNTHETIC-FAIL…` 之后仍 `GREEN: 165/166`，**rc=0**；
    - `1893470a0` 版：`RED: 168/169`，**rc=1**。
- **reviewer 侧原始读数**（供对照）：`openmir2-reviewer-5` @ `c866f35838`（request_changes，B1）在 `b8e5f0691` 上做四种破坏仍 `GREEN: 170/171` rc=0；
  @ `6a483d573b`（request_changes，B3）在 `a7cd30f37` 上让新增 4 条检查全失败仍 `GREEN: 167/169` rc=0。
- **修法**：把新检查并进 `chk()` 序列、放在 `total = pass_n + fail_n` 与 `if fail_n: return 1` **之前**；
  判定用 `total`。现行 head 的落点：调用 **4325**、`total` **4336**、`if fail_n` **4337**（`tools/conformance/card_window.py`）。
  审卡时的固定问句：**「把它改坏，退出码会变吗？」只变文本不变 rc 的牙是假牙。**

#### 教训 B（假红）：新校验自己判错因果 ⇒ 合法场景被判死，三轮后降级 WARN

- **卡**：`openmir2-probe-scenario-validation-gaps`，引入于 `1c9753803`（R6「攻击族 `send.tag` 必须在 Direction 值域内」写成 `raise ScenarioError`，
  文案：`— an out-of-range direction never hits, so the wait can only time out`）。
- **真实 rc（我在树外副本复跑）**：把 `combat_strike.json` 的 `CM_HIT(3014).send.tag` 改成 `99` 存到 `E:/tmp/w67-scratch/r6_tag99.json`：
  - `1c9753803` 版本：`scenario-error: steps[2].send.tag 99 … never hits, so the wait can only time out`，**rc=2**；
  - `1893470a0`（现行）：`WARN: steps[2].send.tag 99 …` + `scenario OK: combat_strike (4 steps…)`，**rc=0**。
  - 复跑注意：老版本要一份带 `mir2-rs/{crates/protocol/src/messages.rs,bins/game-svr/src/attackwire.rs}` 的树（我用 `E:/tmp/w67-scratch/fake`），否则先死在读表。
- **因果核对（逐跳，oracle ↔ Rust）**：两侧都是**先用状态化 Dir 取目标、后写 nDir**：
  - C#：`src/GameSrv/Player/PlayObject.Attack.cs:26` `IActor attackTarget = targetObject ?? GetPoseCreate();`（用**当前** Dir）→ `:75` `AttackDir(attackTarget, nPower, nDir);` → `src/GameSrv/Actor/BaseObject.Attack.cs:12` `Dir = nDir;` → `:13` `_Attack(nPower, attackTarget)` 用的是**已经选好的** target。
  - Rust：`mir2-rs/bins/game-svr/src/lib.rs:8828` `let pose_dir = …e.ai.dir`；`:8834` 用 `pose_dir` 算出 `(fx, fy)`；`:8840` `e.ai.dir = dir;`；`:8846` `front_target(…, fx, fy)`。
  - ⇒ 越界 `nDir` **不影响本击命中**（`CM_TURN(tag=0)` + `CM_HIT(tag=99)` 第一击照样出 `SM_STRUCK`），它只污染**此后**的状态 Dir。
- **三轮**：`1c9753803` 硬判 → `openmir2-reviewer-5` @ `914636f648`（request_changes，I1 指出机理写反）→ 返工 `6498253cf` 降 WARN
  → `openmir2-reviewer-3` @ `c36e852bae`（又抓到 README 把 R2 说成「删攻击步会红」的假覆盖）→ 返工 `ffb40d1d5` → @ `1078fc1098` approve → 合并 `dd0c2c6f6`。
- **修法**：WARN 级、**不改变 rc**；文案改成「不影响本次命中、只污染此后状态 Dir」；selftest 留**反向断言**（`mir2_probe.py:1939`：`assert "never hits" not in warn and "only time out" not in warn`），
  避免文案回潮。教训：**新校验的文案就是判据的一部分**，写「必然失败」之前先把因果链在 oracle 里逐跳 `sed -n` 核一遍。

#### 教训 C（工具弄脏树）：mutation runner 就地改写 tracked 文件 ⇒ 差点把 live mutation 提交进 master

- **卡**：`openmir2-window-rust-binary-evidence`（合并 `712a948e3`）的收尾阶段；纪律 17 落在 `1893470a0`。
- **机制（读码）**：`tools/conformance/selftest_mutations.py` 的 `TARGET = <本目录>/card_window.py`，在 `:385`/`:389`/`:415` 用 `io.open(TARGET, "w")` **就地写入**变异，
  `finally` 里按原字节还原并校验 sha256。跑的过程中树里存在一条 live mutation。
- **真实后果**：在**主检出**对已 staged 的合并树跑它 ⇒ 变异叠在已 staged 的版本上，`git status` 出现 **`MM`**；
  那一刻提交就会把变异（如 `verdict_reason: "compare_fail" → "ok"`）一起提交。发现后中止并还原，`sha256 a022d291…` 与 `:path` blob 一致（`merge_result` `caddb11ca0` note）。
- **注意**：这条**不是** rc 问题（harness 本身 `GREEN 35/35` rc=0）；牙的 rc 再对，也挡不住「工具自己改树」。
- **修法（纪律 17）**：① 牙/mutation 一律在 worktree 副本跑（`E:/tmp/<lane>-teeth`）；② 每次提交前 `git diff` 必须为空，`MM` 就是警报；
  ③ runner 被中断 ⇒ 先 `git checkout -- <file>`，再比 `sha256sum <file>` 与 `git show :<file>` 的 sha256 一致才提交。

### 7.3 「假绿」清单：**6 类 → 9 类**（每类一条 30 秒机判信号）

> 白天 6 类见 §3，原样保留；下面把 9 类补齐并给**可以 30 秒内跑完**的识别信号。
> 判据统一：**rc 才算牙，stdout 不算**；证据必须能从提交树复算。

| # | 类型 | 30 秒机判信号（不改代码就能看出来） | 今日实例 |
|---|---|---|---|
| 1 | 空集合判绿 | 分母为 0 / 输入集为空而 rc=0：`find <夹具目录> -type f` 计数 0 仍绿；日志出现 `0/N`、`total 0`、`--list` 空表 | `check-all` 空目录、`goal_gate` 缺 `counts.tracked`、`param_discovery` 空 `--var` |
| 2 | 假 PASS 账本 | 产物缺件却写 `verdict=PASS`：`report.json` 无 `run_id`/`compare` 段，或 `verdict==PASS` 而 `blockers` 非空 | `--post-comment` 缺件仍 PASS（已修） |
| 3 | 断言了函数没断言调用点 | 把**调用点**改成 `pass`/旧实现后重跑 selftest，rc 仍 0 | sm-skeleton 宿主门、pickwire 排水、CHANGEEXP 接线、smake/clearbag 的 CM_SAY-only 落地（5 次） |
| 4 | 判据前提错 | 判据里的每个 `file:line` 用 `sed -n 'Np'` 取出的行与描述不符；或判据依赖的键在**部署件**里 `grep` 0 命中 | GMEXECUTE 不展开 `<$…>`、`..\` 未剥离、`tracked>0 ≠ FAIL` |
| 5 | 证据哈希不是从提交树取 | 工作区文件的 sha256 ≠ `git show <commit>:<path>` 的 sha256（工作区可能带未提交改动） | 4 次 |
| 6 | 行号漂移误红/漏红 | `--strict-lines` rc≠0；默认档 WARN 行数 > 0 且当日改过该文件 | `npc.rs` 当日 9~53 处漂移 |
| 7 | **自检调用点在 rc 判定之后**（新） | `grep -n "if fail_n" <tool>.py` 的行号 **小于** 新增 `_selftest_*()` 调用点行号；或「把新检查改成必失败 ⇒ rc 仍 0」 | inventory-fixture / rust-binary-evidence（§7.2 A） |
| 8 | **新校验自己判错因果**（新） | 新规则上线后**既有**门禁/夹具由绿变红（被检输入本身合法）；或把规则文案的因果链逐跳 `sed -n` 核 oracle，任一跳方向相反 | R6「越界方向必然超时」假红（§7.2 B） |
| 9 | **门禁工具就地改写 tracked 文件**（新） | `git status --porcelain` 出现 **`MM`**；或 `git diff --name-only` 非空而你并没有改那个文件；提交前 `git diff` 必须为空 | `selftest_mutations.py` 跑在主检出（§7.2 C） |

**一页速查（贴到下一批的开工清单）**：

```sh
# 7：新增自检在不在 rc 路径上（把行号比大小）
grep -n 'if fail_n' tools/conformance/card_window.py             # 判定行
grep -n '_selftest_[a-z_]*(tmp' tools/conformance/card_window.py # 调用行必须更小

# 9：工具有没有弄脏树（提交前必须干净）
git status --porcelain        # 见 MM / 意外 M ⇒ 停；git diff 必须为空
sha256sum tools/conformance/card_window.py
git show :tools/conformance/card_window.py > /tmp/idx.py && sha256sum /tmp/idx.py   # 两值必须一致

# 5：证据哈希是不是从提交树取的
git show <commit>:<path> > /tmp/x && sha256sum /tmp/x <path>
```

### 7.4 R 卡窗口欠账的当晚变化

**看板计数（截至 refs 同步 @ 21:47 +0800，master `1893470a0`）**：待认领 **2** / 进行中 **225** / 待审 **9** / 需要人 **0** / 受阻 **1** / 已完成 **205**。

**欠账总数**：清单口径仍是 `docs/window-batch-plan-20261023.md` §1 的 **173 张**（其 §1.2 的桥：merged-not-closed 217 / 最新 approve tier=R 155 / 无 tier 声明 18）。
今晚新增 **5 张 R 级卡**并入 master（GM 四卡 `f7a9deee1` + action-var-text `599bf0478`）且**都保持未 closed** ⇒ 按同一口径**外推**为 **≥178**；
但我**没有重算**这个总数（7176 条 review 里 252 条无 `decision` 字段、110 张的「最新 approve」无 tier ⇒ 本机重算口径不可靠，**精确值查不到**）。

**因今天的场景/工具卡拿到前置的**：

| 前置卡（merge） | review | 解锁了什么 |
|---|---|---|
| `712a948e3` window-rust-binary-evidence | `e04f9454ff` (reviewer-5, L) | 窗口批次里 **Rust 侧 sha256 / pid 对账**：不修则 173 张的 Rust 二进制证据全记错（`alt` profile 曾指向 .NET 程序集） |
| `57302d3df` sm-struck-31 | `38b40efce0` (reviewer-1, L) | `SM_STRUCK(31)` 的字段级掩码（`param` 掩、`recog` 走会话 ActorId），战斗类场景的比较器前置 |
| `dd0c2c6f6` probe-validation-gaps | `1078fc1098` (reviewer-3, L) | `--check-scenario` 的 R1–R6 形状/语义校验，场景卡上线前的机判前置（R2 的 producer 映射仍未做，残留 `R2-producer-map`） |
| `d5cbb7abf` upgrade_retrieve | `9d3f9b140e` (reviewer-6, L) | R2 升级/取回链的场景就绪（但见下「仍不可达」） |

**仍是 BLOCKED / 不可达的**：

- **`combat_strike` = BLOCKED-FIXTURE**（合并 `302038193`，`merge_result` `c09caecd` @ 21:09）：全库 **7338 角色 × 95409 件装备 × 5 个确定性目标无可行配对**，
  最好的一对 **joint 24%**；根因是伤害扣 `[AC, 2AC-1]` 随机护甲，必伤害要 `DC_min > 2*AC-1` 且 `HitPoint ≥ SPEED-1`。
  另有第二条：目标是**敌对守卫**，它打玩家也让玩家收 `SM_STRUCK(31)` 且 `recog=self`，而 `wait_for` 只按 ident 匹配 ⇒ **跑绿 ≠ 玩家命中**（四处封口写死「窗口批次不得消费本场景任何 PASS」）。
  正在补硬门：新卡 `openmir2-combat-strike-blocked-marker`（in-progress @ 21:52，给 JSON 加顶层 `blocked`，让 `card_window --dry-run` rc=2）。
- **`upgrade_retrieve` 脚本路可达、但 Rust 侧未接线**：唯一可用脚本路是 `UPGRADEITEMEX`（**34 行**；`TAKECHECKITEM`/`UPGRADEITEM`/`CHECKUPGRADEITEMS`/`RANDOMUPGRADEITEM` 全 0）。
  两个门：① **取回有 600000ms 时效门** —— `[Setup] UPgradeWeaponGetBackTime=600000`（部署 `Server.conf:331`；oracle 默认 `60*60*1000` = `GameSvrConf.cs:1421`，读键 `ServerConf.cs:230`，判点 `Merchant.cs:608`；**600000 是部署值不是源码字面量**）；
  ② Rust 侧 `@upgradenow`/`@getbackupgnow` **未执行**（`mir2-rs/bins/game-svr/src/lib.rs:5849-5855` / `:5890-5895` 只 `glog`「GotoLable 未执行（Market_Def 商人脚本未装载）」；`shop.rs` 成功支无 643、失败支零帧）⇒ **修好前不得拿本场景判 Rust 失败**。
- **`equip_struck_save`：原「零刷怪」前提已被证伪，且已在 21:50 返工出新 placement**（`reviewer-6` @ `46ddb64bce`，blocking）：
  旧 placement `map 0 (288,610)` 被 `MonGen.txt:112/113/124/125`（Range=50、Count=50，箱体覆盖玩家格）击穿，期望约 **12 只怪**进 AOI，且刷怪位置按墙钟不可复现。
  返工（status @ 21:50）改为 `map 0103 铁器店 (19,19)`、目标 `卫家店`，并逐条证明 0103 在 `MonGen.txt` **零行**、三份部署件 `cmp` 相同 ⇒ 视野内确定零怪；现为 **needs-review（待审），不再是「待重选」**。
- **35 张不可协议对拍**：口径同 `docs/window-batch-plan-20261023.md` §1，今晚未变（未重算）。

### 7.5 复核时发现的口径不一致（供下一批订正）

1. **`equip_struck_save` 的现状已过期**：派单说法是「待重选 placement」，实际 21:50 已出新 placement 并进 needs-review（见 §7.4）。
2. **Rust 行号漂移**：卡面与 review 文案写 `bins/game-svr/src/lib.rs:8836-8848`，实测在 `302038193` 与 `1893470a0` 上都是 `pose_dir` **8828**、`e.ai.dir = dir` **8840**、`front_target` **8846**
   ⇒ 正确引用是 **8828–8846**。同一旧行号还抄在代码注释里（`tools/probe/mir2_probe.py:856`），属清单第 6 类（行号漂移），建议引用时用**锚文本**（`let pose_dir = …`）而不是行号。
3. **「600000ms」不是 oracle 常量**：C# 默认是 `60*60*1000`（`GameSvrConf.cs:1421`），600000 来自部署 `Server.conf:331`。写窗口判据时不要写成「oracle 硬编码 10 分钟」。
4. **R 卡精确总数查不到**：理由见 §7.4（review schema 混杂）；**不要**把 173 或 178 当成实时真值。
5. **审查人身份**：`57495e25f` 的审查人是 `openmir2-worker-66`（worker 角色而非 `reviewer-*` 池），与作者 `openmir2-worker-70` 不同 principal ⇒ 满足「审查必须不同 principal」，但命名上不是 reviewer 身份，引用时照实写。
