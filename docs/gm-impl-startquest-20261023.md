# GM `@开始提问`（`StartQuestCommand`）空实现（T-219）

- 卡：`openmir2-gm-impl-startquest`（tier=R）；分支 `card/openmir2-gm-impl-startquest`
- 作者：`openmir2-worker-24`（外部车道）；**返工**：`openmir2-worker-67`（rebase 到 `43ce8dca5` + 复审 `f015343d` 的 1 blocking / 1 important / 3 minor）
- oracle 命令体：`src/Modules/GameCommand/Commands/StartQuestCommand.cs:10-23`

## oracle（`StartQuestCommand.cs:10-23`）

`:12-15` 死分支；`:16` 取 `sQuestName`；`:17-21` 空名 ⇒ 红字 `命令格式: @开始提问 问答名称`
（`Name` 被 `[Command] StartQuest=开始提问` 覆盖、`Help` = 「问答名称」）；
**`:22` `WorldEngine.SendQuestMsg(sQuestName);` 整行被注释** ⇒ 非空名**零帧、零副作用**。

## 实现

`Imp::StartQuest` + `start_quest()`：空名 ⇒ 红字；非空 ⇒ `no_frames(NOTE_START_QUEST_MSG_COMMENTED)`
（不臆造问题系统）。合并后 `gm_registry.json` 重算（Implemented **100** / Unimplemented **85**；
与 T-215 `ShowEffect` 同表 ⇒ 两侧各 +1）。

## 用例（3）与牙（2）

表项（字面量帮助文本）；空名/纯空格 ⇒ 帮助；非空两种写法 ⇒ 零帧 + 登记 + 无副作用；
perm 9 ⇒ `权限不够!!!`；**GBK 线上字节**：帮助文案逐字节钉住（28 字节，`downlink::gbk`）。
牙：表项回退 `Unimplemented` ⇒ 红；非空回帮助 ⇒ 红（均实做并还原，sha256 见交付 body）。

## 门禁

fmt / clippy `-D warnings` / `cargo test --workspace` / msgcodegen 604 / anchor ALL PASS /
registry GREEN / T 预约表 GREEN / gm_registry `build+gate+selftest` GREEN（全部跑在**合并树**上）。

## 窗口项（R，未跑）

部署唯一落点：`QuestDiary/LEGM2.CC假人系统/假人登录脚本.txt:40`
（`GMEXECUTE 开始提问 @假人指哪打哪`；Mir200 同）。**注意子目录 `LEGM2.CC假人系统/`**。

- ① **非空参数（部署原样，字面量）**：`GMEXECUTE 开始提问 @假人指哪打哪` ⇒ 两侧**零帧**、状态/DB 不受影响。
  参数是**字面量**，`GMEXECUTE` **不展开 `<$…>`**（runbook §4.6）；因此「非空 ⇒ 零帧」的期望成立。
- ② **空参数（抬权路）**：`GMEXECUTE 开始提问`（无第二参）⇒ `sData` 末尾空格、`Trim` 后 `parameters=""`
  ⇒ `params=[""]` ⇒ 空名分支 ⇒ 两侧**各一条 Red** `命令格式: @开始提问 问答名称`。
  权限由 `GMEXECUTE` 跳到 10（`ExecutionProcessingSys.cs:4925-4947`），**不依赖 admin 账号**。
- ③ **对照（不要写成「看到帮助」）**：玩家直连 `@开始提问` 在部署上**必然**是 `权限不够!!!`——
  `Envir/adminlist.txt` 唯一行缺 IP 被 `LocalDB.LoadAdminList` 丢弃（`LocalDB.cs:80-82`）、
  `Server.conf StartPermission=0` ⇒ 没有 perm 10 账号，`GameCommand.Handle` 的权限门先于命令体。
  这一条只能当**权限门等价性**的对照判据；要验帮助文案必须走 ②。

窗口未跑前本卡不得 `closed`。未起栈/未占端口/未连库。

## 残留物

- `E:/tmp/w24-sq.sha`（原作者牙基线，保留供追溯；本返工另用 `E:/tmp/wt67-gm-*.txt`）。
