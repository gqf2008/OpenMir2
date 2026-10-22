# 说词变量展开：r7「`<$GAMEGOLD>` 原样显示」复核结论（2026-10-22）

**结论一句话**：r7 真机在赌场 `@main` 页看到的 `您当前的元宝为:<$GAMEGOLD>个` **是 oracle 行为**，
不是 Rust 缺陷。卡 `openmir2-say-variable-expansion` 按「不修复」收口；`line_variable_text`
保持 oracle 的字面循环（**扫描不前进**），只补两处把该语义钉死的用例。

## 1. oracle 证据（`src/`，部署件同源）

| 事实 | 位置 |
| --- | --- |
| 说词路径调用 `normNpc.GetLineVariableText(playerActor, sMsg)` | `src/Modules/ScriptEngine/ScriptEngine.cs:989`（`GotoLableSendMerChantSayMsg`） |
| 行公告同走这一支 | `src/GameSrv/Word/WorldServer.cs:674` |
| 循环体：每轮 `HUtil32.ArrestStringEx(sMsg, "<", ">", ref sVariable)`，**返回值丢弃** | `src/M2Server/Npc/NormNpc.cs:245-273` |
| `ArrestStringEx` 明明是「截取 + 返回剩余串」 | `src/OpenMir2/HUtil32.cs:488-545` |
| 仅 `sVariable[0]=='$'` 时 `GetVariableText(..., ref sMsg)` 会**改写 sMsg** | `NormNpc.cs:255-262` |
| `switch (sVariable)` 大小写**敏感**（ordinal） | `NormNpc.cs:281-` |
| `ReplaceVariableText` 用 `IndexOf(..., OrdinalIgnoreCase)`，**只替换第一处** | `NormNpc.cs:1618-1633` |
| 对照：**动作参数**走 `ProcessingBase.GetLineVariableText`，那里 `tempstr = ArrestStringEx(...)` 才是前进扫描 | `src/Modules/ScriptEngine/ProcessingBase.cs:1438-1459` |

`NormNpc` 的循环没有把 `ArrestStringEx` 的返回串赋回 `sMsg` ⇒ **扫描不前进**。唯一的推进来自
`ReplaceVariableText`「把首个 `<...>` 顶掉」。所以：

- 消息里 `<...>` token 的**前缀**全是已实现的 `$` 变量 ⇒ 逐个展开（隐式前进）；
- 一旦首个 `<...>` 是非 `$` token（`<开始-下注/@kaiz>`、`<返回/@main>` 这类链接），
  循环原地空转 101 轮后返回 ⇒ **后面所有变量原样上线**。

赌场 `@main` 页（`Envir/Market_Def/赌场-B013.txt:5`）首 token 就是链接，因此 `<$GAMEGOLD>` 原样——
r7 观察到的正是 oracle 语义。

## 2. 语义推演（已写成用例，`bins/game-svr/src/npc.rs`）

| 输入 | 输出 | 原因 |
| --- | --- | --- |
| `<进入/@main>\ \ 玩家:<$USERNAME> 元宝:<$GAMEGOLD>` | 原样 | 首 token 是链接 ⇒ 空转 101 轮 |
| `<$username>|<$USERNAME>` | 原样 | 首 token 大小写不命中（`switch` 敏感）⇒ 无替换 ⇒ 卡住 |
| `<$USERNAME><$GAMEGOLD>` | `张三0` | 前缀连续变量 ⇒ 每次替换顶掉首 token |
| `<$USERNAME><返回/@main><$LEVEL>` | `张三<返回/@main><$LEVEL>` | 展开到第一个非 `$` token 为止 |

用例 `line_variable_text_stops_at_first_non_variable_token` 与
`say_variables_expand_through_merchant_goto_gbk_and_edcode`（生产入口 `MerchantScriptBook::goto_label`
→ `merchant_say_gbk` → `smframe::sm_merchantsay`，逐字节断言 cp936 原文与 `EDCode.EncodeString` 尾）。

## 3. 独立复核与处置

- 独立 verifier 对**首版实现**（把循环改成经典 `TNormNpc.GotoLable_SendMerChantSayMsg` 的前进扫描、
  登记 whitelist T-157）给 **REQUEST_CHANGES / tier=R**：改动改变 `SM_MERCHANTSAY(643)` 线上字节，
  而 oracle（现行 `src/`）不前进；真机只有 Rust 侧截图，无 C# 对照。
- **采纳**：按 AGENTS.md「行为等价优先于修 bug」，撤销 production 改动（`line_variable_text`
  与 master 逐字节相同），删除 whitelist T-157，改补两处钉住 oracle 语义的用例。本卡由此从
  「R 级行为变更」降为 **tier=L（仅测试 + 文档，生产代码零差异）**。

## 4. 牙（真跑，先红后逐字节还原）

| 牙 | 红检 |
| --- | --- |
| 循环改回**前进扫描**（`rest = next`，经典路径写法） | 两用例全红：`left "<开始-下注/@kaiz>\ \ 您当前的元宝为:4242个"` / `"<进入/@main>… 玩家:张三 元宝:0"` |
| 删 `$GAMEGOLD` 分支（改臂名使其落 `_`） | 生产用例红：`left "…元宝:<$GAMEGOLD>…"` |
| 还原 | `cargo fmt` 后 sha256 `6876ddca53d96a0059c5fa6069c1b95335660ec4fbbfe5c0302a31cc84d33656`（`.bak` 逐字节比对一致） |

## 5. 残余

- C# 同场景真机对照（可选、非阻塞）：赌场点击抓 643 看 `<$GAMEGOLD>` 是否同样原样；本结论由源码
  直接可判，且 r7 Rust 侧观察与推演逐字一致。
- `$STR(...)` 之外的大量变量面（HP/装备/行会/城堡/日期墙钟等）仍在 `get_variable_text` 的
  `_` 分支 ⇒ 原样保留，与 oracle 无分支同形；接它们属另卡（R 级数值面）。
