# openmir2-summon-monster-data-gaps — 召唤怪「名字→DB 行缺失」同族数据缺口

**状态**：事实登记（只读）；两例**同族、分卡**，互相链接（不合并成一张大卡）。
**重立记录**：本文件原提交 `3d2af0b30`（未合分支上，等于不存在）；2026-11-21 照原文重立于 master。
互链：ANGEL 例见 `docs/issues/openmir2-angel-summon-monster-missing.md`；装置缺口总审计见 `docs/issues/device-gap-premise-audit.md`（MG-42 行）。
**同族病根**：召唤走 `Config.<X>` 或按 `Level` 拼出的名字 → 查 `monsters` 表得 race → **无行 ⇒ `GetMonRace=-1` ⇒ `MakeSlave(name)` 返回 null ⇒ 造不出实体**（ANGEL 无回召；SINSU 的 `RecallSlave` 也移不动一个不存在的怪）。

## 例① 精灵（ANGEL / MG-42 / 魔号 41）
- 配置 `M2GameSvr/Server.conf`：`Angel=精灵`；部署 DB `mir2_data.monsters` **无 `精灵`**（仅 `骷髅精灵…`=89，非同种怪，不代用）。
- 解除路径/权限/不得代用 89 或蒙 100 → 见 **[[openmir2-angel-summon-monster-missing]]**。

## 例② 神兽4/5…（SINSU / MG-41 / 魔号 30，`userMagic.Level > 4` 分支）
- oracle `MagMakeSinSuSlave`（`src/M2Server/Magic/MagicManager.cs:1277-1280`）：`if (nExpLevel>4) sMonName = Config.Dragon + (nExpLevel/4 + 4)` ⇒ lv5→"神兽5"、lv8→"神兽6"…
- 部署 DB `monsters`：仅 `神兽(113)/神兽0(81)/神兽1(114)/神兽2(100)/神兽3(100)`，**无 `神兽4/5/6…`** ⇒ 该分支**召唤不出实体**。
- **生产也会发生**：`Level` 能到 5（`CheckMagicLevelUp` 允许 `TrainLv` 高时逐级升）⇒ lv>4 的 SINSU 在生产大概率也造不出怪。
- 另有 `DragonArray` 按玩家等级覆盖 `sMonName`（`:1282-1296`），实际名还取决于 config+玩家等级；但即便不算数组，`Dragon+num` 名本身缺行。
- ⇒ **[[openmir2-sinsu-mg41-plan]] 首批不做「龙名变体」**（该变体=对召唤物 `name` 字段断言，依赖此缺失数据）。

## 待产品/数据侧
- 确认 `精灵`、`神兽4/5+` 是**数据缺失**还是**改名/别名**；补齐 `monsters` 行前，相关召唤分支（ANGEL/41、SINSU 高 Level）**不得升格**。
