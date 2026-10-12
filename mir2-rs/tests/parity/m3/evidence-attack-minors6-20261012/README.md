# M3②③ 夹具：清三条**非阻塞 minor**（事件键按 k 分段 / 历史会话口径 / 残差①措辞）· 证据

- 卡：`openmir2-m3-attack-pair-fixture`（issue `76cc08b8`）／车道 `m3bag`／principal `openmir2-worker-4`
- worktree：`.claude/worktrees/m3bag`／branch：`worktree-m3-bag-equip`
- base：`refs/heads/master` = `6eec1e2e`（本卡上一片「账本两侧化」已合入）
- 任务来源：卡上 remaining **③**「reviewer 先前登记的三条非阻塞 minor（旧会话不可 align、事件键依赖 hp 单调、措辞）随下一片」。
  brief 顶部最新块仍是 **owner 2026-10-12 17:35**（残差③ `my` + 残差① 账本），该片已交付并合入；
  其第 2 条（残差① 两侧账本定位/修法）**仍挂在 m3cs 的真机活栈两侧账本 diff 上**（owner 18:25 派），
  本片**不碰**它、不替它下结论。

## 0. 本片交了什么（一句话，零 `.rs`）

只清三条 reviewer 登记的非阻塞 minor（都在本卡写集 `mir2-rs/tests/parity/m3/**` + `tests/parity/whitelist.md`）：

1. **事件键依赖 hp 单调**（reviewer-5 `6cc3d255` minor）⇒ `attack_diff.py::align_events` 的去重键由
   全局 `(src,actor,dmg,hp,maxhp)` 改为 **按 k 分段** `(current_k,src,actor,dmg,hp,maxhp)`：
   同一 k 内的多视角/重复投递**仍**精确去重；不同 k 的同一击（HP 被回满后才可能出现）**不再被误折叠**。
2. **旧会话不可 align**（同条 minor 的另一面）⇒ `my` 缺失仍 **fail-closed（rc=2）**，**不新增任何
   「缺字段也继续」的旁路**；把错误信息与 docstring 指向既有 `rebuild_attack_my.py` 离线归一化，
   并在证据里**实跑**「rebuild → align」这条历史会话路径证明可用。
3. **措辞**（owner 2026-10-12 18:25 口径纠正）⇒ `whitelist.md` T-29 与 `m3/README.md` 里残差① 的根因
   由「非 Rust 生产路径多抽」改为「**主因 = 非攻击 RNG 消费者/起点未隔离；Rust 生产路径尚未排除**
   （真机活栈两侧账本 diff 待 m3cs）」。

默认行为不变：不带 `--side`/无 `my` 的旧命令形态、`align` 的 fail-closed 语义均**逐字节/逐退出码**保持。

## 1. 读数（原始件 → 文件）

| 文件 | 内容 |
|---|---|
| `01-attackdiff-selftest.txt` | `attack_diff.py selftest` ⇒ **GREEN 30/30**（旧 28 条 + 新增 `p-perk-key-no-cross-k-fold` / `p-same-k-still-deduped`） |
| `02-mutation-global-key-red.txt` | **改坏必红**：把键改回全局 `(src,actor,dmg,hp,maxhp)` ⇒ selftest **RED 28/30**，两条 p-用例必红（`hits=1` 而非 2） |
| `03-missing-my-failclosed-hint.txt` | 历史 attack 行缺 `my` ⇒ `align` **rc=2**，错误信息含「请先用 rebuild_attack_my.py 离线复算归一化」（fail-closed 不变） |
| `04-legacy-path-rebuild-then-align.txt` | 历史会话路径**实跑**：`rebuild_attack_my.py`（从 `机器人:[id]` 还原 `my`）⇒ `align` ⇒ `ALIGN_OK attacked=1 hits=1 sum=5 (other_observer=1 deduped=1)` |
| `05-wording-diff.patch` | 措辞订正的 diff（`whitelist.md` T-29 + `m3/README.md`） |
| `07..10-gates-*.txt` | 四门禁（隔离 `CARGO_TARGET_DIR=E:/tmp/m3bag-minors6-target`）：fmt 0 / clippy 0 / **81 suites 668 passed 0 failed 16 ignored** / msgcodegen 604 |
| `11-selfcheck-nostack.txt` | `run_attack_pair.ps1 -SelfCheckNoStack`（pwsh 7）rc=0，全绿（含 ⑲ attack 行缺 `my` ⇒ rc=2） |
| `06-selfcontained.txt` | 证据自包含复核：`git check-ignore` 空 + `git ls-tree` 计数 == 盘上文件数 |

## 2. 判据与牙齿（都实跑）

1. **按 k 分段去重（牙齿）**：构造 k=1/k=2 的 `(src,actor,dmg,hp,maxhp)` 完全相同（模拟 HP 回满），
   要求 `hits==2`；改回全局键 ⇒ `p-perk-key-no-cross-k-fold` 与 `p-same-k-still-deduped` **同时红**（`02`）。
2. **fail-closed 不降级**：缺 `my` ⇒ rc=2（`03`），且错误信息给出可执行的历史会话路径（`04` 证明该路径 GREEN）。
3. **措辞**：T-29 / README 不再写「非 Rust 生产路径多抽」这种「已排除」口径（`05`），改为
   owner 18:25 的「主因 + 尚未排除」口径。
4. **判据口径不动**：DoD② 仍 = 活栈 `attack_diff.py diff` 逐 k=0；本片只清 minor，**不改**该口径、
   **不加**白名单豁免（T-29 仍标「待独立 principal 复核」）。

## 3. 边界

- 改动面：`mir2-rs/tests/parity/m3/attack_diff.py`、`mir2-rs/tests/parity/m3/README.md`、
  `mir2-rs/tests/parity/whitelist.md`、本证据目录。**零 `.rs`**、零 `src/**`、零 `sql/`、零 `MirClient`。
- **未起栈、未占端口**（离线可判）；**未 merge、未自签 review**（作者 = `openmir2-worker-4`）。
