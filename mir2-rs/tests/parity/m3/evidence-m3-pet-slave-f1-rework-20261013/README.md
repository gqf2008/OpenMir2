# M3⑨-2 F-1 返工证据（reviewer-6 `7724b9e441` request_changes 响应）

- 卡：`openmir2-m3-pet-slave-summon`（M3⑨-实现 2）。
- principal：`openmir2-worker-20`；worktree `m3hp`；分支 `worktree-m3-pet-slave-summon`。
- base：`refs/heads/master` = `d5be14f40eea1ab4e408b5aa055ba02ae3b367da`。
- 返工代码提交：`11c98bf84ef941329fb1f6cd4f5cc48179a821cc`（在 20 片 `c6c9f0928` 之上；证据提交在其上）。
- 边界：`git diff --name-only d5be14f40 11c98bf8 -- src sql MirClient` = **空**（见 `04-boundary.txt`）。
- 未起栈、未占端口、未 merge、未自签。

## 0. 一句话结论

reviewer-6 对 head `8cd889090` 的 `request_changes` 只有一条 Important（F-1）：`RM_MAGICFIRE`
的 oracle 处理器**没有** `ActorId != ActorId` 守卫（`PlayObject.Message.cs:1788-1794`，对比 `RM_SPELL`
`:1454-1460` 有），所以施法者自己也应收 `SM_MAGICFIRE`；Rust 原用同一个排除自己的 `broadcast_ref_msg`
发两帧 ⇒ 施法者丢自己那帧。本返工把广播拆出 `broadcast_ref_msg_with_self`（含自己）专供
`RM_MAGICFIRE`（召唤面 + 20 诱惑面），`RM_SPELL` 仍排除自己；并给召唤面端到端用例加
「自身收 `SM_MAGICFIRE`、自身不收 `SM_SPELL`」断言 + 改坏必红。两条 Minor（M-1 `magics.rs` 文档、
M-2 证据 README head）一并订正。

## 1. oracle 锚点
见 [`01-f1-oracle-anchors.txt`](01-f1-oracle-anchors.txt)。

## 2. 改前 / 改后
见 [`02-change-summary.txt`](02-change-summary.txt)：代码净改动 `main.rs +51/-…`、`magics.rs` 文档 6 行
（`git diff --stat c6c9f0928 11c98bf8`）；另同步既有两个证据目录的门禁读数与 SHA256SUMS。

## 3. 判据
见 [`03-tests.txt`](03-tests.txt)、[`gates/13-summon-wire-test.txt`](gates/13-summon-wire-test.txt)：
`spell_cmd_summon_wires_amulet_and_frames` 增自身帧断言；`slave` 7 passed；`world combat` 1 passed；
全 workspace **98 suites · 1024 passed · 0 failed · 21 ignored**。

## 4. 改坏必红
见 [`redchecks/`](redchecks/)：RC5 把 `broadcast_ref_msg_with_self` 的 `include_self` 改回 `false`
⇒ `spell_cmd_summon_wires_amulet_and_frames` FAILED（mutated rc=101）、restored rc=0、
还原后 `git status` 空。（20 片的 RC1..RC4 见 `evidence-m3-pet-slave-taming-20261013/redchecks/`。）

## 5. 四门禁（隔离 `CARGO_TARGET_DIR=E:/tmp/openmir2-m3hp-target`，`--offline`）
见 [`gates/`](gates/)：`10-fmt` rc=0 / `11-clippy` rc=0 / `12-msgcodegen` 604 rc=0 /
`16-workspace-test` 98 suites 1024 passed 0 failed 21 ignored rc=0。

## 6. 未接（登记不变）
同 `evidence-m3-pet-slave-taming-20261013/README.md` §6：`RM_MAKEHOLYSEIZEMODE` 帧、`OpenCrazyMode`
态、race 54 `Animal`、`NoTame` 恒 false、`CanReAlive`/`MonGen`、`FastTrain`、真机
`run_hero_pet_pair.ps1` diff=0（需窗口）⇒ not_verified。
