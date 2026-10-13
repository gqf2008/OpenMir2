# evidence-m3-anchor-token-class-20261014

M3 夹具族**残余 Observation O-1 收口**片（离线、零起栈、零端口、零 `.rs`）。卡：
`openmir2-m3-fixture-chain-gate`（夹具链深检门 umbrella）；本片关闭的 O-1 由 `openmir2-reviewer-6`
在 `f97370f3`（⑧-6 / ⑧-10 / ⑨-5 三锚门复核）里点名，并被 owner 队列（2026-10-14 00:2x/00:3x）
列为「⑧ 行会/攻城夹具 + 共享管线链深检门的残余 Minor」。

tier：**L**（纯夹具/工具：3 个 `.py`；不触 RNG 抽数序列、时序、持久化落库、战斗数值
⇒ 等价 + 抽样冒烟即放行）。

## O-1 是什么（reviewer-6 `f97370f3`，逐字）

> 「读侧 token 强度」牙的纯标点字符类不完整：仅覆盖 `{}();,` 与空白（正则 `^[{}();,\s]*$`），
> 不含方括号/尖括号等，故整组 token 只剩 `]);` 或 `[]` 时判为『有辨识度』而不报红 ——
> 构造验证：`_token_strength_findings([('X.cs:1-2',_,[(2,']);')]) = []`（token `}`/`;` 则正确报红）。
> …… 若后续想更严可把字符类扩到 `[][<>+=/\-]` 这类。

`where` = `castle_oracle_anchor_gate.py / guild_castle_anchor_gate.py / hero_pet_anchor_gate.py`
的 `_WEAK_ONLY_TOKEN_RE`。

## 本片修了什么

把三门锚门的强度判据由「枚举 `{}();,` 之外的字符算强」改为「**必含字母/数字/下划线/CJK 才算强**」：

```python
_WEAK_ONLY_TOKEN_RE = re.compile(r'^[^0-9A-Za-z_\u4e00-\u9fff]*$')
```

⇒ 对**任何**纯标点/空白组合都判弱（含 `]);` / `[]` / `*/` / `//{` / `=` / `<` …），
含内容 token 的整组不误报。三门**真身锚表在新谓词下仍全绿**（castle 159 / guild 31 / hero_pet 27
锚，`check` 逐锚对冻结 `src/` 机判）。

配牙（防回退）：

* 各门 `selftest` 的 token 强度牙扩为两段：`}`-型（原有 MUT5/MUT9 覆盖） + **O-1 纯标点 4/4**
  （`]);`/`[]`/`*/`/`//{` 必须逐条咬住；第 5 条带 `foo` 的整组不误报）。
* 各门 `mutation` 增一颗 O-1 牙：castle **MUT10** / guild **MUT8** / hero_pet **MUT6**
  （O-1 点名的 `]);` 型整组必须 RED）。

## 读数（本片实跑，原始件见同目录）

| 项 | 读数 | 原始件 |
| --- | --- | --- |
| O-1 差分（旧类 vs 新谓词，同一批整组） | 旧 `^[{}();,\s]*$` 咬住 **0/4**、`]);` ⇒ 无 finding（复现 O-1）；新谓词咬住 **4/4**、`]);` ⇒ RED，带 `foo` 的第 5 条不误报 | `01` |
| castle 锚门 | check GREEN **159 锚**；selftest GREEN（token 强度牙含 O-1 pure-punct=4/4）；mutation **MUT1–MUT10 GREEN** | `02` |
| guild/castle 锚门 | check GREEN **31 锚 / 92 行点**；selftest GREEN（O-1 pure-punct=4/4）；mutation **MUT1–MUT8 GREEN** | `03` |
| hero/pet 锚门 | check GREEN **27 锚 / 57 行点**；selftest GREEN（O-1 pure-punct=4/4）；mutation **MUT1–MUT6 GREEN** | `04` |
| 夹具链深检门（runner 级） | **FIXTURE_CHAIN GREEN: runner_ok=10 prereq_or_skip=0 red=0**（6 条主机门全 rc=0） | `05` |
| 链深检门自检 | `SELFTEST GREEN: fixture_chain_gate selftest 31/31` | `06` |
| 外部改坏必红 | 把 castle 门谓词退回旧类 ⇒ **selftest RED**（O-1 pure-punct=0/4）+ **mutation RED**（MUT10 0+0 未咬住）；`check` 仍 GREEN（真身无弱锚 ⇒ O-1 那类只能靠牙咬）；真身 sha256 前后一致 | `07` |
| 四门禁 | fmt rc=0 / clippy rc=0(-D warnings) / 101 suites 1200 passed 0 failed 22 ignored / msgcodegen 604 rc=0 | `08`–`11` |

## 边界

见 `12-boundary.txt`：base = `origin/master` `6afa376363d3378933d95c26c45cd8eb7c9738f3`；
净差量全在 `mir2-rs/tests/parity/m3/**`（3 个锚门工具 + 本证据目录）；
冻结面 `src sql MirClient mir2-rs/crates/protocol mir2-rs/crates/storage mir2-rs/bins` = **空**；
`*.rs` 计数 = **0**。

## 残留（如实登记）

1. 锚门「行内容 ⇄ 散文声称构造」的**语义层**仍是抽样人核（本片只把**可机判**的「有辨识度 token」
   判据补全——O-1 的点名边界 `]);`/`[]` 已闭合；not_verified）。
2. ⑧-6 / ⑧-10 / ⑨-5 三卡的**卡级真机 DoD**（两侧同序列 `run_*.ps1` `diff=0`、
   `openmir2-m3-clock-injection` + owner 独占窗口）**不在本片范围**，卡保持 `in-progress`。
3. `whitelist.md` 的 **T-42 撞号**（L227 ⑤-2 商人 tick vs L240 ⑥-5 任务夹具）是
   **已路由 owner 的历史问题**（见 `m3_whitelist_registry_audit.py` 文件头 + `whitelist.md` 末段
   「T-48 留给 T-42 撞号去重」），本片**不碰**。
