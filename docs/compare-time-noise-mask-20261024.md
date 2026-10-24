# ident=100「@time 回显」时间文本掩码（compare_runs.py）——交付报告

- 卡：`openmir2-compare-time-noise-mask`（tier=L，role=worker，owner=openmir2-worker-58）
- base：`walgit/master` = `133c0f6a1`；分支 `card/compare-time-noise-mask`
- 写集：`tools/conformance/compare_runs.py`、`tools/conformance/selftest_impl.py`
  （compare_runs 工具的自检模块，验收④ selftest 的载体）、`docs/`
- 判据：加掩码后 `gm_time` / `gm_batch` 的帧流比较变 PASS，且其他场景判定不受影响。

## 1. 现象与定性

真机窗口第 1 轮（`docs/window-run-1-20261024.md` §3 第 3 条）：`gm_time` / `gm_batch`
两侧 ident=100（SM_SYSMESSAGE，GM 命令 @time 的回显）body 等长但差 3 字节——
线缆 offset 38/41/42。EDCode 解码后明文为：

```
oracle: 当前日期时间: 2026-10-24 13:46:43
rust:   当前日期时间: 2026-10-24 13:47:57
```

时间戳数字来自**发送侧墙钟**，两次独立运行必然不同 ⇒ 环境噪声（tier 判定同型于
`t_ms` 全局掩码），非实现差异。明文[i] → 线缆[4*(i//3)+i%3]：明文差 [29,31,32]
（时分秒位）正好对应线缆差 [38,41,42]。

## 2. 掩码规则（规范定义）

只作用于 **ident==100** 的 body，且必须同时满足以下条件才掩，否则完全退回原逐字节
路径（fail-closed，掩码绝不放宽）：

1. body 能**严格** EDCode 解码（`mir2frame.decode_bytes(strict=True)`，任一字节越界
   即失败）；
2. 明文命中「前缀 `当前日期时间: ` + `\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}`」——前缀是
   静态文案，**照常逐字节比**（文案写错照样红）；只有 Match 组 1（时间戳数字段，
   明文 19B）被替换为**等长**占位符 `YYYY-MM-DD HH:MM:SS` 后再比；
3. 掩码生效范围 = 该 Match 区间；同 body 内其他字节、以及任何非 100 的 ident，
   一律照原口径比。

判定输出（绝不静默 PASS）：

* 掩码后相等、原字节不同 ⇒ `whitelisted(kind=time_text_masked)` 显式登记
  （diff_bytes / 明文区间 / tier / 依据）；
* 掩码后仍不同 ⇒ diff，定位在**明文空间** offset（时间位之外的真实差异不被掩）；
* 掩码命中本身 ⇒ 报告 `masked_ranges(space=plain_time_text)` 逐帧可审计。

多重集口径：ident=100 的多重集键（`_multiset_body_token`）同样用「掩码后明文」做
body token——否则「同一文案、只差时分秒」会在集合比较里被误判成缺失/多余帧；
掩码未命中时键与掩码引入前完全一致。

## 3. 实现（tools/conformance/compare_runs.py）

* 常量：`TIME_TEXT_IDENT=100`、`TIME_TEXT_PREFIX`（GBK `当前日期时间: `）、
  `TIME_TEXT_RE`（前缀 + ISO 时间戳，组 1 = 时间戳数字段）、`TIME_TEXT_PLACEHOLDER`
  （等长 19B）；规则表 `rules.json` **未改**（掩码是比较器内置，按卡红线）。
* `time_masked_plain(body)`：解码 + 命中 + 掩码；失败/未命中返回 None。
* `_multiset_body_token`：ident==100 且掩码命中 ⇒ 键用 `plain-time-masked:` 前缀的
  掩码明文 hex；否则原 wire hex token。
* `_compare_time_masked_body`：ident==100 的 body 判定分支（登记/定位口径见 §2），
  挂在 `compare_bodies` 的 ability/bagitems 之后、mask_body_ranges 之前；返回 False
  即完全走原路径。
* 模块 docstring「判据 3) body」补 `time_text` 条目；自检 docstring 补第 15 条。

## 4. 真机数据验证（实跑）

数据（原始产物未动）：

```
d7daa9a734e15700b580d9d6466aecb873b1749c8e270aaa80e875651126ac5d  E:/tmp/window/run1/gm_time/csharp/frames.ndjson
a0491c22954aac22e6e8f4754582dff43d9342597e324c2dae6e850d7e4b35ee  E:/tmp/window/run1/gm_time/rust/frames.ndjson
7f037a2fe851c17ef6ffb7802f0e73d87c88bd531d8e1b561c45fc7cbada70cb  E:/tmp/window/run1/gm_batch/csharp/frames.ndjson
3e6ffa34f2901ac2ad53e7d8977c338921e29d8d6a0688f050ab25ca1cb1cb5c  E:/tmp/window/run1/gm_batch/rust/frames.ndjson
```

命令与结果（修后，head 见 §8）：

```
$ python -I -B tools/conformance/compare_runs.py compare \
    --oracle E:/tmp/window/run1/gm_time/csharp --rust E:/tmp/window/run1/gm_time/rust --out -
VERDICT: PASS (exit 0)
  time_text_masked ident=100 field=body x1        # 显式登记，非静默
  masked_order: 1 组                               # 欢迎 4 条的顺序抖动照旧登记

$ python -I -B tools/conformance/compare_runs.py compare \
    --oracle <tmp>/gm_batch/o --rust <tmp>/gm_batch/r --out -   # 仅 frames.ndjson 的副本目录
VERDICT: PASS (exit 0)                              # 帧流：FAIL ⇒ PASS
  db: state=absent_both（副本目录无库快照，显式登记未比较）

$ python -I -B tools/conformance/compare_runs.py compare \
    --oracle E:/tmp/window/run1/gm_batch/csharp --rust E:/tmp/window/run1/gm_batch/rust --out -
VERDICT: FAIL (exit 1)
  DIFFS: 1 —— 仅剩 db 后态 characters_ablity.Weight 14 vs 0（key 6228）
```

说明：`gm_batch` 全量目录比较仍红，**唯一**残余 diff 是库后态 `Weight` 14 vs 0——
该数值列差归 `openmir2-login-numeric-gaps` 卡处置（window-run-1 报告 §3 第 1 条），
不属于本卡的时间噪声范围，掩码刻意不触碰库口径。帧流部分（本卡的判据对象）已
由 FAIL 转 PASS，`time_text_masked` 登记条数与真机 @time 帧数一致（两场景各 1 条）。

## 5. 回归（18 场景全量 sweep，同一数据、修前修后各跑一遍）

对 `E:/tmp/window/run1/` 全部 18 个场景跑 `compare_runs.py`，verdict 与 diff/undecidable
条数**逐一相同**，只有本次目标两处变化：

| 场景 | 修前 | 修后 |
|---|---|---|
| gm_time | FAIL（2 条 diff） | **PASS**（0 条） |
| gm_batch | FAIL（3 条 diff） | FAIL（**1 条** = Weight 库 diff，见 §4） |
| 其余 16 场景 | — | 逐一相同（含 npc_click_auto=PASS、npc_click=FAIL 的帧序差异、12 个 UNDECIDABLE） |

`npc_click`（帧序错位）与 `npc_click_auto`（diff=0）的判定不受掩码影响；含 ident=100
欢迎文案的场景（登录流程均有）因文案不含时间戳模式，掩码不命中、行为逐字节同前。

## 6. selftest（compare_runs selftest，274/274 GREEN）

新增 `t_time_text_mask` 12 条断言（`tools/conformance/selftest_impl.py`），夹具用
GBK 明文 → EDCode 线缆编码，且以真机 @time body 的 hex（`REAL_TIME_WIRE`）钉住
编码形态：

1. 夹具往返无损（33B 明文）+ 与真机同 hex；
2. ① 只差时间文本位 ⇒ 掩码生效判定 PASS + `time_text_masked` 登记（diff_bytes=3、
   明文区间 [[14,33]]）+ `masked_ranges` 命中登记；
3. ② 掩码命中、但时间位之外备注文案不同 ⇒ 仍判 FAIL（多重集缺失/多余点名具体帧），
   且不出现 `time_text_masked`；
4. ②b（ident=100 改逐字节规则）掩码后真差异 ⇒ FAIL 定位在**明文** offset=37 并带
   时间区间证据；②c 前缀静态文案被改（间→刻）⇒ 掩码不命中 ⇒ FAIL 定位线缆
   offset=13（前缀照比）；
5. ③ 掩码后明文长度仍不同 ⇒ FAIL（body_len，明文空间）；
6. ④ 牙：同形时间差放 ident=658 ⇒ 掩码不越 ident，仍 FAIL 且定位线缆 offset=38
   （与真机证据同 offset）、无 `time_text_masked`；
7. ⑤ 牙：body 非合法线缆字节（明文直写）⇒ 解码失败退回逐字节 ⇒ FAIL，不掩不登记。

## 7. 门禁（全部在本卡 head 实跑）

| 门禁 | 结果 |
|---|---|
| `compare_runs.py selftest` | GREEN 274/274，rc=0 |
| `card_window.py --selftest` | GREEN 240/240，rc=0 |
| `stale_pointer_gate.py` | OK，rc=0 |
| `dynamic_count_stamp_gate.py` | OK（0 缺戳），rc=0 |
| `git diff --check` | 干净 |
| CRLF=0 / BOM=0 | 干净（见 §8 核查命令输出） |
| `git merge-tree --write-tree walgit/master HEAD` | 干净（见 §8） |

**顺手修复（披露）**：`dynamic_count_stamp_gate` 在 base `133c0f6a1` 上本来就红
（rc=1）：`docs/window-run-1-20261024.md` 两处「FAIL 5」计数缺快照戳（该报告由
`efb8c9314` 提交，worker-67 的 `13e5d20cb` 只补了引用文档的戳、漏了自身计数行）。
本卡按写集（docs/）补上两枚 `@ efb8c9314 @ 2026-10-24` 戳（§2 计分行、§4 结论行），
使门禁回到 rc=0。改动仅追加戳注，不动数字与结论。

## 8. head 与核查命令

```
head:   <见线程 patch 条目>
$ git diff --check ; echo rc=$?            # rc=0
$ git grep -Il $'\r' -- tools/conformance docs | wc -l   # 0（CRLF=0）
$ python - <<'PY'                          # BOM=0
for p in ["tools/conformance/compare_runs.py","tools/conformance/selftest_impl.py",
          "docs/window-run-1-20261024.md","docs/compare-time-noise-mask-20261024.md"]:
    assert not open(p,'rb').read(3).startswith(b'\xef\xbb\xbf'), p
print("BOM=0")
PY
$ git merge-tree --write-tree walgit/master HEAD   # 无冲突
```

## 9. 残留物清单

| 路径 | 处置 |
|---|---|
| `E:/tmp/wt-worker-58/.tmp-gmbatch/`（frames.ndjson 副本） | 已删（验证用临时副本） |
| `.baseline-verdicts.txt` / `.after-verdicts.txt` / `.g1..g4.log` | 已删（sweep 与门禁的临时输出） |
| `E:/tmp/window/run1/**` | 原始真机产物，未改动（只读） |
