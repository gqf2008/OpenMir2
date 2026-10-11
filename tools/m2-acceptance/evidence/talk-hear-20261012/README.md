# 证据：`CM_SAY` 普通说话 → `SM_HEAR(40)`（2026-10-12）

- 卡：`openmir2-m2-gamesvr-host`
- 片：`brief-m2svr-next.md` §21 第 1 条（补驱动 `talk` 阶段）+ 服务端回声（`SM_HEAR`）。
- 报告：`docs/M2-聊天-普通说话-20261012.md`

## 文件

| 文件 | 是什么 | 命令 |
| --- | --- | --- |
| `green-after-fix.txt` | 绿检：帧逐字节 + 收件人集合（含自己） | `cargo test -p game-svr --offline say_broadcasts_hear_to_self_and_near_players_only` |
| `redcheck-no-self.txt` | 红检①：`hear_targets` 去掉「含自己」⇒ **FAILED**（`socket=901 未收到 SM_HEAR`） | 同上（先改坏，后还原） |
| `redcheck-series.txt` | 红检②：`sm_hear` 的 `Series` 1→0 ⇒ **FAILED**（`left:0/right:1`） | 同上（先改坏，后还原） |

## `m2-03-talk.png`（真机截图）——**未采，如实登记 PREREQ**

- 驱动阶段已就绪：`tools/client/mir_flow.ps1` 的 `talk` 阶段（`ingame` 之后）——
  回车开聊天框 → 注入 `-TalkText` → 回车发送 → 截 `m2-03-talk.png`。
- 本片**未起栈、未占端口**：栈/窗口状态见交付时的 `E:/tmp/openmir2-watch/last-m2svr.md`。
  真机采图命令（窗口可用时）：

  ```powershell
  powershell -ExecutionPolicy Bypass -File tools/client/mir_flow.ps1 `
    -Account mir2test -Password mir2pass -OutDir E:/tmp/m2acc/run1 -WalkClicks 3 -TalkText "m2 talk ok"
  ```

  产物 `E:/tmp/m2acc/run1/m2-03-talk.png`；用 `check_screenshots.ps1 -ShotsDir E:/tmp/m2acc/run1` 校验清单。
- **画面判据**：聊天框里出现 `aaa:m2 talk ok`（本角色自己发的文本）——这条正是服务端 `SM_HEAR` 的回声。
