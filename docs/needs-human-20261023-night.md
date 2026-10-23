# 需要 owner 决策/授权的事项（2026-10-23 深夜收口）

> 快照：截至 `ee292359b` @ 2026-10-23 23:1x（+0800）。动态数字引用时**必须带这个戳**。
> 本文件只列**循环内无权自决**的事；已自决的写在 `docs/leader-queue-20261023.md` 与各卡 status 里。

## 1. 真机窗口（唯一能推进条件② 的钥匙）

- **需要授权**：起 C# 私有根 + Rust 双栈跑窗口（`E:/tmp/conformance`，alt 剖面 15xxx/17xxx）。
  循环硬约束是「不起服务端、不占端口」，所以窗口只能由被授权的车道/人跑。
- **现在比以前好跑**：
  - `docs/window-runbook-20261023.md` 已按当前工具全面订正（`49644498f`），并新增
    **§0.5「开窗前 60 秒自查清单」**（核二进制 sha → 核 alt 端口 → 核 profile 键 → `--dry-run` → `--selftest`）。
  - `start_mir2_server.ps1 -WhatIf` 现在会打印**绝对路径 + sha256 + 构建类型**，release 缺失
    不再静默退 debug（`821492abf`）；建议跑窗口时加 `-RequireRelease`。
  - BLOCKED 场景在工具层被硬拦（`8dac04eb0` + `2a69cd6e0`）：`card_window` 见到 `blocked` 即 rc=2、
    不建 out、不起栈。
- **窗口跑不了的三类**（不是「欠窗口」，是「现在跑也跑不出来」，**别混进排期估算**）：
  1. `combat_strike` 覆盖的 **R3 13 张**：全库无可行配对（最好 joint 24%），且守卫挨打也产
     `SM_STRUCK(31)`（`recog=self`）⇒ 跑绿 ≠ 玩家命中。
  2. `castle_zone` 覆盖的 **4 张**：申请链 `NotImplementedException`（T-38）、GM 入口 perm 10、
     `AttackSabukWall.txt` 空 Δ；解封需 owner 裁决「`StartCastlewarTime` / DB 双行会 /
     私有根 `AttackSabukWall.txt` / 多会话探针」三项。
  3. `spell_cast` 覆盖的 **R5 8 张**：缺 `characters_magic` 夹具（卡 `openmir2-window-magic-fixture` 在办）
     **且** Rust 侧魔法实现未完成。

## 2. `rules.json` 的 43 条 `PENDING`（`approved_by` 只能 owner/coordinator 签）

- 审计件：`docs/rules-pending-approval-pack-20261023.md`（`4e0bfd2b8`）。分级 **33 可批 / 10 需补证据**。
- **ident 14（`ignore`）单独需要 owner 裁定**：reviewer-2 三份签名条目明写「本 approve 不构成授权」；
  卡 `castle-merchant-*` 虽已合并，但 **R 级窗口未跑**——「合并算不算落地」两种读法结论不同。
- **批准不是改个字段**：实测把 43 条改成已批准 ⇒ `compare_runs selftest` 从 198 变 **176/198 rc=1**
  （22 条断言写死 PENDING）⇒ 批准提交必须同步改断言，否则门禁假红。
- 今晚已修的两类相关缺陷（供决策参考）：**7 条规则被全局 `recog` 掩码吞掉语义字段**
  （`213`/`766` 曾「什么都不比」，`b42f20a8`）；**缺 202/203/642/709 四条**（`6c2da8bf`）。

## 3. 三张卡需要「多会话」或 owner 批准替代判据

- `run-gap-checkdupobj`（3 人同格推挤）、`disappear-recipients-gridscan`（±12 观察者）
  ⇒ 单 probe 构造不出；已各发 comment 请裁决。
- `enter-map-cell-rejected-vs-oracle` ⇒ **真机不可达**（`MapInfo.txt` 1560 条 route 中 1556 可走、
  4 条缺图在 `CellRejected` 之前返回 `NoSuchMap`）⇒ 建议 owner 批准「构造级替代」并 close。

## 4. whitelist `T-161..T-222` 的 owner 逐条复核（旧账）

- 这批登记的作者多为 worker，末栏写「待独立 principal 复核」；按 §1 条件② 的定义需要
  「显式登记 + owner 复核」。今晚新增的 T-225..T-243 已是新口径，但旧账仍差 owner 一句话。

## 5. 两条 Rust 侧接线缺口（不是 owner 的事，列在此以便排期）

- `@upgradenow` / `@getbackupgnow` 只 `glog` 不执行（`lib.rs:5849-5855` / `5890-5895`）
  ⇒ `upgrade_retrieve` 场景的窗口在修复前会「Rust 缺 643」。
- `DoDamageWeapon` / `SM_DURACHANGE(642)` 未接（`combat.rs:1151-1153` 抽了 `Random(5)` 但 `:1266` 丢弃）
  ⇒ `equip_struck_save` 场景的窗口会「Rust 缺 642」。
- 这两条都已在对应卡的 status/docs 登记「**修复前不得据此判 Rust 失败**」。

## 6. 私有 C# 栈保留

- `E:/tmp/conformance/csharp-root` 仍在使用（`mir2_test_*` 库）；请确认是否长期保留。
