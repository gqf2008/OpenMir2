# MG-7 断言强度：钉到表名与列清单（2026-10-24）

> 卡：`openmir2-magic-fixture-assertion-strength`（tier=L）
> 产出：`openmir2-worker-66`（worktree `wt-worker-66`，branch `card/magic-assert-strength`）
> 基线：`walgit/master` = `e115339e3`（含依赖卡 `openmir2-magic-fixture-selftest-gaps` 的 `dbeebdce`）
> 来源：`openmir2-magic-fixture-selftest-gaps` 的 review（reviewer-6）断言强度发现。
> 写集：`tools/conformance/card_window.py`、`tools/conformance/selftest_mutations.py`、本文件。

---

## 0. 问题与选择

**问题**：MG-7 断言 2 只钉 `"VALUES (6228, 2," in sql.write`。把 `_magic_write_sql` 的 INSERT
目标表改成 `characters_magic_BOGUS`（列/值不变）后，magic 桩只按 `VALUES` 正则更新状态、
读回也不区分表 ⇒ `card_window --selftest` 仍 **GREEN 201/201**。**能抓到值，抓不到目标。**

**选择：断言路径（主），不选「桩对未知表/列 fail-closed」**。理由：
1. 卡面 acceptance 第一条明确要求「断言钉到表名 + 列清单」；断言是机判直接可读的证据。
2. 本卡 forbidden 含「改夹具语义」；桩 fail-closed 会改变测试夹具对 SQL 的接受范围（将来合法的
   多行 INSERT / 不同列序会被误拒），属于夹具语义变化，超出本卡范围。
3. 断言已足以让 wrong-table / wrong-column 回归变红（§2 三颗牙），且不改变被测对象。

---

## 1. 改动

### 1.1 `card_window.py`：新增一条 MG-7 强度断言

在 MG-7 原有 4 条断言**之后**追加（原 4 条一条未改、未削弱）：

```python
write_stmt_m7 = ((cs_m7.get("sql") or {}).get("write") or "")
chk(re.search(
        r"INSERT INTO `[^`]+`\.`characters_magic` \(`PlayerId`, `MagicId`, "
        r"`Level`, `UseKey`, `CurrTrain`\) VALUES \(6228, 2, 1, '2', 3\);",
        write_stmt_m7) is not None,
    "MG-7 写入语句钉到表名+列清单：INSERT INTO …`characters_magic`"
    "(PlayerId, MagicId, Level, UseKey, CurrTrain)（实际 %r）" % write_stmt_m7)
```

原样运行时的实际语句（断言命中）：

```
/* magic */ INSERT INTO `mir2_test_db`.`characters_magic`
(`PlayerId`, `MagicId`, `Level`, `UseKey`, `CurrTrain`) VALUES (6228, 2, 1, '2', 3);
```

列名与 DDL 一致（`sql/mir2_db.sql:371912-371920` 的 `characters_magic`：
`PlayerId/MagicId/Level/UseKey/CurrTrain`）；`[^`]+` 允许任意一侧库名（C#/Rust 库名不同）。

### 1.2 `selftest_mutations.py`：把两颗牙固化进 harness

新增 `MAG1`（目标表改 `characters_magic_BOGUS`）与 `MAG2`（列名 `Level` → `LevelX`），
`expect` 锁定新断言文案 `MG-7 写入语句钉到表名+列清单`。两颗牙可复跑：
`python -I -B tools/conformance/selftest_mutations.py --only MAG1,MAG2`。

---

## 2. 三颗牙（真实 rc）

| 牙 | 变异 | `card_window --selftest` rc | 结果 |
| --- | --- | ---: | --- |
| ① | `INSERT INTO … characters_magic_BOGUS` | **1** | RED，新断言 FAIL（见下） |
| ② | 列名 `Level` → `LevelX` | **1** | RED，新断言 FAIL（见下） |
| ③ | 原样 | **0** | **GREEN 202/202** |

① 的 FAIL 原文：

```
FAIL MG-7 写入语句钉到表名+列清单：INSERT INTO …`characters_magic`(PlayerId, MagicId, Level, UseKey, CurrTrain)（实际 "/* magic */ INSERT INTO `mir2_test_db`.`characters_magic_BOGUS` (`PlayerId`, `MagicId`, `Level`, `UseKey`, `CurrTrain`) VALUES (6228, 2, 1, '2', 3);"）
```

② 的 FAIL 原文：

```
FAIL MG-7 写入语句钉到表名+列清单：…（实际 "/* magic */ INSERT INTO `mir2_test_db`.`characters_magic` (`PlayerId`, `MagicId`, `LevelX`, `UseKey`, `CurrTrain`) VALUES (6228, 2, 1, '2', 3);"）
```

**还原**：两次均从备份逐字节还原，`sha256(card_window.py)` 回到
`b153ca3f93b50db4f9f7a2c351ca2c038ba06df03061b995e279a65c92443117`（= 提交树 blob），
原样复跑 **GREEN 202/202**。

### 2.1 既有牙仍成立（未削弱）

| 既有牙 | 变异 | 实测 |
| --- | --- | --- |
| A | `profile_magic` 覆盖分支删掉（只取顶层） | **rc=1，RED 198/202**，MG-7 共 4 条 FAIL（含新强度断言——覆盖失效后 csharp 不写库，`sql.write` 为 None） |
| B | 去掉 `/* magic */` 标记 | **rc=1，RED 201/202**，MG-7 第 4 条 FAIL（magic-columns 日志缺失） |

两次均逐字节还原，sha 与基线一致。

---

## 3. 计数

- `card_window --selftest`：**201/201 → 202/202**（只增不减；新增 1 条断言，原有 4 条不变）。
- `selftest_mutations`：**38 颗牙全红（含新增 MAG1/MAG2）+ 逐字节还原**，rc=0。

---

## 4. 门禁（实跑）

| 门禁 | 结果 |
| --- | --- |
| `python -I -B tools/conformance/card_window.py --selftest` | **GREEN 202/202**，rc=0 |
| `python -I -B tools/conformance/selftest_mutations.py` | **MUTATION HARNESS GREEN: 38/38**，rc=0（baseline/restored sha `b153ca3f…`） |
| `python -I tools/hygiene/conflict_marker_gate.py` | **0 markers**，rc=0 |
| `python -I tools/whitelist/t_reservation_gate.py check` | **GREEN 247/247**（max=T-263），rc=0 |

---

## 5. 边界与残留

- 未改 `src/**`、`rules.json`、`scenarios/*.json`；未起栈/未连库/未占端口。
- 未合并、未自签 review、未删 worktree。**2026-10-24 订正**：本卡已合入 master `51a450c20`（approve reviewer-3 `d2685535`，closed）；上句为交卡时状态。
- 残留：`E:/tmp/worker66-scratch/`（备份、变异输出；仓库外）。
