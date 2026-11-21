# MG-52：魔号 43（`SKILL_43`）的 `ClientSpellXY` 显式 case —— 勘察结论与接线清单

> 状态：**已实现**（2026-11-21，`card/mg-52-56-explicit-cases`）：43 接入 `warrskill`，golden `warr-43.json` 两例（off2on/on2off）对拍通过。A 类小叶子。
> 分支：`card/mg-52-card`（docs-only）；base `master = b480336f9`。

## 1. oracle 事实：不是"零行为"

- **`src/M2Server/Magic/MagicManager.cs:555-556` 的 `case MagicConst.SKILL_43:` 确实是空体**（`break;`，零抽、零帧）——
  但它的**可达路径不在这里**。
- 施法走 `src/M2Server/Player/PlayObject.Attack.cs:1253-1266` 的 `ClientSpellXY` **显式 case**（`case MagicConst.SKILL_43`，
  注释「破空剑」）：当 `MagicArr[SKILL_43] != null` 时按当前位**翻转**并各发一条：

  | 分支 | 行为 |
  | --- | --- |
  | `!MBo43Kill` | `Skill43OnOff(true)` ⇒ `SysMsg("开启破空剑", Green, Hint)` + `SendSocket("+CID")` |
  | 否则 | `Skill43OnOff(false)` ⇒ `SysMsg("关闭破空剑", Green, Hint)` + `SendSocket("+UCID")` |

- `Skill43OnOff`（`src/M2Server/Player/PlayObject.Base.cs:640-651`）除翻转 `MBo43Kill` 外只发上述绿字：
  **`SysMsg(..., MsgColor.Green, MsgType.Hint)` ⇒ `RM_SYSMESSAGE`(252) ⇒ `SM_SYSMESSAGE(100)` 一帧**。
- 该 case `result = true`，**不扣 MP、不发 `HealthSpellChanged`(53)、不调 `DoSpell`**（⇒ 无 638/训练尾/距离门）。
- `MBo43Kill` 除 `IPlayerActor` 的接口声明外**全仓无消费者**（`src/` grep）⇒ 翻转位本身惰性；
  **可观测面 = 1× `SM_SYSMESSAGE(100)` + 1× 短包（`+CID` 或 `+UCID`，toggle 交替）**。

⇒ 结论：**"零行为"不成立**；准确说法是"**零魔法效果**，但有 1 条系统消息帧 + 1 条动作短包"。

## 2. 短包是"线上帧"，且装置已记录

`SendSocket(string)`（`src/M2Server/Player/PlayObject.Socket.cs:48-70`）不是裸 socket 文本：它构造
**20B `ServerMessage` 头**（`Ident = Grobal2.GM_DATA`、`PackLength = -len`），尾部直接放 ASCII 正文
（`"+CID"` / `"+UCID"`），再经 `M2Share.NetChannel.AddGateBuffer` 下行。

装置侧（`mir2-rs/tests/parity/csharp/MagicGolden/Program.cs` 与 `ClientSpellXYEntry.cs` 的同口径
`ParseFrames`）对 `PackLength < 0` 分支**已经记录**为 `{ ident = 0, tail = "+CID" }`；
`RecordingNetChannel` 把三处 `SendSocket` 重载统一汇到同一 `AddGateBuffer`。

⇒ **装置零增量**。

## 3. Rust 侧实现清单（已按此落地，逐项）

1. **会话位 `MBo43Kill`**（默认 `false`；toggle 语义；存档面 oracle 未落库 ⇒ 只做会话态）。
2. **`SM_SYSMESSAGE(100)` 两段文案**："开启破空剑" / "关闭破空剑"（Green/Hint；文本取自 oracle 原文，
   不依赖常量自证）。
3. **短包下行**：`GM_DATA` Ident + **负 `PackLength`** 的 20B 头 + ASCII `"+CID"` / `"+UCID"`。
4. **负例断言（易回归面）**：
   - 显式 case **不扣 MP**、**不发 `SM_HEALTHSPELLCHANGED(53)`**、**不调 `DoSpell`**（无 638/训练尾）；
   - 显式 case **不执行 `default` 首句** `Dir = GetNextDirection(...)`（既有 `takes_default_dir` /
     `EXPLICIT_CASE_MAG_IDX` 已按此建模，新接线不得回退）；
   - toggle 交替：第一次 `+CID`/绿字"开启"，第二次 `+UCID`/绿字"关闭"。
5. `magicwire.rs` 的 8-case 表格 43 行已改为「已接 + `Arm::Skill43` + `MagicState::mbo43_kill`」
   （56 行改注「不适用：oracle `MagicArr[56]` 越界」，见 `openmir2-magic-56-oracle-index-out-of-range`）。
6. **源码默认文案记进 Fixture**：golden 的 `fixture_spec.toggle_text_on/off` 记录生成时的
   `MessageSettings.RedHalfMoonOn/Off`（"开启破空剑"/"关闭破空剑"）；比较器 `warr_43_matches_golden`
   断言 Fixture 值 == 字面量，并逐帧比对 100 的尾串，防默认串被改而 golden 静默漂移。

## 4. 为什么值得验

它能钉住「**显式 case 不执行 `default` 支的 `Dir`/MP/53**」这条**易回归的路由分支**——这是今天在
9/10 的站位与 49–54 的批里都**未覆盖**的一类；代价极小（1 帧 100 + 1 条短包 + 负例）。

## 5. 排期与依赖

- **A 类小叶子**（装置零改动、判据清晰），**排在有效果的技能之后**。
- 当前 `magicwire.rs` 与 `lib.rs` 有两条线在改（`c6` 的 MG-18、`7d` 的施毒批）⇒ **本卡只出结论，不动代码**。
