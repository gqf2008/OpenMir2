# run3 场景夹具迁移（inventory → fixtures.inventory_replace）— 2026-10-25

- 卡：`openmir2-scenario-fixture-migration`（tier **L**，owner `openmir2-dev-2`，branch `card/scenario-fixture-migration`）。
- **一句话摘要**：本卡修的不是「夹具写法」，而是**发现的输入源**——旧 `inventory` 通道在
  46/46 满包上写不进背包 ⇒ `SM_BAGITEMS(201)` 里没有该物品 ⇒ `make_index`/`item_name`/`takeon_where`
  **推不出** ⇒ discovery 必失败；迁移到 `fixtures.inventory_replace` 后物品落在槽 40/41 ⇒ 201 带它 ⇒
  派生成立（run3 不需要再加 `--var`）。
- 背景（W1 的 B 类：「场景侧问题，不是 Rust 差异」）：
  - **#14 `bag_equip` / #15 `eat_item`** 仍走旧 `inventory` 通道；基线背包 **46/46 全占** ⇒
    旧通道在声明槽位时 fail-closed（「声明槽位里已有 1 行」）。
  - **#13 `equip_struck_save`** 的 `(19,18)` 卫家店 3s 超时是「目标格没打到 actor」，
    不是超时不够。

## 1. 迁移：#14/#15 改走 profile 的 `fixtures.inventory_replace`

场景里**不再**声明 `inventory`（旧通道保持原样、其 fail-closed 语义一处未松），声明改到 profile：

| run3 | 场景 | profile（alt 副本 + 唯一差异） | replace |
|---|---|---|---|
| #14 | `bag_equip` | `tools/window/profiles/bag_equip.json` | `position=40, index=207, make_index=5001, std_mode=5`（木剑） |
| #15 | `eat_item` | `tools/window/profiles/eat_item.json` | `position=41, index=3, make_index=5002, std_mode=0, ani_count=102`（金创药(小量)） |

run3 的 `extra` 追加 `--profile tools/window/profiles/<name>.json`（`--profile` 后者覆盖前者，
与 #24 的 profile 通道同型）。**只替换本场景真实需要的那一槽**，不动其它槽。

`card_window.py` 的 `--selftest` 原自查 ①/② **把「bag_equip/eat_item 必须声明旧 `inventory`」写死了**
⇒ 迁移后必红。Leader 裁决授权严格范围修改；tester 的 rc 修复（`card_window.py:10172` 的 `0 or 2`）
先合入（`d858508a1`），本卡随后 rebase 并应用已保存补丁（`E:/tmp/cw-replace-dryrun.patch`，sha256 见卡）：

1. 自查 ① 改为「场景有 `inventory` **或** profile 有 `fixtures.inventory_replace`」；
2. 自查 ② 继续验证**旧通道** dry-run 预览（改用合成声明，用例不删）；
3. 新增自查 ④：`inventory_replace` 在 dry-run 下打印 `replace Position=…`；
4. **主 CLI dry-run** 接上 `run_inventory_fixture` 的 dry-run 分支（此前只有 selftest 调它，
   主 dry-run 一行不打 ⇒ 会被误读成「场景写错了」）。

dry-run 实测（两侧都打）：

```text
[csharp] 背包替换/丢弃夹具（dry-run 不连库）：1 项 positions=[40]
[csharp]   replace Position=40 StdIndex=207 MakeIndex=5001
[rust]   背包替换/丢弃夹具（dry-run 不连库）：1 项 positions=[40]
[rust]     replace Position=40 StdIndex=207 MakeIndex=5001
```

## 2. #13 寻靶：先证明「(19,18) 有固定 NPC」再攻击

`equip_struck_save` 原文直接 `CM_TURN + CM_HIT` 打身前格 `(19,18)`；该格在部署里应是固定 NPC
「卫家店」（`MerChant.txt:26`：`比奇城/卫家店 0103 19 18 …`），但格空时表现为 3s 超时且**无法区分**
「没打到 actor」与「打到了但没掉耐久」。

修法（场景侧，不改超时）：在原步骤前加**存在性前置**——

```json
{"click_npc": "pos:19,18"}          // 命中前过静态对象门；多命中/无命中 fail-closed
{"wait_for": 643, "timeout_ms": 5000}  // 643 = 点击应答面板：证明该格 NPC 真的在且应答
```

`pos:` 空命中的错误串会列出该格 **±3 邻近可见对象**（`name/(x,y)/actor/首见 seq/移动帧/类别`），
下次直接按清单改坐标即可；**不静默跳过、不加大超时**。

## 3. 三类失败信息怎么区分（排障用）

| 现象 | 错误串来源 | 关键词 |
|---|---|---|
| 旧通道槽位已被占 | `card_window.py` 的 `run_inventory`（`stditems`/写后读回） | `inventory: …`、`声明槽位里已有 N 行` |
| replace 声明非法/物品不存在 | `_validate_fixture_inv_replace`（未知键/越界/重复槽）与 `run_inventory_fixture`（`物品模板 index=%d 不存在`、`StdMode/Shape 不符`、写后读回不符、其它槽位指纹变化） | `inventory_replace: …` |
| 目标格无对象 / 无 NPC | probe `pos:` 寻址 | `不是固定/静态形态`（活动对象）或 `没有对象匹配 pos=(19,18)；该格邻近(±3)可见对象：…` |

## 4. 验证

| 项 | 结果 |
|---|---|
| `card_window.py --selftest` | **GREEN 435/435**（本 head 实测） |
| `mir2_probe --selftest` | OK 21 checks（本 head 实测） |
| 三场景 `--check-scenario` | 全 OK（bag/eat 带 `--var`；equip 7 步） |
| `run_batch --list` | 24（blocked 3 ⇒ 入批 21），#14/#15 的 `extra` 显示 `--profile …` |
| `blocked_sources_check check` | rc=0（两来源一致） |
| `git diff --check` / `merge_tree_helper` | rc=0 |
| 牙 | a 旧通道占位 fail-closed（`card_window` 既有用例 + ② 保留）；b replace 物品不存在 ⇒ `inventory_replace: 物品模板 index=… 不存在`（`run_inventory_fixture`，DB 面，窗口复核）；c `pos:` 空命中列邻近对象（probe 既有牙 + 本场景前置） |

## 5. 未覆盖 / 真机

- **真机三条各跑一次**（tester 窗口，本卡不自行起栈）：#14/#15 的 replace 写入/还原、#13 的 `pos:19,18+643`
  与随后 `642` 是否出现；证据（帧数、643/642）挂卡。
- replace 的「物品模板不存在 / StdMode 不符 / 写后读回」属 **DB 面**校验，dry-run 不连库，只能在窗口验证。

## 6. 残留物清单

- 入库：`tools/conformance/card_window.py`（授权范围：自查 ①/② 放宽 + 新增 ④ + 主 dry-run 接 replace 预览）、
  `tools/window/profiles/bag_equip.json`、`tools/window/profiles/eat_item.json`、
  `tools/probe/scenarios/{bag_equip,eat_item,equip_struck_save}.json`、`tools/window/run3-scenarios.json`（#14/#15 `extra`）、本文件。
- 未入库：`E:/tmp/be-dry.txt`、`E:/tmp/cw-st.log`、`E:/tmp/cw2.log`（dry-run/selftest 证据）。
- 未触碰：`run_batch.sh` 执行逻辑、`src/**`、mysqld；未起服务端、未占端口、未跑 cargo。
