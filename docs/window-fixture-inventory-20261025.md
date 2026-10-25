# 窗口夹具：背包替换/丢弃（`fixtures.inventory_replace` / `fixtures.inventory_drop`）

卡：`openmir2-window-fixture-inventory`（tier=L，工具面）。分支：`card/window-fixture-inventory`。
日期：2026-10-25。写集：`tools/conformance/card_window.py` + 本文件。

---

## 1. 为什么需要它（缺口）

基线角色 `winplay0` 的 `characters_bagitem` **46/46 槽全占**（每个 `Position` 0..45
都有一行 `StdIndex=0` 的空壳行），且**没有武器**。既有场景顶层 `inventory` 通道
（`scenario_inventory` / `run_inventory`）要求「声明槽位原本为空」（占用行数非 0 直接
fail-closed）⇒ 在当前基线上**写不进去**；而 `restore-baseline*.sh` 会把删掉的行恢复回来。

后果（dev-1 在 `openmir2-butch-two-draws-unblock` 的 butch 实测）：无法声明「装武器
（`CM_TAKEONITEM`）」前置 ⇒ 空手时 `characters_ablity.Dc=0` ⇒ 对 AC=0 的动物伤害恒 0
⇒ `SM_STRUCK` 不发、击杀链断。

本卡给夹具加**替换/丢弃**语义：不改基线、不要求空槽，而是「声明某槽要变成什么」，
跑完全部整行还原。

## 2. 声明形态

两键都挂在**既有 `fixtures`** 下（与 `level` 同款；`sides.<side>.fixtures` 可整体覆盖）：

```json
"fixtures": {
  "inventory_replace": [
    {"position": 40, "index": 207, "make_index": 5001,
     "std_mode": 5, "ani_count": 0, "dura": 10, "dura_max": 20,
     "note": "木剑（装武器前置）"}
  ],
  "inventory_drop": [42, {"position": 43, "note": "清空两格"}]
}
```

- `inventory_replace[]`：必填 `position`（0..45）、`index`（= `StdIndex`，必须是
  `mir2_data.stditems.Id` 里存在的模板）、`make_index`（>0，两侧必须相同）；
  可选 `std_mode` / `ani_count`（与模板 `StdMode` / `Shape` 交叉核对）、`dura` /
  `dura_max`（缺省 0）、`note`。**未知键一律拒**。
- `inventory_drop[]`：整数 `position` 或 `{position, note}`；该槽**必须原本有行**，
  否则 rc=2（见 §4）。
- 两键的 `position` 集合**不许重叠**，各自不许重复。
- 与场景顶层 `inventory`（要求空槽）是两条互不影响的通道：既有的 21 条场景不受影响；
  新场景要「覆盖/清空已有槽」时用本通道。

## 3. 运行口径

流水线位置与 `level` 完全一致：**restore 之后、起栈之前**写 DB；**停栈之后**还原。

1. 解析角色（`ChrName`+`LoginID` 唯一、未删除；库名必须是测试库；与部署 `Server.conf`
   的 `[DataBase] database` 交叉核对）。
2. 读**未声明槽位**的行数 + MD5 指纹（`inventory_fingerprint_sql`，排除声明槽位）。
3. 逐槽写前**整行快照**：`Id / Position / MakeIndex / StdIndex / Dura / DuraMax`。
4. **先把全部前置校验做完**（模板存在、`std_mode`/`ani_count` 匹配、drop 槽非空），
   一个字节都不写；任一项不过 ⇒ rc=2。
5. 逐槽 `DELETE`（`replace` 再 `INSERT`），写后**按槽读回**断言字段逐项相等
   （`drop` 断言该槽无行）。
6. 写完再读指纹：未声明槽位的**行数与指纹都必须逐字节不变**。
7. 停栈后 `restore_inventory_fixture`：逐槽 `DELETE` + 按快照**连 `Id`** `INSERT`，
   再读回逐字段比对；失败该侧 rc=2（不静默跳过）。

`--dry-run`：只打印预览（`replace Position=.. StdIndex=.. MakeIndex=..` /
`drop Position=..`）并把 SQL 记进证据，**不连库**。

证据落盘：每侧 `inventory_fixture.json`（`targets[]` = 每槽 `before`/`after` 行、
`fingerprint.before/after`、`sql`、`action`、`error`、`restore_error`）。凭据只存内存
`_session`，落盘经 `level_evidence()` 过滤（自测 INV5 用 `SECRETPW` 证明过滤有效）。

## 4. fail-closed 面（全部 rc=2，不静默跳过）

| 阶段 | 条件 | 表现 |
|---|---|---|
| 声明期 | 空数组/非数组、缺必填键、未知键、非整数、越界（position≥46）、`make_index<=0`、同槽重复、replace 与 drop 重叠 | `ConfigError`（加载即失败） |
| 声明期 | 两侧 `inventory_replace`/`inventory_drop` 不同 | `_check_fixture_symmetry` 报「不一致」（对拍无效） |
| 运行期 | 角色不存在/不唯一/已删除、库名不是测试库、库配对失败 | rc=2 |
| 运行期 | 模板不存在、声明 `std_mode`/`ani_count` 与模板 `StdMode`/`Shape` 不符 | rc=2，且**未发生任何写** |
| 运行期 | `drop` 的槽本来为空 | rc=2（夹具没生效，不许假绿） |
| 运行期 | 写后读回不符 | rc=2 + 已写项**就地回滚**（回滚失败单独记 `rollback_error`） |
| 运行期 | 未声明槽位的行数或指纹变了 | rc=2（动到了未声明的行） |
| 还原期 | 还原 SQL 失败或还原后行 ≠ 写前行 | 该侧 rc=2，`inventory_fixture.restore_error` 点名槽位 |

## 5. 怎么让「装武器打怪」类场景可声明

- 在 profile 的 `fixtures` 里声明「把第 N 格替换成武器」（示例见 §2），场景本身只写
  `CM_TAKEONITEM` 步；跑完背包整行还原，基线不动。
- 因为走的是既有 `fixtures` 对称校验，两侧声明必须逐字节相同 ⇒ 两侧拿到的
  `MakeIndex`/`StdIndex`/耐久一致，`value_mismatch` 不会来自夹具本身。
- 场景侧无需改 `mir2_probe.py`：声明在 profile，不在场景顶层键白名单里。

## 6. 自测（构造级，382/382 绿）

`python -I -B tools/conformance/card_window.py --selftest`（既有 366 + 本卡 16 条 = 382）：

```
PASS INV1 声明解析：replace 2 件 + drop 2 槽（int 与对象两种写法）
PASS INV 牙1 空数组/缺键/未知键/越界/make_index=0/同槽重复/replace 与 drop 重叠 => ConfigError
PASS INV 牙2 两侧 inventory_replace 不同 => ConfigError（对拍无效）
PASS INV2b 两侧声明相同 => 通过
PASS INV3 SQL 形态：replace=DELETE+INSERT，drop 只 DELETE，还原连 Id 写回
PASS INV 牙3 行读回列数不对/同槽多行/NULL => ConfigError
PASS INV4 dry-run：打印 replace/drop 预览且不执行任何 mysql
PASS INV5 生产路径：40/41 被替换、42 被 drop；证据不含凭据（_session 只在内存）
PASS INV6 还原：按快照整行写回（含 Id），背包与写前逐字段相同
PASS INV7 drop 单槽：写后该槽为空，还原后整行（含 Id）复原
PASS INV 牙4 写后读回不符 => rc=2，且已写项就地回滚到写前
PASS INV 牙5 其它槽位指纹变化 => rc=2（动到了未声明的行）
PASS INV 牙6 drop 空槽 => rc=2（夹具没生效，不许假绿）
PASS INV 牙7 模板不存在 => rc=2，且一个写语句都没发（先校验后写）
PASS INV 牙8 声明 std_mode 与模板不符 => rc=2
PASS INV8 接线：run_side 起栈前应用、finally 停栈后还原
```

INV5–INV7 用有状态假栈模拟 `characters_bagitem`（46 行），覆盖写、读回、drop、
整行还原；不是真库。

## 7. 真机往返（acceptance ④，**待做**）

卡面要求「真实测试库往返：替换一格 → SELECT 断言 → 还原 → SELECT 复原」。当前**按
Leader 指令暂缓**：castle 升级腿的受控重跑尚未进行，**在它之前不得动测试库**
（`mir2_test_db` / `mir2_test_db_r` 必须保持 retry 脚本种子之外的原状）。

受控重跑完成后立即补做（计划命令，结果回填本节）：

1. 记录 `winplay0` 背包 46 行快照（`SELECT Position,Id,MakeIndex,StdIndex,Dura,DuraMax`）；
2. 用 dry-run 校验声明：`--profile <path>.json --dry-run`，确认预览行；
3. 真跑一次**只做夹具**的最小窗口（或等下一次真机窗口顺带），写后 `SELECT` 断言
   40 槽 = 声明值、42 槽无行；
4. 停栈后核对还原：46 行逐字段与步骤 1 相同（含 `Id`）、其它槽指纹不变；
5. 证据（命令 + 输出 + 路径 + sha256）回填本节并挂到卡线程 comment。

在证据补齐前，本卡**不声称**「真机往返已验证」。

## 8. 边界与残留

- 只写**测试库**的角色背包表；不碰生产库、不碰 `E:/MirServer` 内容（内容冻结）。
- 不新增场景、不改 `run3-scenarios.json`；要进批需另卡（`fixtures` 由 profile 携带）。
- `inventory_drop` 空槽报错是有意为之（与 `inventory` 的「空槽才能写」互补）；
  若将来需要「空了也算过」的语义，必须另加显式开关，不得放松默认。
- 停栈后还原依赖角色行仍在（`player_id` 由起栈前解析并保存在内存证据里）；若跑中角色
  被删，还原会失败并 rc=2——这是期望行为。
