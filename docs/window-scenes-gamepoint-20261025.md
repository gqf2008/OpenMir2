# 窗口新场景第二批：GAMEPOINT 兑换链 `gamepoint_exchange`（tier L，工具面）

- 卡：`openmir2-window-scenes-gamepoint`（owner `openmir2-dev-2`，branch `card/window-scenes-gamepoint`）。
- 写集：`tools/probe/scenarios/gamepoint_exchange.json`、`tools/window/run3-scenarios.json`（**只追加一行**）、本报告。
- **不动**：`card_window.py` / `run_batch.sh` / `start_mir2_server.ps1`、生产代码；不起服务端不占端口。
- 映射：`docs/window-kit-coverage-20261025.md` §1.3 的 **W-PD-1**（playdice T-142 ⑨ 的 GAMEPOINT 落库 + 5008）。

## 1. 步序（脚本行号）

| # | 步骤 | 依据 |
|---:|---|---|
| 1 | `click_npc npc_actor` | `CM_CLICKNPC(1010)` → `NormNpc.Click`（`NormNpc.cs:68-74`）→ `GotoLable(@main)`；recog 由 `param_discovery` 从 `643.recog` 发现 |
| 2 | `wait_for 643` | `[@main]`（`真爱-G002.txt:3-28`）：`<赠送[玫瑰花]/@shua>` `<使用[玫瑰花]/@yhua>`（`:7`） |
| 3 | `dlg_select @yhua` | `1011` → `Merchant.UserSelect`（`PlayObject.Operate.cs:114-128`）；`[@yhua]` 在 `:105` |
| 4 | `wait_for 643` | `[@yhua]` 页（`:105-110`）：`<$GAMEPOINT>` 当前朵数 + `<9朵兑换[一心一意]烟花/@9hua>`（`:107`） |
| 5 | `dlg_select @9hua` | `[@9hua]`（`:520`）：`#if CHECKGAMEPOINT > 8`（**:522**）/ `#act GAMEPOINT - 9`（**:524**）——:90/:92 属 `[@ChangeName]`(:88) 块 |
| 6 | `wait_for 5008` | `SM_GAMEGOLDNAME(5008)`：`SendGoldInfo`（`PlayObject.Base.cs:927-939`）`Param=LoWord(GamePoint)`、`Tag=HiWord(GamePoint)`；扣 9 后 9→0 |
| 7 | `wait_for 643` | 兑换后的脚本说词（同 handler） |

**placement**：`Merchant.txt:127` `功能NPC/真爱 G002 49 49 红娘 …` ⇒ 商人在 `(49,49)`；本场景放 **`G002 (50,49)`** 东邻格。

## 2. 证据形态（**不得假设任何未复算字段**）

- **帧**：`5008` 的 `Param/Tag` 逐字段（Lo/Hi of GamePoint）+ `643` 说词；帧序按 `MsgQueue` 的 `(byte)wIdent` 升序
  （`RM_MERCHANTSAY` 11009&0xFF=1 ＜ `RM_GAMEGOLDCHANGED` 8084&0xFF=**148** ⇒ 说词先（**注意用 RM，不是 SM 自带的 5008&0xFF=144**））。
- **DB**：`characters.GamePoint` 前后值（9 → 0）；**不写 `cleared` 之类未复算字段**。

## 3. 前置与登记

1. **测试角色 `SoftVersionDateEx != 0`**：`SendGoldInfo` 首行 `if (SoftVersionDateEx == 0) return;`
   （`PlayObject.Base.cs:930-933`）——为 0 时**根本不发 5008**，窗口前置需保证该列非 0（登录记录带出）。
2. `CHECKGAMEPOINT > 8` 需要测试库角色 `GamePoint≥9`。**runner 侧** `card_window.py` 的 `ASSETS_COLUMNS`
   自 P0 卡（`aa94a89db`）起已含 `GamePoint`；但 **`tools/probe/mir2_probe.py` 的 `_check_assets`
   仍只认 `Gold`** ⇒ 本场景**不写 `assets` 键**（否则 `--check-scenario` fail-closed，实测 rc=2），
   前置由窗口脚本/手工置库提供。**该不一致已登记给 Leader**（建议一张 L 卡把 probe 的白名单与
   `card_window.py` 对齐）。
3. `run3-scenarios.json` 的编号依赖：本分支基于 `card/window-scenes-clearmapmon` 的 `725bd57be`
   （含 #18/#19）——但**当前 master 的 run3 只到 #17**（clearmapmon 仍在审），故本分支从 master（含 P0 `aa94a89db`）起，本卡追加为 **#18**。**合并顺序**：若 clearmapmon（#18/#19）先合并，本行需顺延为 **#20**（由后合并方 rebase 时重编号，本卡不重排既有条目）。

## 4. 自证（离线，不启服务端）

```text
$ python -I -B tools/probe/mir2_probe.py --check-scenario \
    --scenario tools/probe/scenarios/gamepoint_exchange.json --var npc_actor=1
note: scenario.placement 声明了 map=G002 x=50 y=49，但本驱动器不预置位（由 card_window.py 在起栈前改测试库完成）
scenario OK: gamepoint_exchange (7 steps, params=npc_actor=1)      # rc=0
```

`run3-scenarios.json`：**只追加一行（#18）**（`git diff 725bd57be -- tools/window/run3-scenarios.json` = +2/−1，
即 #19 补逗号 + #20 新增），既有 19 条逐字节不变。

## 5. 门禁

见 patch body / status：四卫生门 + `git diff --check <base> <head>` + `merge_tree_helper`。
