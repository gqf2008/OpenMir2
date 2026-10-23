# GM `@开始提问`（`StartQuestCommand`）空实现（T-219）

- 卡：`openmir2-gm-impl-startquest`（tier=R）；分支 `card/openmir2-gm-impl-startquest`，base `d3393f182`
- 作者：`openmir2-worker-24`；worktree `OpenMir2-wt-cronclaim`

## oracle（`StartQuestCommand.cs:10-23`）

`:12-15` 死分支；`:16` 取 `sQuestName`；`:17-21` 空名 ⇒ 红字 `命令格式: @开始提问 问答名称`
（`Name` 被 `[Command] StartQuest=开始提问` 覆盖、`Help` = 「问答名称」）；
**`:22` `WorldEngine.SendQuestMsg(sQuestName);` 整行被注释** ⇒ 非空名**零帧、零副作用**。

## 实现

`Imp::StartQuest` + `start_quest()`：空名 ⇒ 红字；非空 ⇒ `no_frames(NOTE_START_QUEST_MSG_COMMENTED)`
（不臆造问题系统）。`gm_registry.json` 重算（Implemented 98→99 / Unimplemented 87→86）。

## 用例（2）与牙（2）

表项（字面量帮助文本）；空名/纯空格 ⇒ 帮助；非空两种写法 ⇒ 零帧 + 登记 + 无副作用；
perm 9 ⇒ `权限不够!!!`。牙：表项回退 ⇒ 红；非空回帮助 ⇒ 红（均还原、sha256 一致）。

## 门禁

fmt / clippy `-D warnings` / `cargo test --workspace`（141 bins 0 FAILED）/ msgcodegen 604 /
anchor ALL PASS / registry GREEN / T 预约表 GREEN / gm_registry `build+gate+selftest` GREEN。

## 窗口项（R，未跑）

脚本 `QuestDiary/假人登录脚本.txt:40` 的 `Gmexecute 开始提问 <名>` ⇒ 两侧零帧；
无参 ⇒ 两侧各一条 Red `命令格式: @开始提问 问答名称`。窗口未跑前不 closed。
未起栈/未占端口/未连库。

## 残留物

- `E:/tmp/w24-sq.sha`（牙基线）。
