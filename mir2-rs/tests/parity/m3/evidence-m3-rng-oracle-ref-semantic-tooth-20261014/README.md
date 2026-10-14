# evidence-m3-rng-oracle-ref-semantic-tooth-20261014

卡 `openmir2-m3-rng-trace-scene-groups`（M3 夹具卫生片）——**站点表 `oracle_ref` 方法体归属语义牙
+ reviewer-5 O-1 伪 range 收口 + 本卡场景锚门接入链深检**。

> 写集：`mir2-rs/tests/parity/m3/**`；**零 `.rs`**；零起栈 / 零端口；共享主检出未动；未 merge、未自签 review。

## 结论

1. **同族缺口补牙（Claim B 侧）**：`rng-trace-probe/consumer_groups.tsv` 的 `oracle_ref` 此前只做
   「存在 + 行号范围」，**缺 C# 方法体归属**（同仓 `drop-exp/drop_exp_purity_gate.py` 上一片已补）
   ⇒ 本片在 `rng_consumer_groups.py` 补齐：C# 行的 ref 必须落在其 `pattern` 点名的**方法体**内；
   正则族名（`GameItemSystem.UpgradeRandom\w+`）按**字面前缀** `UpgradeRandom\w*(` 匹配。
2. **reviewer-5 `f7318e59` O-1 收口**：`_method_body_ranges` 旧口径把任何 `name(`（含
   `if (_Attack(...)) { … }` / `if (Think())` 的**调用点块**）都当声明 ⇒ 产出**伪 range**
   （实测 `_Attack` 得 `(13,17)`、`Think` 得 `(255,259)`），给「ref 落错方法」留了假负通道。
   两处实现同时收紧为「**真声明**」（名字前必须是类型/修饰符，前一个词不得是
   `if/while/for/return/new/...`）。
3. **本卡场景锚门静默漂移实例（顺手收口，同一写集）**：`rng-trace-scene/check_anchors.py` 此前
   **零调用方**（只靠手工跑证据 README；`run_rng_trace_pair.ps1 -SelfCheckNoStack` 仅在系统 B READY
   时才走）⇒ 与 castle/guild/hero/selloff/quest/party 六个同族锚门相比，它是唯一「在场却坏无声」
   的一个。实测真身已漂移：`DRIVER-CONTRACT.md:59` 仍写 `mir2-rs/bins/game-svr/src/main.rs:5598`，
   而 `enable_production_monster_ai(&mut world)` 已随合并漂到 `:5665`，`check` / `selftest`
   **都 RED、无人发现**。收口：① 文档订正 `:5598`→`:5665`；② 把 `check_anchors.py` 的
   `selftest` + `check` 两向登记进 `fixture_chain_gate.SHARED_CHECKS`；③ 链门 selftest 加**防回退牙**
   （两向常在；与 master 的行会战登记门并集后 `.py` 登记项计数 `≥21`）。

## 判据 → 证据映射

| 判据 | 证据 |
| --- | --- |
| `rng_consumer_groups.py selftest` = **20/20** GREEN（原 15 + 方法体归属 5 条） | `02-selftest.txt` |
| `drop_exp_purity_gate.py selftest` = **32/32** GREEN（原 28 + O-1/正则族 4 条） | `02-selftest.txt` |
| 真表**改坏必红**（真身错锚 / 伪 range 调用点 / 正则族错行） | `03-mut-real-table-red.txt` |
| C# 行 `oracle_ref` 方法体归属逐行审计（两表 37 行，0 RED） | `04-oracle-ref-audit.txt` |
| 锚门漂移 RED→GREEN + 接入 fixture_chain（selftest 39/39 / check GREEN） | `05-chain-registration.txt` |
| 四门禁 | `06`–`09` |
| 两侧驱动无栈自检（37/37、20/20） | `11`、`12` |
| 边界 / 冻结面 / 零 `.rs` | `10-boundary.txt` |

## 改坏必红原始输出（摘，见 `03`）

    改坏 (MUTATED) Think :208 -> :255 (调用点 if (Think()) 的块；O-1)
      RED = ['行3(monster_ai/csharp): oracle_ref 行 255 不在 `Think` 方法体内
             （range=[(187, 218)]；ref=src/M2Server/Monster/MonsterObject.cs:255）']
    改坏 (MUTATED) _Attack :41 -> :14 (调用点 if (_Attack(...)) 的块；O-1)
      RED = ['行36(csharp/measured): oracle_ref 行 14 不在 `_Attack` 方法体内
             （range=[(30, 66)]；ref=src/M2Server/Actor/BaseObject.Attack.cs:14）']

> 收紧前这两条都是**绿**（伪 range `(255,259)` / `(13,17)` 放行）——这正是 O-1 的假负通道。

## 如实登记

- Rust 侧 `oracle_ref` 仍是**文件级/模块入口**锚（无行号或目录），语义牙按口径跳过（不可判，不误伤）。
  收紧到行级锚属 reviewer-5 `108fc72b` O-1 明示的**窗口期/④ 侧**工作，本片不动。
- 方法体识别**保守**：多行签名（名字单独成行）不产生 range ⇒ 若未来真表出现此类声明会 fail-closed
  点名（当前两表全部单行，0 误伤，见 `04`）。
- 锚点是 `main.rs` 的**首个**匹配行；master 继续加长宿主时，`check_anchors` 会在链门里**恒 RED**
  （这正是接入共享管线要拿到的「不再静默」）。
- 卡级 R 真机 DoD（同场景含怪两相 trace + `groupdiff --fold --strict` site/order diff=0）仍待承接卡
  `openmir2-m3-rng-scene-driver` + owner 独占窗口。
- EOL/清单口径：按**仓库字节（git blob / LF 归一）**记；`verify_sha256sums.py verify --check-extra` /
  `eolcheck` 读数见 §复核。

## 复核（本片实跑；全程作者 worktree；零起栈/零端口）

    $ python -I tools/evidence-hash/verify_sha256sums.py verify --check-extra <EV>
    $ python -I tools/evidence-hash/verify_sha256sums.py eolcheck <EV>
