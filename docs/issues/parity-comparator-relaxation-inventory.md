# 比较器"放宽面"清单（审计卡）

> **被核 head**：本地 `master = d0b377680`（该树比远端 `d592f66` 多一笔未跑门的 merge；经核
> `golden/` 与比较器在这笔里未变，结论对远端同样成立）。审计为**只读**，本卡不改代码。
> 依据：`mir2-rs/tests/parity/golden/magic/*.json` 与
> `mir2-rs/tests/parity/tests/magic_fireball_parity.rs`（**高 churn 文件 ⇒ 本卡一律用符号锚，
> 不写行号**）。

## 0. 结论（先看这段）

- `frame_order_scope = "multi_victim_same_tick"` 的实际覆盖面是 **6 个 stem / 18 条 case**，
  对应 **4 个技能**（9、10、6、48）。
- 这 6 个 stem 在清单上落 **8 个条目**：**MG-13 / MG-14 / MG-15**（6 的三个拆法）、
  **MG-17**（9）、**MG-18**（10）、**MG-50 / MG-64 / MG-65**（48 的三个拆法）。
  ⇒ **"6 个 stem" ≠ "6 个清单条目"**；用"4 个条目"表述时它指的是 4 个技能，不是清单行数。
- 另有两类**不经过该字段**的放宽：**6 条由参数或 stem 名授权**（§2）、**2 条全局口径**（§3）。
  做队列化/收紧时请按本表逐条对照"移除后哪支会红"。

## 1. ① 由 `frame_order_scope` 字段授权

**判据位**：`magic_fireball_parity.rs` 的 `real_ledger_verify`（`multi_victim` 读
`gc["Fixture"]["frame_order_scope"]`）；**多重集键** `sent_key` / `due_key`；
**自检**：`real_ledger_verify_labels_opts` 的 `scope_marked` 白名单（stem 不在白名单 ⇒ panic）
+ `stem ↔ magic_id` 静态一致性。

| stem（golden 文件） | 魔号 | 放宽了什么维度 | 移除后哪支会红 |
| --- | --- | --- | --- |
| `fire.json` | 9 | 该接收者 `Sent` 的**顺序**（改多重集）；`due_ms_rel` 仍严格进键；字段全比 | `real_rust_ledger_fire`：多受害者同刻（批量 flush vs 逐 actor `Run()`） |
| `skill10.json` | 10 | 同上 | `real_rust_ledger_skill10`：同上 |
| `amyounsul.json` / `amyounsul2.json` | 6 | `Sent` 顺序；`due_ms_rel` 只判批次类（0 vs ≠0），640 另按 `|Δ|≤32ms` | `real_rust_ledger_amyounsul` / `real_rust_ledger_amyounsul2`：毒派生帧（656/657/100）的跨 actor 序 |
| `groupamyounsul.json` / `groupamyounsul2.json` | 48 | 同上 | `real_rust_ledger_groupamyounsul` / `real_rust_ledger_groupamyounsul2`：同上（群体毒） |

实测枚举（被核 head）：顶层 golden 52 个；带该键 29 个；**值非 null 恰 6 个文件 / 18 条 case**
（每文件 3 条，值全为 `multi_victim_same_tick`）；其余 23 个为 `null`、23 个无该键。

## 2. ② 由参数或 stem 名授权（**不是**该字段）

| # | 放宽点（符号锚） | 放宽了什么维度 | 涉及 stem | 移除后哪支会红 | **它掩盖了什么（已登记？）** |
| --- | --- | --- | --- | --- | --- |
| 1 | `strip_empty_receivers`（`real_ledger_verify_labels_opts` 的 `drop_empty_receivers` 参数；调用点是 `real_rust_ledger_holyshield`） | **收件人集合**：两侧都删"零帧收件人" | `holyshield` | `real_rust_ledger_holyshield`（golden 的 `mon0`/`mon1` 零帧、台账只登记有帧的收件人） | **台账/装置表形差异**（零帧收件人只在 golden 登记），**不是行为差异**；非空收件人仍逐帧严格比。✔ 已登记（比较器注释） |
| 2 | `sort_sm_ident`（`real_ledger_verify_labels_opts` 的 `stem == "groupdeding"` 分支，ident=716） | **SM_716 组**的组内顺序（等优先级广播的 4 叉堆 artifact） | `groupdeding` | `real_rust_ledger_groupdeding` | **SM_716 的组内 tie 序**：宿主把 `RM_SPELL` 直发、不进缓冲 ⇒ 复现不出"带 17 时 221 组的 tie 序"（与全局队列模型同源）。✔ 已登记（该比较器注释） |
| 3 | `sent_filter`（`keep = [17, 638, 642]`；调用点是 `real_rust_ledger_skeleton`） | **比较面收窄**：只比选定 ident，且按 socket 多重集 | `skeleton` | `real_rust_ledger_skeleton`（若改全帧严格：造怪替身相关非可比帧 + 帧序） | **①`638` vs `642` 的顺序**：golden `638,642` / Rust `642,638`（oracle 队列 `638=136 < 642=141` vs Rust 立即发 642）——属 `openmir2-refmsg-queue-model.md` 的全局队列差异；**②**640 的 `tag` 由 #6 单独排除。✔ 已登记（全局队列卡 + #6 注释）；**但本比较器注释未点名 638/642 这一项**，见 §2b |
| 4 | `sent_filter_sinsu`（`keep = [17, 638, 642]`；调用点是 `real_rust_ledger_sinsu` 两处） | 同上 + 显式比 `recog` / `due_ms_rel` | `sinsu` | `real_rust_ledger_sinsu` | 同 #3 的 638/642 顺序；**另：`807`（回召）帧未建模**（golden `cap_hit` caster 有 `807`，Rust 无）。✔ 后者已登记（该比较器注释），前者同 #3 |
| 5 | `sent_filter`（`keep = [17, 638, 642]`；调用点是 `real_rust_ledger_angel`） | 同上（与 skeleton 同形） | `angel` | `real_rust_ledger_angel` | 同 #3 的 638/642 顺序（golden `638,642` / Rust `642,638`）。✔ 已登记（全局队列卡）；本比较器注释未点名，见 §2b |
| 6 | `frames_640_caster`（只取 `(recog, param, series, due_ms_rel)`，**不含 `tag`**） | **640 的 `tag`（TranPoint）不比** | `skeleton` / `sinsu` / `angel` | 对应比较器：离线替身造怪 0 抽 ⇒ RNG 流偏移 ⇒ golden=2 / Rust=1 | **造怪抽数不同 ⇒ TranPoint 不同**（离线替身 0 抽 vs Rust `create_monster_oracle`）。✔ 已登记（`frames_640_caster` 注释） |

### §2b. 逐条自查的结论

**逐条查过：6 条放宽都对应已登记的差异，没有一条是无记录的静默口径。** 差异只有 5 类：

1. `638` vs `642` 的顺序（#3/#4/#5，skeleton/sinsu/angel）——oracle 队列 `638=136 < 642=141`，
   Rust 立即发 642；属 `openmir2-refmsg-queue-model.md` 的全局队列差异。**⚠ 本清单是第一次把
   它明确归到这三个 stem 的 `sent_filter` 名下**（此前只在该全局卡与 MG-16/57 的 notes 里）。
2. `807`（回召）帧未建模（#4，sinsu 的 `cap_hit`）——比较器注释已登记。
3. `640` 的 `tag`（TranPoint）随造怪抽数不同（#6）——`frames_640_caster` 注释已登记。
4. `SM_716` 组内 tie 序（#2，groupdeding）——该比较器注释已登记。
5. 零帧收件人的台账表形差异（#1，holyshield）——比较器注释已登记，非行为差异。

**给后续收紧者的提醒**：要动 #3/#4/#5，先看 `openmir2-refmsg-queue-model.md`——638/642 的顺序
不是这三个夹具的局部问题，而是宿主"直发 vs 入队"的全局差异；单改 `sent_filter` 会把这层
掩盖一起揭开。

## 3. ③ 全局口径（不是 per-stem 放宽）

| # | 放宽点（符号锚） | 放宽了什么维度 | 涉及 stem | 移除后哪支会红 |
| --- | --- | --- | --- | --- |
| 1 | `verify` 的 RM 多重集（`rm_keys` 排序后比较） | **RM 列的顺序**（字段仍全比） | **所有** stem | 所有走 `verify` 的比较器：C# 侧 `MsgQueue` 两趟排空 + `VisibleHumanList` 缓存决定顺序，不构成行为语义 |
| 2 | 严格分支的 **640 `due_ms_rel` `|Δ|≤32ms`** 容差（`multi_victim` 之外的 `sent_fields` 循环） | **640 的 due 精确毫秒**（行为判据是批次类 0 vs >0） | 所有**非** `multi_victim` 且含 640 的 stem（`firewind`/`energyrepulsor`/`spacemove`/`angel`/`skeleton`/`sinsu` 等） | 收紧为严格相等后：凡 C# 生成时 `Environment.TickCount` 跨 tick（1000↔1016）的 stem 会红——`spacemove` 的 golden 里**实测已有 1016** |

## 4. 统计时的假命中坑（写给以后用 grep 的人）

清单 `docs/issues/parity-gap-magic.json` 里 **MG-46 的 evidence 是 `unamyounsul.json`**，
其中**包含子串** `amyounsul.json`。任何 `grep 'amyounsul.json'` 的统计都会把 MG-46 多算进来。
**正确做法**：按精确 stem（`/amyounsul.json` 带路径分隔符）或解析 JSON 后比对
`evidence.fixture`，不要用裸子串 grep。

## 5. 复现命令（被核 head 上执行）

```text
# 枚举：非 null 的 frame_order_scope（应恰 6 个文件、各 3 条）
for f in $(git ls-tree --name-only master mir2-rs/tests/parity/golden/magic/ | grep '\.json$'); do
  echo "$f"; git show master:$f | grep '"frame_order_scope"' | grep -v ': null'
done

# 清单映射（解析 JSON，避免 §4 的子串假命中）
python - <<'PY'
import json; d=json.load(open('docs/issues/parity-gap-magic.json'))
stems=['amyounsul.json','amyounsul2.json','fire.json','skill10.json',
       'groupamyounsul.json','groupamyounsul2.json']
for e in d['entries']:
    hit=[s for s in stems if s in json.dumps(e.get('evidence') or {},ensure_ascii=False).replace('unamyounsul.json','')]
    if hit: print(e['id'], hit)
PY
```
