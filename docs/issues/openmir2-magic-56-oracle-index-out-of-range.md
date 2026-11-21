# 魔号 56（`SKILL_REDBANWOL`）：oracle 侧是 `MagicArr` 越界崩溃，不是可对拍的帧叶子

> 状态：**只读结论卡**（2026-11-21）。**不新增清单条目、不动分母**（56 从来不在分母里；
> 加进去等于改口径，见 `metric-semantics-must-not-shift`）。是否把 55/56 纳入清单另议。

## 1. 事实链（全部 oracle 原文 + 数据）

1. `MagicArr = new UserMagic[50]` —— `src/M2Server/Player/PlayObject.cs:1041`。
2. `RecalcHitSpeed` 对 `MagicList` 逐条写入 `MagicArr[userMagic.MagIdx] = userMagic`
   —— `src/M2Server/Player/PlayObject.cs:3684`，**无边界检查**（写越界）。
3. 魔号 56 在数据里**存在**：`sql/mir2_data.sql:124` 的 `magics` 行
   `(56, 56, '异形换位', 2, 19, …)`。
   ⇒ 角色**学会 56 的那一刻**（`ReadBook`/`SendAddMagic` 后紧接 `RecalcAbilitys()`）即抛
   `IndexOutOfRangeException`。
4. `ClientSpellXY` 的 `case MagicConst.SKILL_REDBANWOL`（`src/M2Server/Player/PlayObject.Attack.cs:1153`
   是 `case` 标签）在 `src/M2Server/Player/PlayObject.Attack.cs:1154` 先**读**
   `MagicArr[SKILL_REDBANWOL] != null` ⇒ **即使绕过学会路径**，施法同样**读**越界（抛同样的异常）。
5. `src/M2Server/Magic/MagicManager.cs` 里的 `SKILL_43` / `SKILL_REDBANWOL` 分支与本 case **是两处 switch**，不可互替。

**结论**：56 的 oracle 语义是**崩溃**，不是"发 `SM_SYSMESSAGE(100)` + `+WID/+UWID` 短包"。
⇒ 它**不能**作为帧对拍叶子；对它的"期望"要么是异常，要么是"不崩"，两者都不是帧比较能覆盖的。

## 2. 实测证据（装置侧复现）

生成 56 golden 时（`dotnet run --project mir2-rs/tests/parity/csharp/MagicGolden -c Release --
--case warrskill --out-dir …`，装置 `ClientSpellXYEntry.RunOne` 按 oracle 形状 `caster.MagicArr[spec.MagicId] = um`）：

```
Unhandled exception. System.IndexOutOfRangeException: Index was outside the bounds of the array.
   at MagicGolden.ClientSpellXYEntry.RunOne(...) in ...ClientSpellXYEntry.cs:line 347
```

——这不是装置 bug，是照抄 oracle 撞到真边界。

## 3. Rust 侧现状（有意保留的偏差，不是遗漏）

`mir2-rs/bins/game-svr/src/skill_ability.rs` 的 `RecalcHitSpeed` 对应实现对
`MagIdx >= MAGIC_ARR_LEN(50)` 是**跳过 + WRN 登记**（"oracle `MagicArr` 越界；本仓跳过并登记"）。
⇒ 本仓**有意**复刻"不崩"，不抛异常；`MagicArr[56]` 恒 `None`。

因此：
- 该偏差**写明为有意保留**（防御式降级），不是待修缺口；
- 任何"56 的帧叶子"都不该按 oracle 的 100+短包写期望（oracle 根本到不了那里）；
- `magicwire.rs` 的 8-case 表格里 56 行标注 **不适用（崩溃路径）**，本卡为引。

## 4. 影响面与建议

- **不排对拍批**；不新增清单条目、不动分母。
- 若将来要验"56 的降级行为"，应另立**防御性/崩溃复刻**卡：断言"Rust 不崩 + 记 WRN +
  `MagicArr[56]` 保持 `None`"，而不是帧对拍。
- 与 43（MG-52）无关：43 < 50，在界内，照常做帧叶子（卡
  `docs/issues/magic-52-clientxy-explicit-case.md`）。
