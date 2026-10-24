# `butch_two_draws` 的剩余缺口：场景击杀步（tier L，取证/收窄）

- 卡：`openmir2-butch-scene-kill-step`（owner `openmir2-dev-2`，branch `card/butch-scene-kill-step`，base `13013389e`）。
- 结论：**本轮不做击杀步**（属跨工具面改动，见 §2），按卡面红线**保持 `blocked`**，并把 `missing_fixture` 收窄到唯一剩余缺口。

## 1. 已落地（不再是缺口）

| 项 | 状态 | 依据 |
|---|---|---|
| 活体夹具（`fixtures.monsters`） | **已落地** | `card_window.py:3339-3367` 的 `profile_fixtures`；`docs/window-fixture-corpse-20261025.md` §2/§3（写 MonGen 行 `map x y name 0 count 360`，**第 7 列 ZenTime=360>0**；`=0` 会被两端解析器丢弃） |
| C# 栈 RNG 接线 | **已落地** | `15dbbe843`（`start-csharp.ps1` + `RngSeedHook.dll` + `MIR2_RNG_SITE`） |
| `CM_BUTCH(1007)` 两段帧 | 参数口径已写死 | `butch_two_draws.json`：`recog=MakeLong(X,Y)` → `SM_SKELETON(33)` → `SM_BUTCH(637)`（`PlayObject.Message.cs:829-831`、`Messages.cs:199/299`） |

## 2. 唯一缺口：场景击杀步（本轮未做，原因）

需要同时动两处**非本卡写集**的能力：
1. **profile 侧**：在 `tools/conformance` 的 profile 里声明 `fixtures.monsters=[{map,x,y,name,count}]`（该文件与 `card_window.py` 属 tester 写集）；
2. **场景侧**：新增「攻击到死亡」的步——`CM_HIT(1004)`（或 `combat_strike.json` 用的 `CM_HIT(3014)` 一系）反复打，直到 `wait_for` 死亡/尸体帧；死亡/尸体的 ident 需按 oracle（`SM_DEATH`/`SM_STRUCK` 与 `monai` 的 `settle_deaths`）与既有 `combat_*` 场景确认后写死。

第 2 项要定「打几次、每次间隔、判据 ident」，属**新场景步型/参数口径**，与本卡（只收窄 blocked）不同批；且该步会被以后所有「需要尸体」的场景复用，建议单开 L 卡（可命名 `openmir2-probe-kill-step`）。

## 3. 当前 `butch_two_draws.json` 的 `blocked`

```
reason        = 只剩「场景内击杀步」；活体夹具与 C# RNG 均已落地
missing_fixture = 补击杀步（CM_HIT 打到死亡 + wait 尸体帧）+ profile 侧 fixtures.monsters 在 CM_BUTCH 同格刷 1 只
```

run3 **不改**：`#21 butch_two_draws` 仍 `blocked`（`--list` 标 `YES`，批次自动排除）。

## 4. 自证与门禁

- `--check-scenario` rc=0（4 步，blocked 形状合法）。
- 卫生门禁 + `git diff --check <base> <head>` + `merge_tree_helper` 见 patch body。
