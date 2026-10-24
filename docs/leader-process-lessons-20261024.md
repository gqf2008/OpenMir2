# 2026-10-24 协作/操作事故复盘（只写「下次怎么做」）

- 卡：`openmir2-leader-process-lessons-20261024`（tier=L）；写集 `docs/`。
- 作用：把今天踩到的六类事故从「会话记忆 / 卡片 comment」收进仓库。**不追责**，只留**证据**与**1 分钟自检**；
  涉及主体一律按角色/会话名写（worker / reviewer / orchestrator / 道具名），不写个人。
- 与既有规则互链：`docs/leader-queue-20261023.md` 规则 14–20（派活 / 门禁 / 审计链 / 变异 / 身份 / 接管）——其中**规则 20「长命 agent 会耗尽上下文——每张卡起新 agent」与下文 §1 同源**；本文件只补今天**新增**的坑，
  规则原文不重复。
- 自检命令都在 2026-10-24 当天**实跑过**（见各条「自检（已跑）」）；§1 的空闲自检片段另在 **Windows PowerShell 5.1 与 pwsh 7** 两个壳下各跑同一组正/负控，结果逐字一致（v5）。

---

## 1. 重复派单 + 同 worktree 双实例

- **现象**：同一张卡被派给同一 principal 的两个实例；一个在跑全量变异，另一个切分支/`rebase`，还有实例 `taskkill` 了正在跑的 `selftest_mutations`。
- **证据**：① `card-window-var-passthrough` 线程的 claim/status 交错——worker-73 的 comment `7f01290e` 明写「同一 worktree `E:/tmp/wt-worker-73`、同一 principal（worker-73）两实例并发」；`selftest_mutations` 进程被杀后 `card_window.py` 停在 MG7D 变异态（见第 2 条）。
- **证据（第二例，本卡自身）**：本卡认领（worker-24 `status`，`ts=1792796265`）后 **268 秒**又被派给 worker-58；worker-58 读完线程看到 `needs-review`（`91829afc1`）与已有 claim，按 §0b 未建分支、未提交竞争 patch，只留 comment `58e40ee9`。⇒ 两次都是「派单方没看 claim」，不是执行方抢卡。
- **根因**：派卡只看 `owner` 字段、没看**该 worktree 里是否已有进程在跑**；`card_claim` 的 `ttl` 不阻止第二个实例再认领。
- **代价**：变异白跑、分支被误切、一次「残稿」需要人工 sha 对齐才敢继续。
- **规则**：**派卡/认领前先证明「该 worktree 空闲」**——① 进程（候选集**只按本 worktree 限定**，见下；本 worktree 命中 ⇒ `BUSY`，只命中他车道 ⇒ 剔除，哪个 worktree 都归不到 ⇒ `INDETERMINATE`，**既不判忙也不判闲**）、② git 现场（`status` 干净、无 `index.lock`）、③ 规则 19 第 4 查——三步都必须是「空闲」才发签名 `status`。
- **自检（已跑，v5）**：判据是一个**明确的 verdict + 退出码**，不是「人眼看列表」：
  `IDLE=0` / `BUSY=2` / `INDETERMINATE=3` / `NOT-APPLICABLE=4`。**只有 `IDLE=0` 允许写「空闲」**；`2/3/4` 一律不得当作空闲。第 1–3 步没有全部 `IDLE/idle` 就**不许写「空闲」**。
  ```powershell
  function Test-WorktreeIdle {
    param([Parameter(Mandatory)][string]$Path)
    $norm = ($Path.ToLowerInvariant()).Replace('\','/').TrimEnd('/')
    # 0) 只判「本仓登记在册的 worktree」：路径不存在 / 未登记 => NOT-APPLICABLE(rc=4)，不扫全机
    $wts = @()
    if (Test-Path -LiteralPath $Path) {
      $wts = @(git -C $Path worktree list --porcelain 2>$null |
        Select-String '^worktree ' |
        ForEach-Object { ($_.Line -replace '^worktree ','').ToLowerInvariant().Replace('\','/').TrimEnd('/') })
    }
    if (-not (Test-Path -LiteralPath $Path) -or $wts -notcontains $norm) {
      return [pscustomobject]@{ Verdict='NOT-APPLICABLE'; Rc=4; Own=@(); Unattributed=@(); OtherLane=@() }
    }
    $all = @(Get-CimInstance Win32_Process)
    $byPid = @{}; foreach ($p in $all) { $byPid[$p.ProcessId] = $p }
    $kids  = @{}; foreach ($p in $all) { if ($p.ParentProcessId) { $kids[$p.ParentProcessId] = @($kids[$p.ParentProcessId]) + $p } }
    function _Hit($p, $n, $lf) {
      $t = ((@($p.CommandLine) + @($p.ExecutablePath)) -join ' ').ToLowerInvariant().Replace('\','/')
      if ($t.Contains($n)) { return $true }
      return ($t -match "(^|[^a-z0-9_-])$([regex]::Escape($lf))([^a-z0-9_-]|$)")
    }
    function _Chain($p) { $out=@($p); $cur=$p; for($i=0;$i -lt 8 -and $cur.ParentProcessId;$i++){ $cur=$byPid[$cur.ParentProcessId]; if(-not $cur){break}; $out+=$cur }; $out }
    function _Desc($p) { $out=@(); $q=@($p); for($d=0;$d -lt 8 -and $q.Count;$d++){ $nq=@(); foreach($x in $q){ foreach($y in @($kids[$x.ProcessId])){ if($y){$out+=$y; $nq+=$y} } }; $q=$nq }; $out }
    function _WtHit($p, $w) { $lf = Split-Path $w -Leaf; return ((@(@(_Chain $p) + @(_Desc $p)) | Where-Object { _Hit $_ $w $lf } | Measure-Object).Count -gt 0) }
    # 1) 候选 = 构建/测试类；归属只看「本 worktree 路径 / 叶子名」是否出现在 自身·8 层祖先·8 层子孙
    $cand = $all | Where-Object { $_.Name -match '^(python|python3|py|cargo|rustc|card_window|mir2_|node)' }
    $own=@(); $other=@(); $unknown=@()
    foreach ($p in $cand) {
      if (_WtHit $p $norm) { $own += $p; continue }
      $hit = $null
      foreach ($w in $wts) { if ($w -ne $norm -and (_WtHit $p $w)) { $hit = $w; break } }
      if ($hit) { $other += [pscustomobject]@{PID=$p.ProcessId;Name=$p.Name;Worktree=$hit} }
      else      { $unknown += $p }
    }
    # 2) 收尾：本 worktree 命中 => BUSY(2)；都不归属 => INDETERMINATE(3)；清 => IDLE(0)
    $v  = if ($own.Count) { 'BUSY' } elseif ($unknown.Count) { 'INDETERMINATE' } else { 'IDLE' }
    $rc = @{ BUSY=2; INDETERMINATE=3; IDLE=0 }[$v]
    return [pscustomobject]@{ Verdict=$v; Rc=$rc; Own=$own; Unattributed=$unknown; OtherLane=$other }
  }
  ```
  ```powershell
  $wt = 'E:/tmp/wt-worker-73'                            # 换成 git worktree list 里本次那条
  $v = Test-WorktreeIdle -Path $wt
  "verdict=$($v.Verdict) rc=$($v.Rc)"
  $v.Own; $v.Unattributed; $v.OtherLane                    # 明细：本车道 / 归不到 / 他车道
  # 脚本化判定（真正的退出码）：powershell -NoProfile -Command ". ./idle.ps1; exit (Test-WorktreeIdle -Path <wt>).Rc"
  # 本命令（含上面整个函数）在 **Windows PowerShell 5.1 与 pwsh 7 下均已验证**：同一组正/负控输出逐字一致。
  ```
  - `own` = 路径/叶子名出现在**自身命令行、8 层祖先、或 8 层子孙**里 ⇒ **`BUSY(2)`**：按 PID 问一句再动。查祖先/子孙是必需的——`cargo test` 自身命令行常不带路径（靠子进程 `rustc --out-dir <wt>/target/...` 归位），`cmd /c cd /d <wt> && python` 则靠祖先归位。
  - 只命中**别的** worktree ⇒ 他车道，**从本 worktree 的判定里剔除**，不进 `unknown`；多车道机器上这是常态。
  - **哪个 worktree 都归不到** ⇒ `unattributed` ⇒ **`INDETERMINATE(3)`**。这是 `unknown` 唯一的收尾口径：`Win32_Process`/CIM **不暴露 cwd**，所以「cwd 在本 worktree、命令行与祖先/子孙都不含路径」的进程（如 `Start-Process -WorkingDirectory <wt>` 起的 python）无法归因——按 PID 问一句 / 查父链归到某条已知车道，确认与本卡无关后**重跑**；**不得把 unknown 直接当忙，也不得忽略**。
  - `$Path` 不存在或未登记在 `git worktree list` ⇒ **`NOT-APPLICABLE(4)`**：没有「本车道」可判，不扫全机、不产生 unknown（**负控**）。**4 = 参数/登记错误**：拼错一个字母也会落到这里，必须修路径后重跑，**不得当作通过**（否则就是新的假绿）。
  - `own=0` 且 `unattributed=0` ⇒ 继续第 2、3 步；三步全是空闲/通过才判 `IDLE`。
  ```powershell
  # 第 2 步：git 现场（有未提交改动或 index.lock ⇒ 有人在写）
  git -C $wt status --porcelain
  git -C $wt log -1 --format=%ci
  Test-Path (Join-Path (git -C $wt rev-parse --absolute-git-dir) 'index.lock')   # 必须 False
  # 第 3 步：规则 19 第 4 查——该卡线程 10 分钟内有没有同 principal 的写入
  walgit collab thread <卡> --repo . | python -I -c "import json,sys,time;t=json.load(sys.stdin);print([(e['principal'],e['entry']['ts']) for e in t if time.time()-e['entry']['ts']<600])"
  ```
  **v3 旧写法错在哪（实测，照抄会得出错结论）**：
  (a) 候选集只按进程名过滤、**不按 worktree 归属** ⇒ 机器上任意他车道的 python/cargo/rustc 都变成 `unknown`，而规则又说 `unknown ⇒ 不能判空闲` ⇒ 在多车道机器上第 1 步**永远过不了**（§3 自己就把「已有其它车道在跑」当常态）；
  (b) **负控失败**：`$wt='E:/tmp/wt-definitely-idle-nonexistent'`（路径根本不存在）也照样输出他车道的 `unknown` 行（v4 实测 3 行：cronclaim 的 `hidden2.py`、reviewer-5 的探针、spg-base 的 python）⇒「不存在的 worktree」被判成「不能判空闲」；
  (c) 只输出 `Why` 列，**没有 verdict / 退出码**——「unknown ⇒ 忙」只是散文，不是可执行判据。
  **v5 双壳对照（2026-10-24 实跑，`E:/tmp/wt-worker-73`，同一时刻同一组探针）**：正控（`python <probe>.py E:/tmp/wt-worker-73`，命令行只含该路径一次）⇒ `BUSY rc=2 own=1`；杀掉 ⇒ `IDLE rc=0`；`Start-Process -WorkingDirectory <wt>`（命令行无路径、CIM 不暴露 cwd）⇒ `INDETERMINATE rc=3 unattributed=1`；不存在的路径 ⇒ `NOT-APPLICABLE rc=4`。**以上四组在 Windows PowerShell 5.1 与 pwsh 7 下输出逐字相同。**
  **为什么必须接 `Measure-Object`（v4 旧写法失败实跑）**：同一探针下，旧写法 `(... | Where-Object {...}).Count -gt 0` 在 `powershell` 5.1 得 `INDETERMINATE rc=3 own=0 unattributed=1`（管道里只有一个 CimInstance 时 `.Count` 是 `$null`），`pwsh` 7 却得 `BUSY rc=2 own=1`。v4 的「正控能过」是碰巧链上命中 2 个对象；v5 改成 `| Measure-Object).Count` 后两个壳复核一致。
  第一例「双实例」真的起过并发进程（worker-73，路径 `E:/tmp/wt-worker-73`，comment `7f01290e`）；**§1 第二例不是路径问题**——那是派单方没看 claim，worker-58 在认领前就拒了（comment `58e40ee9` 原文：未认领 / 未建分支 / 未提交 competing patch），只作「派单前没读线程」的样本。

## 2. 变异进程未退 + 残稿

- **现象**：上一轮的 `selftest_mutations` 没有正常退出，继续写 `card_window.py`；未提交改动里混进了变异态（例如 `magic_decl = profile_magic(prof)`）。现场是卡 `openmir2-card-window-var-passthrough`，证据见 worker-73 comment `7f01290e`——**引用与正文必须指同一颗牙**：该 comment 原文记作 **P5**（`selftest_mutations.py:271`「预置证据没进 summary」），而上面这个 `magic_decl` 片段实属 **MG7D**（`selftest_mutations.py:306-311`，丢掉按侧覆盖）；此处以**代码 / MG7D** 为准，P5 是 comment 当时的记法。
- **证据**：`git status --short` 出现 `M tools/conformance/card_window.py`；`git hash-object` 与 `git rev-parse HEAD:<path>` 不等。
- **根因**：变异 harness 与日常编辑共用同一工作树，且**没有「跑完即核」的硬检查**；进程被外部杀死时不会自我还原。
- **代价**：若直接提交，会把「变异版」当交付；若直接 `git checkout` 又会丢自己的改动——两难。
- **规则**：**任何变异/自测跑完立刻核三件事**：`git status --short` 为空、被变异文件的 blob == `HEAD` blob、无残留 `selftest_mutations` 进程。
- **自检（已跑）**：
  ```sh
  git status --short
  git hash-object tools/conformance/card_window.py
  git rev-parse HEAD:tools/conformance/card_window.py      # 两行必须相同
  ps -W | grep selftest_mutations | wc -l                   # 计数（不是判定）：必须为 0；grep -c 在 0 时 rc=1，别挂 rc 判定
  ```

## 3. 宿主内存压力杀后台任务

- **现象**：`inventory-fixture-reachability` 首交时全量变异跑到 **39/48 牙**被系统终止（**不是牙失败**）；并行跑多个变异全量会触发。
- **证据**：`E:/tmp/inv-mut3.out` 39 行 `OK`、0 行 `FAIL`；终止通知写明「system is running low on memory」；之后不敢自行重启。（该文件在**仓外**，可能被清理；需要长期证据时把摘要抄进报告。）
- **根因**：每颗牙都会起一整套 `card_window --selftest`（几十个子进程），并行/堆积时内存峰值远超单跑。
- **代价**：验证半途而废；交付里只能写「39/48 + 残留」，复审还得补跑。
- **规则**：**变异只串行、逐牙跑，并在开跑前看空闲内存**；被打断时**如实记录进度**，不要「重跑掩盖」。
- **自检（已跑）**：
  ```powershell
  (Get-CimInstance Win32_OperatingSystem).FreePhysicalMemory / 1MB   # 本次 9.93 GB
  (Get-Process python,cargo -ErrorAction SilentlyContinue | Measure-Object).Count  # 本次 4（他车道）
  ```
  低于 ~4 GB 或已有其它车道在跑变异 ⇒ 只跑 `--only <ID>` 单牙。

## 4. 认领即消失（dead claim）

- **现象**：某会话的 `card_claim` 三次占卡、零交付（`window-batch-rehearsal`、`rehearsal-combat-filter-stale`、`needs-human-count-sync`）。
- **证据**：三个线程里 `card_claim`（`actor=openmir2-worker-23`，`ttl=1800`）之后**没有该 principal 的任何条目**（无它自己的 `status`/`patch`/`comment`）；卡最终都由**别的 principal** 交付：`window-batch-rehearsal` → worker-67（`status` `ts=1792785083`）、`rehearsal-combat-filter-stale` → worker-67、`needs-human-count-sync` → worker-58。
- **证据（反面，必须写进来）**：远端**确实有**这些卡的分支——逐张用**分支全名**查：`git ls-remote walgit refs/heads/card/window-batch-rehearsal refs/heads/card/rehearsal-combat-filter-stale refs/heads/card/needs-human-count-sync`（注意 `needs-human-count-sync` 的分支名**不含** `rehearsal`，所以 `'refs/heads/*rehearsal*'` 这个 glob 根本覆盖不到它）；但这些线程里**没有 claim principal 签名的 patch/status**（分支 tip 的 `%an` 是 worker-67/69，那是共享 git 配置的噪声，**不能当身份**——见规则 16）。而且两次 claim 在别人接管时**还没过期**：`window-batch-rehearsal` 二次 claim `1792784597`+1800 > 接管 `1792785083`；`needs-human-count-sync` claim `1792795397`+1800 > worker-58 首个条目 `1792795699`。⇒ 这三次接管靠的是 **Leader 转派/裁定**，不是「等 ttl 过期后自取」。
- **证据（为什么分支匹配不能当「有无交付」的必要条件，v4 实测 / v5 复算）**：扫本地 `refs/collab/inbox` 快照内的 8175 条 entry（含 1189 条 `patch`）：**18 条 patch 条目（17 个不同 head、14 张卡）**的 head 不在**同名活分支** tip 的祖先链上（两向 `git merge-base --is-ancestor` 均 rc=1）；**3 条 head 对象已被裁**（`git cat-file -e` 失败：卡 `openmir2-monster-showhp-53-broadcast` 的 `ca7cb5acc…` 与 `6485c204…`，以及卡 `openmir2-window-criteria-quality-audit` 的 37 位 `ed7453ac5c70c5cbc8cc86c49a0f6e216ed57`——它同时落进下面的「短于 40 位」桶）；**658 条 head 短于 40 位**：其中 **302 条是分支名**（如 `worktree-bevy-dropbench`）、**356 条才是缩写 oid**——分支名要走 `git rev-parse <ref>`，不能当缩写解析。
- **统计口径（必须和数字一起写，v5 补记）**：快照 = 截止 **`ts<=1792801382`** 的 **8175 条 entry / 1189 条 patch**；「活分支」= **`walgit` 远端仍存在的 `refs/heads/card/*`**（逐张 `git ls-remote walgit refs/heads/card/<卡>`）；head 先 `git rev-parse --verify '<head>^{commit}'` 解析成完整 oid，再双向 `merge-base --is-ancestor`。**放宽为「本地任意已知分支」会变成 21 / 20 / 17**——多出的 3 张是分支只在本地、`walgit` 上没有的卡：`openmir2-r-evidence-registry-sweep`、`openmir2-conformance-rule-sm-struck-31`、`openmir2-scenario-combat-strike`（三者 `ls-remote` 命中均为 0）。**别只写一个数不写口径**：账本与分支 tip 一直在动，口径一换数字立刻变。
- **证据（not-provable 样例）**：`openmir2-script-envir-root-host`——worker-66 签 patch `0632c6b103`（ts=1792752205），worker-24 的 status（ts=1792753161）原文写「从良好提交 `2c8281618` 重建 + cherry-pick docs `0632c6b10` … force-with-lease 重推」，该卡分支 tip = `4d83c728d3`；`git merge-base --is-ancestor 0632c6b103 4d83c728d3` ⇒ rc=1，反向 ⇒ rc=1，对象存在。⇒ 若把「分支 tip 必须对上 patch.head」当必要条件，**真的签过 patch 的 worker-66 会被判「无签名交付」**——这是假 dead claim 的新入口。
- **根因**：`card_claim` 是**软锁**（只有 ttl），不会因为「人没了」自动失效；board 的 `owner` 也不一定跟着变。
- **代价**：卡在「待认领/进行中」之间空转，别的车道要么等、要么撞车。
- **规则**：**四件证据的语义是「claim 失效」，不是「卡无人做」**，且 ②③ 必须**限定到 claim principal**：① 该 claim 的 `ts+ttl` 已过（或卡已被 Leader 转派/关闭）；② **该 claim principal 自己**没有签名交付——**主判据（唯一必要条件）只取协作账本**：`patch`/`status` 条目的 `actor` 是否出现该 principal。**分支匹配只是佐证，不是必要条件**：能对上 ⇒ 证据增强；对不上 ⇒ **git 不提供结论（not-provable）**，既不能据此判「无交付」，也不能据此判「有交付」，交回账本 + Leader 裁定（常见原因：`force-with-lease` 重推 / `rebase` / 从良好提交**重建**、合并后分支已删、head 对象已被裁）。比较前必须先把账本里的 head 解析成完整 oid：`refs.head` 可能是缩写，也可能根本没有 `refs` 而把 head 放在 `body.head`（如上述 worker-66 的 patch）。**不要用 `git log --format=%an` 认交付者**：本仓五个 worktree 共用同一份 `.git/config` 的 `user.name`（实测主检出 / cronclaim / wt-worker-73 / wt-worker-67 / wt-reviewer-2 全 = `openmir2-worker-67`），实测 `card/needs-human-count-sync` 的两次 patch 由 worker-58 签名、分支 tip 的 `%an` 却是 worker-67——拿 author 判会把**活 claim 判成 dead**（与规则 16 直接冲突）；③ **该 claim principal 自己**没有后续 `status`/`patch`/`comment`——别人接管后的交付**不算**它的证据；④ 卡上写明接管声明。四件缺一（尤其「ttl 未过期」「已有他人交付」）⇒ 只发 comment 请 Leader 裁定，不自行接管；已被他人交付的卡**不重复接管**。
- **自检（已跑，v4）**：
  ```sh
  # ① claim 是否过期
  walgit collab thread <卡> --repo . | python -I -c "import json,sys,time;t=json.load(sys.stdin);\
  [print(e['entry']['actor'],e['entry']['ts'],e['entry']['body'].get('ttl'),'expired=',time.time()-e['entry']['ts']>e['entry']['body'].get('ttl',0)) for e in t if e['entry']['kind']=='card_claim']"
  # ②③ 只看签名条目：该 claim principal 自己有没有 patch/status/comment；head 先取 entry.refs.head，没有则取 entry.body.head
  walgit collab thread <卡> --repo . | python -I -c "import json,sys;t=json.load(sys.stdin);print([(e['entry']['kind'],e['entry'].get('actor') or e['principal'],(e['entry'].get('refs') or {}).get('head') or e['entry'].get('body',{}).get('head')) for e in t if (e['entry'].get('actor') or e['principal'])=='<claim principal>'])"
  # 分支是否存在：逐张卡用**分支全名**查（卡名不含 rehearsal 的不要用 *rehearsal* glob）
  git ls-remote walgit refs/heads/card/window-batch-rehearsal refs/heads/card/rehearsal-combat-filter-stale refs/heads/card/needs-human-count-sync
  ```
  **佐证（不是必要条件）——分支存在时这样查祖先，并允许「not-provable」**：
  ```sh
  git rev-parse --verify '<patch.head>^{commit}'              # 账本里常是缩写：先解析成完整 oid；失败 ⇒ not-provable
  git fetch walgit <branch>                                  # 本地没有该分支才需要
  git merge-base --is-ancestor <patch.head.oid> FETCH_HEAD   # rc=0 ⇒ 佐证成立（head 在分支上）
  git merge-base --is-ancestor FETCH_HEAD <patch.head.oid>   # rc=0 ⇒ 佐证成立（分支只是落后）
  git cat-file -e <patch.head.oid>                           # 失败 ⇒ head 已被裁 ⇒ not-provable
  # 两向都 rc=1 ⇒ not-provable：不得据此判「无签名交付」，回账本与 Leader 裁定
  ```
  本次实测 `window-batch-rehearsal`：两次 claim `ts=1792782616/1792784597`、`ttl=1800`、`now=1792796276` ⇒ 均已过期。复核 ② 时先打印该 principal 自己的条目与 head（上面的 one-liner），再**当且仅当**分支存在时把它当佐证；**`git log -1 --format=%an` 只能当反例**——它在本仓是共享配置噪声，不是 principal。

## 5. 解合并冲突时「顺手去重」误删表外行

- **现象**：解 `window-batch-rehearsal` 三行冲突时用了跨全文件的去重脚本，把表外同名列（`dice_chain` 整行）删掉。
- **证据**：冲突 resolving 后 `git diff <master> -- docs/window-batch-rehearsal-20261024.md` 出现**冲突块之外**的删除行（`dice_chain` 整行；master `47fd96ca8` 该表 `:78`）；随后 `git merge --abort` 回退。
- **根因**：把「冲突解决」当成「全局清洗」；没有把改动范围限制在 `--diff-filter=U` 的文件与冲突块内。
- **代价**：一次合并回退 + 重做；若未发现会静默丢一行场景。
- **规则**：**解冲突只改冲突块**（`<<<<<<<` 到 `>>>>>>>` 之间）；禁止在解冲突时跑全文件重写/去重。
- **自检（已跑）**：
  ```sh
  git diff --name-only --diff-filter=U        # 只有这些文件允许被改
  git diff <master> -- <涉及文件> | grep -c '^-[^-]'   # 删除行数应只来自冲突块
  ```
  （本次实跑冲突文件数：`0`。）

## 6. 把复审请求发给作者本人（自签风险）

- **现象**：两次把复审请求发给 patch 作者本人；作者按 §0b/§4 拒绝自签。
- **证据**：review 条目的 `actor` 与 patch 的 `actor` 相同；或 `review` 请求对象等于作者 principal。
- **根因**：派复审时只看「谁在线」，没比对 `patch.actor`。
- **代价**：一轮往返白费；若作者没拒绝，会污染「独立审查」账本。
- **规则**：**派复审前比对 `patch.actor` 与候选 reviewer**，必须不同 principal；合并计票只认**非作者**的 approve。
- **自检（已跑）**：
  ```sh
  walgit collab thread <卡> --repo . | python -I -c "import json,sys;t=json.load(sys.stdin);\
  print('patch',{e['entry']['actor'] for e in t if e['entry']['kind']=='patch'},\
  'review',{e['entry']['actor'] for e in t if e['entry']['kind']=='review'})"
  ```
  两个集合**不得相交**。示例值**会随时间变化**，不要当固定事实：2026-10-24 复核 `openmir2-gate-baseline-snapshot` 时是 patch={`openmir2-worker-24`}、review={`openmir2-reviewer-6`}（该 review `ts=1792797722`，晚于早先记下的「review=∅」）——判据只看两个集合**有没有交集**，不看具体成员。

---

## 附：本文件新增 / 未新增的规则

- 本文件第 1–6 条都是**今天新增**的操作规则（规则 14–20 已覆盖的部分只互链，不重复；§1 另与规则 20 同源）。
- 第 2 条与规则 **17** 有关（mutation runner 就地改写 tracked 文件、中断先 `git checkout --` 还原）；第 3 条的「**变异只串行、逐牙跑**」是**本文件新立的规则**——既有 14–20 未覆盖（`grep -c '串行' docs/leader-queue-20261023.md` = 0；规则 19 是「第 4 查 / 接管声明」，与变异无关），此处不是补命令，是立新规则。
- 门禁基准一律用**主源 `walgit/master`**，不用 `origin/master`（GitHub 镜像）：同一 head 换 base 会得到不同 merge-tree tree（v4 head `2c8f0b611`：base `walgit/master=53ce75bf0` ⇒ `d9145a0ab35676513cc9f272a62f8cdec62f3a97` rc=0；base `origin/master=0e7da10b8` ⇒ `9eab02cf5…`）。证据里必须写 base sha。
- 未写入的同类观察：`board --json` 不列出的卡（`其它需人工` 桶）会让「认领前重读」失效——已由 `docs/goal-gate-…` 与 `board-index-reconcile` 卡承接，本文件不扩。
