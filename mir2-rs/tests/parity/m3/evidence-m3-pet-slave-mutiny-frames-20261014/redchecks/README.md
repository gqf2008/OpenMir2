# redchecks/ —— 改坏必红（RC1–RC3）

卡：`openmir2-m3-pet-slave-masterdie-wire`（残余帧面）；作者 `openmir2-worker-20`。

口径：**改 → 跑 → 还原**；还原用 `git checkout --` 后以 `git hash-object <f> == git rev-parse HEAD:<f>` 机器核对。
变异脚本 = [`mutate.py`](mutate.py)（`python -I mutate.py <m1|m2|m3>`，断言 anchor 唯一）。

| 编号 | 变异（相对实现提交 c55dff977） | 应红的用例 | 实读 |
| --- | --- | --- | --- |
| RC1 | `bins/game-svr/src/smframe.rs`：`sm_changenamecolor` 的 `Param` 由 `u16::from(name_color)` 改 `0` | `smframe::tests::sm_changenamecolor_header_only` | **rc=101** · 0 passed 1 FAILED |
| RC2 | `bins/game-svr/src/main.rs`：`drain_slave_mutiny_downlink` 的「色循环 / 名循环」合并为每宝宝一组 `色,名` | `tests::slave_mutiny_downlink_groups_all_colors_before_names` | **rc=101** · 1 passed 1 FAILED |
| RC3 | `bins/game-svr/src/main.rs`：删掉名帧循环（只留色帧） | `tests::slave_mutiny_downlink_emits_namecolor_then_username` + `...groups_all_colors_before_names` | **rc=101** · 0 passed 2 FAILED |

还原后重跑（[`restored-green.txt`](restored-green.txt)）：3/3 **ok** rc=0；
实现文件 `git hash-object` 与 `HEAD:` 相等（`da04add7…`、`248bcb7e…`），`git status --porcelain` 实现文件干净。

> RC2 只会打红「多宝宝出队序」那条；单宝宝的 `...emits_namecolor_then_username` 在 RC2 下仍绿
> —— 这正是该用例**分工**的意义（前者钉优先级出队序，后者钉「色→名」与载荷/AOI 距离门）。
