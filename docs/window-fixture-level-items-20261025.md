# 窗口夹具：角色等级声明（level）与道具声明（items）裁定项（2026-10-25）

- 卡：`openmir2-window-fixture-level-items`（tier **L**，P1；owner `openmir2-tester-1`，
  worktree `.worktrees/tester-1`，branch `card/window-fixture-level-items`）。
- 来源：`openmir2-clearmapmon-t118-unblock` 的判定②——t118 的 `@rw01` 需要
  `checkitem 佛牌 1` + `checklevel 43`，而三库无佛牌、`winplay0` 等级 1。
- 写集：`tools/conformance/card_window.py`、本文档。

## 1. `fixtures.level`（已实现）

```json
"fixtures": {"level": 43, "note": "t118 前置 checklevel 43"}
```

- **语义**：起栈前把**本侧测试库** `characters.Level` 设为声明值（1..255）；等级是 DB 侧状态，
  **不写 Envir**，因此不走 `_fixture_envir_roots` 的影子双写。
- **执行点**：`run_side` 在 restore / placement / assets / Envir 夹具之后、端口检查与起栈之前
  （`run_level_fixture`）。
- **安全面（fail-closed）**：
  1. 写前按 `ChrName+LoginID` 定位唯一未删行，记录 `Id`/`Level` 现值与**其余列 MD5 指纹**；
  2. `UPDATE characters SET Level=? WHERE Id=?`，影响行数必须为 1；
  3. 写后**读回**：`Level == 目标` 且其它列指纹不变，否则 rc=2；
  4. 停栈后 `restore_level_fixture` 把 `Level` 写回写前值并**重读**校验；失败该侧 **rc=2**
     （DB 留在被改状态会污染下一轮）。
- **对称性**：`level` 纳入 `_fixture_symmetry_key` ⇒ 两侧声明必须同值（`sides.*.fixtures` 覆盖也
  要带同一份；不同值 ⇒ 加载期 ConfigError，因为对拍无效）。顶层 `fixtures` 默认应用于两侧。
- **dry-run**：只打印 `等级夹具（dry-run 不连库）：db=… UPDATE characters SET Level=43 WHERE Id=<row> …`，
  **不执行任何 mysql**。
- **取值**：整数 1..255（`characters.Level` 为 `int(11)`，部署实测最大 255）；0/256/负数/bool/
  字符串/浮点/null 一律 ConfigError；未知键仍 fail-closed。

## 2. `fixtures.items`（**不实现，交 owner 裁定**）

- 需求来自 t118 的 `checkitem 佛牌 1`。实测：`mir2_data` / `mir2_test_data` / `mir2_test_data_r`
  三库 `stditems` **均无「佛牌」**（`LIKE '%佛牌%'` 0 行；标准包也只有 `泰轮佛尘`）。
- 要让背包里出现「佛牌」，必须先**新增 stditems 定义**——这直接触碰**内容冻结**（三条硬约束之一）。
  ⇒ 本卡**不实现 items 夹具**，不写入任何 stditems 行；登记为 owner 裁定项：
  1. **甲**：允许在**测试库**（`mir2_test_data*`）新增任务道具定义（生产 `mir2_data` 不动）；
  2. **乙**：改 `武士-0114` 脚本前置（例如改用现有道具），属 oracle 行为变更，需另立 R 卡；
  3. **丙**：维持 t118 永久不可达（当前结论），W-CM-1 走替代判据（台账第 4 行）。
- 现有 `inventory` 夹具只能引用**已存在**的 `stditems` 行；在无定义时写入背包行没有可用
  `StdIndex`，`checkitem` 也不可能命中。

## 3. 与 t118 / W-CM-1 的关系

- **仅有 `level` 仍不能解封 t118**：`@main` 还有 `checkitem 佛牌 1`，而佛牌定义不存在 ⇒
  `@rw01` 仍不可达（另见同 seed 跨起栈 `movr` 漂移）。
- ⇒ W-CM-1 的关卡证据仍按台账第 4 行的三段替代判据：**刷怪 = q001 真机 #19；清场过滤与
  宠物/召唤物 = 仅构造级**。`level` 夹具的价值是把「等级前置」从手工改成可声明，供其它需要
  高等级角色的场景使用。

## 4. 实测

```text
$ python -I -B tools/conformance/card_window.py --selftest
PASS LEVEL1 声明 level=43 => 解析透传（note 不参与）
PASS LEVEL1b 边界 1 与 255 放行
PASS LEVEL 牙1 level 0/256/负数/bool/字符串/浮点/null => ConfigError 点名 1..255
PASS LEVEL 牙2 未知键 levels => ConfigError 点名
PASS LEVEL 牙3 两侧 level 不同 => ConfigError（对拍无效）
PASS LEVEL2 dry-run：打印 UPDATE 预览且不执行任何 mysql
PASS LEVEL3 生产路径：写前 1 -> 写 43 -> 读回 43；停栈后还原并重读为 1
PASS LEVEL3b 已是目标等级 => noop（不打 UPDATE）
PASS LEVEL 牙4 写后读回不符 => rc=2
PASS LEVEL 牙5 还原后重读不符 => 返回错误（调用方升级 rc=2）
PASS LEVEL4 接线：run_side 起栈前应用 level，finally 还原
GREEN: card_window.py selftest 360/360
```

真实测试库往返（两侧各做一次，只读旁证）：

```text
csharp: APPLY rc=0 action=updated before=1 after=43；DB_LEVEL_AFTER_APPLY=43；
        RESTORE err=None；DB_LEVEL_AFTER_RESTORE=1
rust  : APPLY rc=0 action=updated before=1 after=43；RESTORE err=None
```

## 5. 门禁

`card_window --selftest` 360/360；7 项卫生门禁（`stale_pointer --ref master`）+
`git diff --check <base> <head>` + `merge_tree_helper` 见 patch body。未起正式服务端；未改
`E:/MirServer`；未 push。

## 6. 残留

- 测试库 `winplay0` 的 `Level` 已还原为 1（两侧）。
- `items` 侧无任何写入；等待 owner 在 §2 的甲/乙/丙中裁定。
