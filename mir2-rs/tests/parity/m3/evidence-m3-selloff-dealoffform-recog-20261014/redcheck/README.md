# 改坏必红（⑩-1 收口片：SM_SENDDEALOFFFORM 头字段口径）

判据 = 源码「作废修复、还原旧口径」后，两条用例必须变红；还原后必须全绿。

## 变异体（MUT-1）

把 `bins/game-svr/src/selloff.rs::open_deal_off_form` 的帧头改回旧口径：

```diff
-            recog: npc_id,
-            param: 0,
+            recog: 0,
+            param: npc_id as u16,
```

- `01-mutated-red.txt`：`cargo test -p game-svr --lib open_deal_off_form` ⇒ **rc=101，2 failed**
  （`open_deal_off_form_gated_by_sale_deal_and_active_sell` 断言 `recog==42` 实得 0；
  `open_deal_off_form_wire_header_puts_npc_id_in_recog` 线缆头 `Recog` 实得 0、期望 0x1234_5678）。
- `02-restored-green.txt`：`cp` 回修复版后同命令 ⇒ **rc=0，2 passed**。
- 还原后 `git status` 仅 `M mir2-rs/bins/game-svr/src/selloff.rs`（无残留）。

> 变异体本身就是**修复前的已合并实现**（⑩-1 原 head 5e00def177d 的 `open_deal_off_form`），
> 因此这条红检同时证明：修复前该用例若按 oracle 口径写就会红 ⇒ 旧断言 `param==42/recog==0` 是被一起改坏的。
