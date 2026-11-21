# B18 重同步证据（58 支；落 49、9 支暂不落盘＝B22/B17 欠账，2026-11-22）

由 walgit/master `7f21aa6a3`（已回滚 (a)：emit `due_ms_rel`；三路径虚拟钟）逐支重出，每支 3× 逐字节稳定（不稳定支：`无`）。**落 49 支**（行尾对齐、无整文件 churn）；**9 支暂不落盘**（旧 golden 无 due ⇒ 比较器不比 ⇒ 绿，不在缺口上盖新值）；`mabmabe-nondefault` 单列（CharStatus 0x20，B17）。

| stem | S | T | Y | due(old→new) | diff+ | diff- | 落 | 3× |
|---|---|---|---|---|---|---|---|---|
| amyounsul | 2 | 1 | 0 | 有→有(0/900/1000→0/1000) | 26 | 20 | 落 | ✓ |
| amyounsul2 | 2 | 1 | 0 | 有→有(0/900/1000/1016→0/1000) | 20 | 14 | 落 | ✓ |
| angel | 2 | 1 | 0 | 有→有(=) | 5 | 1 | 落 | ✓ |
| bigcloak | 12 | 1 | 0 | 无→有(0/800/1000) | 120 | 9 | 暂不 | ✓ |
| bighealing | 8 | 1 | 0 | 有→有(=) | 29 | 5 | 落 | ✓ |
| cloak | 12 | 1 | 0 | 无→有(0/1000) | 96 | 3 | 落 | ✓ |
| dejiwonho | 8 | 1 | 0 | 有→有(=) | 27 | 3 | 落 | ✓ |
| energyrepulsor | 2 | 1 | 0 | 有→有(=) | 9 | 3 | 落 | ✓ |
| fire | 8 | 1 | 0 | 有→有(=) | 146 | 26 | 落 | ✓ |
| fireball | 3 | 1 | 0 | 无→有(0) | 66 | 2 | 暂不 | ✓ |
| fireball-5 | 12 | 1 | 0 | 无→有(0) | 93 | 2 | 暂不 | ✓ |
| fireball-nonzero | 0 | 1 | 0 | 有→有(=) | 2 | 2 | 落 | ✓ |
| fireboom | 8 | 1 | 0 | 有→有(=) | 27 | 3 | 落 | ✓ |
| fireboom47 | 8 | 1 | 0 | 有→有(=) | 27 | 3 | 落 | ✓ |
| firecharm | 12 | 1 | 0 | 无→有(0) | 99 | 2 | 暂不 | ✓ |
| firecross | 2 | 1 | 0 | 有→有(=) | 8 | 2 | 落 | ✓ |
| firewind | 2 | 1 | 0 | 有→有(=) | 9 | 3 | 落 | ✓ |
| groupamyounsul | 2 | 1 | 0 | 有→有(=) | 12 | 6 | 落 | ✓ |
| groupamyounsul2 | 2 | 1 | 0 | 有→有(=) | 12 | 6 | 落 | ✓ |
| groupdeding | 13 | 1 | 0 | 无→有(0/1000) | 278 | 16 | 暂不 | ✓ |
| grouplightening | 13 | 1 | 0 | 无→有(0) | 111 | 3 | 暂不 | ✓ |
| hangmajinbub | 8 | 1 | 0 | 有→有(=) | 27 | 3 | 落 | ✓ |
| hbfireball | 8 | 1 | 0 | 有→有(=) | 27 | 3 | 落 | ✓ |
| healing | 8 | 1 | 0 | 有→有(=) | 28 | 4 | 落 | ✓ |
| holyshield | 13 | 1 | 0 | 无→有(0/1000) | 180 | 8 | 落 | ✓ |
| lightflower | 8 | 1 | 0 | 有→有(=) | 27 | 3 | 落 | ✓ |
| lightflower-nonzero | 0 | 1 | 0 | 有→有(=) | 3 | 3 | 落 | ✓ |
| lightning-11 | 12 | 1 | 0 | 无→有(0) | 93 | 2 | 暂不 | ✓ |
| lightning-11-nonzero | 0 | 1 | 0 | 有→有(=) | 2 | 2 | 落 | ✓ |
| lightning-45 | 12 | 1 | 0 | 无→有(0) | 93 | 2 | 暂不 | ✓ |
| mabmabe | 2 | 1 | 0 | 有→有(=) | 8 | 2 | 落 | ✓ |
| mabmabe-nondefault | 0 | 1 | 6 | 有→有(0/600/700/1000→0/600/700/1000) | 86 | 86 | 落 | ✓ |
| shield | 8 | 1 | 0 | 有→有(=) | 27 | 3 | 落 | ✓ |
| showhp | 13 | 1 | 0 | 无→有(0/1000) | 86 | 4 | 暂不 | ✓ |
| sinsu | 8 | 1 | 0 | 有→有(=) | 17 | 1 | 落 | ✓ |
| skeleton | 8 | 1 | 0 | 有→有(=) | 17 | 1 | 落 | ✓ |
| skill10 | 8 | 1 | 0 | 有→有(=) | 148 | 28 | 落 | ✓ |
| skill49 | 8 | 1 | 0 | 有→有(=) | 27 | 3 | 落 | ✓ |
| skill51 | 8 | 1 | 0 | 有→有(=) | 27 | 3 | 落 | ✓ |
| skill52 | 8 | 1 | 0 | 有→有(=) | 27 | 3 | 落 | ✓ |
| skill53 | 8 | 1 | 0 | 有→有(=) | 27 | 3 | 落 | ✓ |
| skill54 | 8 | 1 | 0 | 有→有(=) | 27 | 3 | 落 | ✓ |
| snowwind | 8 | 1 | 0 | 有→有(=) | 27 | 3 | 落 | ✓ |
| spacemove | 2 | 1 | 0 | 有→有(0/1000/1016→0/1000) | 10 | 4 | 落 | ✓ |
| tamming | 0 | 1 | 0 | 有→有(=) | 6 | 6 | 落 | ✓ |
| turnundead | 2 | 1 | 0 | 有→有(=) | 34 | 4 | 落 | ✓ |
| unamyounsul | 13 | 1 | 0 | 无→有(0/1000) | 94 | 2 | 落 | ✓ |
| warr-12 | 3 | 0 | 0 | 无→无 | 8 | 4 | 落 | ✓ |
| warr-25 | 3 | 0 | 0 | 无→无 | 8 | 4 | 落 | ✓ |
| warr-26 | 3 | 0 | 0 | 无→无 | 10 | 5 | 落 | ✓ |
| warr-27 | 3 | 0 | 0 | 无→无 | 6 | 3 | 落 | ✓ |
| warr-3 | 3 | 0 | 0 | 无→无 | 6 | 3 | 落 | ✓ |
| warr-34 | 3 | 0 | 0 | 无→无 | 8 | 4 | 落 | ✓ |
| warr-38 | 3 | 0 | 0 | 无→无 | 8 | 4 | 落 | ✓ |
| warr-4 | 3 | 0 | 0 | 无→无 | 6 | 3 | 落 | ✓ |
| warr-43 | 0 | 0 | 0 | 无→无 | 1 | 1 | 落 | ✓ |
| warr-7 | 3 | 0 | 0 | 无→无 | 6 | 3 | 落 | ✓ |
| windtebo | 2 | 1 | 0 | 有→有(=) | 9 | 3 | 落 | ✓ |

**注（warr-43）**：上一轮因 0/0（仅字节格式差）被 excl；本轮 due 恢复 emit 后不再 0/0，已进批（落）。

## 9 支暂不落盘（旧 golden 不动；依赖 B22/B17）

| stem | 类型 | golden 新due | Rust 台账 due | 依赖 |
|---|---|---|---|---|
| fireball | A | 0 | 600 | B22（`CurrentDueMs` one-shot 未设） |
| fireball-5 | A | 0 | 600 | B22 |
| firecharm | A | 0 | 1200 | B22 |
| grouplightening | A | 0 | 600 | B22 |
| lightning-11 | A | 0 | 600 | B22 |
| lightning-45 | A | 0 | 600 | B22 |
| showhp | A | 0 | 1500 | B22 |
| bigcloak | B | 800 | 900 | B17（recog 顺序互换 + 步进差） |
| groupdeding | B | 800 | 900 | B17（+比较器 640 减法溢出） |

## D=12 due 值表（golden 新值 vs Rust 台账；全量门实测）

| stem | golden | rust | 判 |
|---|---|---|---|
| cloak | 1000 | 1000 | PASS |
| holyshield | 1000 | 1000 | PASS |
| unamyounsul | 1000 | 1000 | PASS |
| fireball | 0 | 600 | A |
| fireball-5 | 0 | 600 | A |
| firecharm | 0 | 1200 | A |
| grouplightening | 0 | 600 | A |
| lightning-11 | 0 | 600 | A |
| lightning-45 | 0 | 600 | A |
| showhp | 0 | 1500 | A |
| bigcloak | 800 | 900(+recog互换) | B |
| groupdeding | 800 | 900(+640溢出) | B |

**列义**：S=新增 schema 键数（18 键表）；T=delivery/now 变（虚拟钟后确定）；Y=剥离 schema/time/due 后语义残余（落盘 49 支全 0；`mabmabe-nondefault` Y=6＝CharStatus 单 bit，B17）；due=旧→新；diff±=行尾对齐后 `git diff --no-index --numstat`。
