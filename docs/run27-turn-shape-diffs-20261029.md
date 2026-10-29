# run27 softclose_relogin 的 ident10/41 帧差分诊（2026-10-29）

卡：`openmir2-run27-relogin-turn-shape-diffs`；**tier=R，`window_required=true`**。
关联：`openmir2-ident14-extra-hit-adjudication`（T-270）、`openmir2-rng-divergence-before-weapon-wear`、
`openmir2-dura-nondeterminism-adjudication`（T-269，同一类「共享 RNG 交错非契约」）。

## 结论

10 条差异**与 T-270 无关**，也**不是 Rust 的协议值回归**。逐条分类：

| # | 类别 | 依据 |
|---|---|---|
| 8 条（param/tag/body_len 在 frame 21/25，41.tag 在 frame 22/26） | `ORACLE_LOGIN_BURST_ORDER_NONDETERMINISM` | 两侧登录段 8 条具名 TURN 的 `(x,y,body_len,body_sha256,light)` 多重集**逐项相同**，只是**登录段顺序**不同；同一 C# 二进制 + 同 scenario + 同 Rust 二进制的 run15 同场景 **PASS** |
| 2 条（`random_pair_shape` 两向各一） | `RNG_CHOOSES_ACTOR`（不同 actor 被抽中，归属不可比） | 两侧无名 11B idle TURN 落在**不同 actor**：C# `recog 9915350/(318,336)`、`recog 1406845931/(318,332)`；Rust `recog 12/(327,327)`、`recog 117/(318,336)` ⇒ 现有「两侧都有对就比 (param,tag,light,phase)」在语义上无法对齐；**去掉 phase 也消不掉**（见 A2-2 / F1） |

**判据建议**：本卡不落 compare_runs 规则（`tools/window/...draft.json` 里 `active=false`），只保留一条草案
**P1（登录出现段按多重集）**，带可执行负例；**P2（去掉 phase）已按 reviewer-21 aafbf8a8 F1 + owner 裁决撤回**——
它消不掉那两条 `random_pair_shape`。**未 ignore ident10/41**，未改场景等待、未放宽 `body_len`。
是否激活由 owner/coordinator 裁决 + A4 真机复跑决定。

> 卡面写「7 条 ident10 + 3 条 ident41」，实测是 **8 条 ident10 + 2 条 ident41**（见 `diff_table`）；
> 分类结论不受影响。

## A1：10 条差异逐帧 + T-270 无关证明

证据根：`E:/tmp/window/run27/softclose_relogin/`（report sha256 `1c2b2794…`，C# frames `ff2c1776…`，
Rust frames `1cd059ef…`）。

| report frame | ident | 差异 | 归属 |
|---:|---:|---|---|
| 21 | 10 | param 332↔327、tag 317↔327、body_len 30↔27 | C# 第 2 帧是 B(332,317,30B)，Rust 第 2 帧是 D(327,327,27B) |
| 22 | 41 | tag 0↔1 | 41 与本侧第 2 个 TURN 配对（C# 跟 B ⇒ 0；Rust 跟 D ⇒ 1） |
| 25 | 10 | param 327↔332、tag 327↔317、body_len 27↔30 | 与 frame 21 互为镜像（C# 第 4 帧 D，Rust 第 4 帧 B） |
| 26 | 41 | tag 1↔0 | 同 frame 22 的镜像配对差 |
| — | 10 | `random_pair_shape` oracle `(318,332,light2,gamegate2)` | C# 的无名 idle TURN 落点/phase |
| — | 10 | `random_pair_shape` rust `(327,327,light2,gamegate)` | Rust 的无名 idle TURN 落点/phase |

**T-270 无关（独立重算）**：自建 `E:/tmp/dev-2/run27-t270-whitelist-cleared.rules.json`
（sha256 `e5d6f7b3…`；只把 ident14 的 `oracle_extra_shapes=[]`、`oracle_extra_status=PENDING`，
其余同仓规则），生成方式：

```python
import json
r = json.load(open("tools/conformance/rules.json", encoding="utf-8"))
for rule in r["rules"]:
    if rule.get("ident") == 14:
        rule["oracle_extra_shapes"] = []
        rule["oracle_extra_status"] = "PENDING"
        rule.pop("oracle_extra_evidence", None)
        rule.pop("oracle_extra_note", None)
json.dump(r, open("E:/tmp/dev-2/run27-t270-whitelist-cleared.rules.json", "w",
                  encoding="utf-8"), ensure_ascii=False, indent=2, sort_keys=True)
```

重跑命令与结果：

```sh
# 仓规则
python -I -B tools/conformance/compare_runs.py \
  --oracle E:/tmp/window/run27/softclose_relogin/csharp \
  --rust   E:/tmp/window/run27/softclose_relogin/rust \
  --out E:/tmp/dev-2/run27-repo-rules.json
# T-270 白名单清空副本
python -I -B tools/conformance/compare_runs.py \
  --oracle E:/tmp/window/run27/softclose_relogin/csharp \
  --rust   E:/tmp/window/run27/softclose_relogin/rust \
  --rules  E:/tmp/dev-2/run27-t270-whitelist-cleared.rules.json \
  --out    E:/tmp/dev-2/run27-nowhitelist-own.json
```

两次都是 `UNDECIDABLE / rc=2`、**10 条差异**，`kind/ident/frame_index/field/oracle/rust` 十元组逐条相同
（report sha256 `d51e2e2f…` vs `c1ed6c46…`）。⇒ 清空白名单不会消除这组差异。

## A2：两类归属的证据

### A2-1 登录出现段顺序（8 条）

两侧具名 TURN 的多重集**完全相等**（各 8 条 = A/B/C/D 各 ×2，`body_sha256` 与 `light=2` 逐项相同）：

| 标记 | (x,y) | body_len | body_sha8 |
|---|---|---:|---|
| A | (318,336) | 27 | `eb5b755b` |
| B | (332,317) | 30 | `d7fbde6e` |
| C | (318,332) | 30 | `18514f46` |
| D | (327,327) | 27 | `fcadedc7` |

顺序：

- C# 登录段 `gamegate`：**A, B, C, D**；重登录段 `gamegate2`：A, D, C, B。
- Rust 登录段 `gamegate`：A, D, C, B；重登录段 `gamegate2`：A, D, C, B。

即：两侧**内容**相同、重登录段**顺序也相同**；唯一差异是 C# 的**登录段顺序**。

**同二进制对照（关键）**：

| 运行 | C# 二进制 sha256 | Rust 二进制 sha256 | scenario sha256 | C# 登录段顺序 | 结论 |
|---|---|---|---|---|---|
| run15 | `ab655008…` | `31c0dec8…` | `11525139…` | A, D, C, B | **PASS（0 diffs）** |
| run27 | `ab655008…`（同） | `31c0dec8…`（同） | `11525139…`（同） | **A, B, C, D** | 10 diffs |

run15(`99a6cdc2`) 与 run27(`481ffbcb`) 之间只改了 `tools/conformance/*` 与 `docs/*`，`tools/probe/`
与 `src/` 零改动 ⇒ **同一 oracle 二进制在两次运行里给出了两种登录段顺序**，这是 oracle 侧
run-to-run 非确定性，不是 Rust 偏差。

跨运行观测（softclose_relogin，按 4 条具名 TURN 的位置序列）：run3/run4/run5/run12/run15 的 C#
登录段都是 `A,D,C,B`，只有 run27 是 `A,B,C,D`；Rust 在 run5/12/15/27 恒为 `A,D,C,B`（与另外 5 次 C# 一致）。

> 关于「Rust 首帧多一条 (327,327,body11) 后整体后移一拍」的读法（dev-1 07e9b78b 的 hypothesis）：
> 两侧 ident10 总数**相同**（各 10 条 = 8 条具名尾 + 2 条 11B 无名尾），不是 Rust 多出一帧。
> 差别是**无名 idle TURN 的落位**：Rust 的第一条在登录段之前（idx19，`gamegate`，t=879），C# 的两条
> 都在重登录之后（idx73/75，`gamegate2`）；`random_branch_pairs` 把它摘成成对帧后，两侧保留下来的
> 具名段都是 8 条，10 条差异是这 8 条的**顺序/配对**差，不是「多一帧」造成的整体位移。

### A2-2 随机成对帧（2 条）

`random_branch_pairs` 只把 **body_len==11** 的无名 TURN 与其后同 recog 的 41 当「随机分支对」：

| 侧 | 无名 TURN #1 | 无名 TURN #2 |
|---|---|---|
| C# | (318,336) dir4 light2 @ `gamegate2` | (318,332) dir5 light2 @ `gamegate2` |
| Rust | (327,327) dir1 light2 @ `gamegate` | (318,336) dir7 light2 @ `gamegate` |

- 对数相同（各 2 对）、`light` 相同（2），但**被抽中的 actor 不同**（`recog` 与坐标都不同）⇒
  共享全局 RNG 的交错（C# 无锁 `Random` + 多线程 `ProcessMonsters`，见 T-266 证据）在
  `Merchant.Run` 的 `Random(50)`/`RandomByte(8)` 上选了不同的 NPC/时机。
- **P2 撤回（reviewer-21 aafbf8a8 F1）**：把 `phase`（驱动侧连接/阶段标签 `gamegate`/`gamegate2`）
  从形状里去掉**消不掉这两条**——去 phase 后仍得 C# 形状集 `{(318,336,2),(318,332,2)}` vs
  Rust `{(327,327,2),(318,336,2)}`。phase 只是加剧差异，不是根因；`classification.pair_shape`
  因此改为 **`RNG_CHOOSES_ACTOR`（不同 actor 抽中的 idle turn，归属不可比）**，两条保持 `UNDECIDABLE`。
- 另：具名 TURN 的 `series`（Dir）两侧也全部不同（C# 1/7/7/6…，Rust 4/1/1/4…），但这是
  ident10 的**既有**整字掩码（`head_mask:["series"]`，oracle 墙钟随机不可复现），不是本卡新增豁免。

## A3：判据草案（对称 + 负例，不 ignore 10/41）

草案落在 `tools/window/rules-run27-relogin-turn-shape-diffs.draft.json`（`active=false`，
不可被 compare_runs 加载；未激活前没有任何归一化效果）：

- **P1（登录出现段按多重集）**：同一 phase 内连续成对的具名 TURN(+41) 段，当且仅当**对数相等**且
  `(param,tag,light,body_sha256)` 多重集**完全相同**时按多重集比较；负例：段内任一
  param/tag/light/body 不同、对数不同、跨 phase 合并 ⇒ 仍红；段外帧（含 11B idle turn）不受影响。
- **P2 已撤回**（reviewer-21 aafbf8a8 F1 + owner 裁决）：它消不掉两条 `random_pair_shape`；
  草案里移入 `withdrawn_criteria`，并把理由与「若将来要归一只能走 recog 交集才比形状、需 owner R 裁决」
  的替代方案写明。

两条性质都不是 ignore：10/41 帧仍在流里，P1 只是把**已证明非契约的维度**（登录段顺序）从判据里拿掉，
且以「多重集/其余字段全等」为前提。

### P1 的可执行负例（本卡内可复跑）

草案 `draft_criteria[0].selftest_cases` 里有 6 条合成用例；下面这段**读草案并断言**，可直接复跑：

```python
import json
d = json.load(open("tools/window/rules-run27-relogin-turn-shape-diffs.draft.json", encoding="utf-8"))
p1 = next(c for c in d["draft_criteria"] if c["id"].startswith("P1"))


def key(p):
    return (p["x"], p["y"], p["light"], p["body_sha256"])


def runs(seq):
    out = []
    for p in seq:
        if out and out[-1][0] == p["phase"]:
            out[-1][1].append(key(p))
        else:
            out.append((p["phase"], [key(p)]))
    return [(ph, tuple(sorted(v))) for ph, v in out]


def p1_equal(oracle, rust, *, always_equal=False):
    return True if always_equal else runs(oracle) == runs(rust)


ok = True
for case in p1["selftest_cases"]:
    got = p1_equal(case["oracle"], case["rust"])
    hit = got == case["expect_equal"]
    ok = ok and hit
    print(("PASS" if hit else "FAIL"), case["name"], "->", got)

negatives = [c for c in p1["selftest_cases"] if not c["expect_equal"]]
mutant_broken = [c["name"] for c in negatives
                 if p1_equal(c["oracle"], c["rust"], always_equal=True) != c["expect_equal"]]
print("mutation(always-equal) breaks %d/%d negative cases" % (len(mutant_broken), len(negatives)))
print("CHECK", "GREEN" if ok and len(mutant_broken) == len(negatives) and negatives else "RED")
```

实测输出（2026-10-29，本 worktree）：

```text
PASS 正例：多重集相同、登录段顺序不同 => 等价 -> True
PASS 负例：某对的 body_sha256 不同 => 不等价 -> False
PASS 负例：某对的 light 不同 => 不等价 -> False
PASS 负例：对数不同（oracle 4 vs rust 3）=> 不等价 -> False
PASS 负例：跨 phase 不得合并（oracle g1,g1,g2,g2 vs rust 全 g1）=> 不等价 -> False
PASS 段外非成对帧不参与 P1（同一 pairs、额外 context 帧 => 仍等价） -> True
mutation(always-equal) breaks 4/4 negative cases
CHECK GREEN
```

> 这组牙目前挂在**文档内可复跑片段**上（本卡写集＝`tools/window/rules*` + 本文档）；要进 CI 需另开卡
> 新增 `--check` 工具并在 `tools/hygiene/tool_selftest_gate.py` 登记、同步两处认树数字（会扩写集）。

## A4：真机窗口（未跑，本卡不 green）

1. 当前 master 真机复跑 run27 场景**两次**（同二进制、带 `--rng-log`）：确认 C# 登录段顺序是否仍在
   `A,B,C,D` / `A,D,C,B` 间跳变；若稳定为同一顺序，P1 需重新评估。
2. 同两次运行核对无名 idle TURN 的 `Random(50)`/`RandomByte(8)` 抽取对象与时机，确认是 RNG 流位差
   而非 Rust 多抽/少抽。
3. owner/coordinator 对 P1 的 R 级裁决，以及对「随机 idle turn 的 actor 归属是否可比」的 R 级裁决
   （后者决定两条 `random_pair_shape` 是否永远保持 `UNDECIDABLE`）；未裁决前 10 条差异保持
   `UNDECIDABLE`，不得用 ignore 洗绿。

## 残留物

- 证据（保留）：`E:/tmp/dev-2/run27-repo-rules.json`、`E:/tmp/dev-2/run27-nowhitelist-own.json`、
  `E:/tmp/dev-2/run27-t270-whitelist-cleared.rules.json`（自建；生成命令见 A1）；
  输入 `E:/tmp/window/run27/softclose_relogin/`、对照 `E:/tmp/window/run15/softclose_relogin/`。
  reviewer-21 的临时副本 `E:/tmp/window/reviewer-21-review/rules-270-nowhitelist.json` 只作交叉对照
  （跨 lane 路径不稳定，已不作为本卡证据根）。
- 过程产物（可删）：`E:/tmp/dev-2/make-run27-draft.py`、`E:/tmp/dev-2/run27.txt`。
