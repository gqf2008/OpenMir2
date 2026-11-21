# ANGEL（41 精灵）召唤怪「精灵」在生产数据中缺失

> 状态：**登记（数据缺口）**——2026-11-21 重立（原卡只存在于未合分支）。
> 依据：`docs/issues/device-gap-premise-audit.md` 的 MG-42 行（本卡是它的「唯一真阻塞点」展开）。
> 基线 master `2414209c3`；本卡不改代码。

## 1. 区分两件事（实现批最容易混）

| 面 | 结论 | 依据 |
| --- | --- | --- |
| **harness 能不能出 golden** | **能**（不阻塞） | `mir2-rs/tests/parity/csharp/MagicGolden/OfflineWorldEngine.cs` 的 `RegenMonsterByName` 是离线替身，按名字造一个 `MonsterObject`（17/30 同法；造怪抽数**已记录在 golden `Rng` 中且可复现**——skeleton/angel 的驱动不做 RNG 记账，sinsu 已记账且前 5 抽逐值相同） |
| **生产里 41 能不能召唤出东西** | **不能（恒失败）** | 见 §2 |

## 2. 生产链为什么恒失败

1. `MagMakeAngelSlave`（`src/M2Server/Magic/MagicManager.cs:1343-1354`）调
   `playObject.MakeSlave(Config.Angel, …)`；`Config.Angel` 缺省 **`精灵`**
   （`src/Modules/SystemModule/Conf/Model/GameSvrConf.cs` 的 ctor）。
2. `MakeSlave`（`BaseObject.cs:1180-1200`）→ `RegenMonsterByName`（`WorldServer.MonGen.cs:382`）
   → `int nRace = GetMonRace(sMonName)`；`GetMonRace`（`WorldServer.cs:167-174`）查 `MonsterList`
   **查不到就返回 -1**。
3. `CreateMonster`（`WorldServer.MonGen.cs:498-…`）按 `nMonRace` 走 `switch` 造具体怪，
   **没有 `default` 臂** ⇒ race = -1 时 `cert` 保持 null ⇒ 返回 null。
4. `MakeSlave` 拿到 null ⇒ 返回 null ⇒ `MagMakeAngelSlave` 返回 false ⇒ **41 在生产中是空技能**
   （`boTrain` 也不会置位）。

**数据面证据**：仓内冻结 `sql/mir2_data.sql` 的 `monsters` 表里**没有名为 `精灵` 的行**
（`grep -c "'精灵'," sql/mir2_data.sql` = 0；文件里 7 处「精灵」全是 `骷髅精灵*` 变体）。

## 3. 解除条件（二选一，均需另行排期）

1. **补数据行**：在部署的 `monsters` 表补 `精灵`（race/属性按设计；这是**数据变更**，不在代码仓）。
   补完后 41 的生产链才成立，实现批可用真实路径出 golden。
2. **继续用 stub 出 golden**：可立即做，但 golden 只证明「给定一个怪物之后的 adopt/尾帧行为」，
   **不证明生产可召唤**；升格时 scope 必须写死「stub 验证 + 生产数据缺失（本卡）」。

## 4. 待核（不升级为结论）

部署库 `monsters` 是否同样缺 `精灵`：本审计**未直连查库**（避免访问凭据）。
**要核定需**：有权限者执行只读 `SELECT … FROM monsters WHERE name='精灵'`，结果贴回本卡。

## 5. 关联

- 装置缺口总审计：`docs/issues/device-gap-premise-audit.md`（MG-42 行）
- 下一批计划：`docs/issues/qa-next-batch-plan-20261121.md`（MG-42 行，已改为「生产数据缺口 + harness 可 stub」）
- 同类数据缺口：`docs/issues/openmir2-summon-monster-data-gaps.md`（同名族总卡，2026-11-21 已重立于 master）
