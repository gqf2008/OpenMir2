# compare 规则表登记 ident 601（SM_DROPITEM_FAIL）— 交付报告

卡：`openmir2-compare-ident601-registration`（tier=L）· owner：openmir2-worker-74 ·
分支：`card/compare-ident601-registration`（base = walgit/master `2f22459fd`，
含 600 卡合并 `4a6c5406d`）· 2026-10-24

## 0. 结论

600 卡的直接残余已闭合：`SM_DROPITEM_FAIL(601)` 以 **action=compare 全字节比 +
`head_compare:["recog"]`** 入 `tools/conformance/rules.json`（第 48 条）。冻结夹具
`tools/conformance/fixtures/dropitem_ack/` 新增 601 case（与 600 同一 `CM_DROPITEM`
应答对），该帧由 **UNCLASSIFIED（rc 2）** 转为 **已比（rc 0，0 diff）**；三颗牙
（删规则 / 改 body 一字节 / 改 recog）全部实测红且可定位。**Rust 零变更**。

| 项 | 值 |
| --- | --- |
| 确认 status oid | `59343c6fc4b16953279f63c1e33528168af37673` |
| base | `2f22459fd`（walgit/master；其上 600 已合并） |
| 分支 | `card/compare-ident601-registration` |
| tier | L（纯规则表 + 夹具 + 自检；无 Rust/服务端改动，真机只做抽样冒烟） |

## 1. ① C# 源取证（601 语义）

| 项 | 证据 |
| --- | --- |
| 常量名 | **`SM_DROPITEM_FAIL = 601`**（`src/OpenMir2/Messages.cs:278`；上一行 :277 是 `SM_DROPITEM_SUCCESS = 600`） |
| 发送点 | `src/M2Server/Player/PlayObject.Message.cs:801-810` 的 `case Messages.CM_DROPITEM`：成功支 `:804`、失败支 **`:808`** `SendDefMessage(Messages.SM_DROPITEM_FAIL, processMsg.nParam1, 0, 0, 0, processMsg.Msg)` —— 与 600 **同 send 点、同五参形态** |
| 下行编码 | 6 参 `SendDefMessage`（`PlayObject.Socket.cs:27-38`）⇒ `MakeMessage(601, nRecog=nParam1, nParam=0, nTag=0, nSeries=0)` + `SendSocket(ClientMsg, EDCode.EncodeString(sMsg))`；`sMsg` 为空时不带尾（`:32-37`） |
| body 算法 | `EDCode.EncodeString`（`src/OpenMir2/EDCode.cs:175-186`）= `HUtil32.GetBytes`（GBK）→ `EncryptUtil.Encode(…, bSrc.Length, …)`：按**字节数**编码（3B→4 字符），空串不带尾 |
| 源侧字段映射 | `src/GameSrv/Word/WorldServer.cs:796-820`：CM_DROPITEM 走 `SendMsg(…, nParam1=Recog, nParam2=Param, nParam3=Tag, sMsg)` ⇒ **`processMsg.nParam1` = 线缆 `Recog` = 请求 `MakeIndex`**，回帧原样写回 `nRecog` |
| oracle 真机向量 | `mir2-rs/tests/parity/m3/fixtures/ground_vectors.json` case `drop_fail_item_id_zero`：request `{make_index:51, name:"ring"}` ⇒ `immediate_frames=[{ident:601, recog:51, param:0, tag:0, series:0}]`。同批七个 `drop_fail_*`（`control_price_too_high`/`safe_zone`/`no_throw_item`/`is_can_drop_false`/`deal_tick_guard`/`name_with_space`/`makeindex_mismatch`）全部 601 同形，recog 分别等于各自请求的 make_index（54/55/56/57/58/59/99） |
| Rust 同形 | `mir2-rs/bins/game-svr/src/ground.rs:746-765` 失败支 `fail_frame(make_index)` = `{ident:601, recog:make_index, param:tag:series:0}`（普通丢弃因 `MapItem.ItemId` 恒 0 恒走此支，T-19）；`pickwire.rs:66-79 immediate_frame` 分派到 `smframe.rs:399-406 sm_dropitem_reply(false, make_index, msg)` = `command_header(make_index, 601, 0, 0, 0)` + `gm_data_body(edcode::encode(msg))` |

**字段口径汇总**：`ident=601`；`Recog=MakeIndex`（语义值）；`Param=Tag=Series=0`
（源码常量）；`body=EDCode(GBK(请求物品名))`（空则无 body）。
**无墙钟/RNG/会话成分** ⇒ 全字节比，不 ignore、不掩码。

**为什么必须 `head_compare:["recog"]`**：全局 `recog` 掩码是为「会话相关 ActorId」
设的（`rules.json` `global[0]`）；601 的 recog 是 MakeIndex。机制见
`compare_runs.py` `RuleSet.mask_for`（先 copy 全局掩码，再按 `head_compare`
`masks.pop(f)`）。不取回会把两侧 MakeIndex 差异静默吞成 rc 0 —— 牙 C 钉死这一点。

## 2. ② 规则登记

`tools/conformance/rules.json` 追加第 48 条（append 位，紧接 600）：

```json
{"ident": 601, "action": "compare", "tier": "L",
 "head_compare": ["recog"],
 "reason": "SM_DROPITEM_FAIL(601) 是真实协议帧（Messages.cs:278）……（全文见 rules.json）",
 "approved_by": "PENDING owner/coordinator review (draft: openmir2-worker-74)"}
```

- 不登记 `head_mask` / `ranges` / `ignore`：头部常量与 body 全部照比。
- `approved_by` 保持 **PENDING**，未自批；批准归 reviewer/coordinator。
- diff 只有 +8 行（仅 601 条目），未顺带改动任何既有条目。

## 3. ③ 冻结夹具（新增 601 case）

| 项 | 值 |
| --- | --- |
| 路径 | `tools/conformance/fixtures/dropitem_ack/{csharp,rust}/frames.ndjson`（两侧仍逐字节相同，现各 2 行） |
| 600 行 | 既有（`pickup_item`：`make_index=5001`、`木剑` ⇒ body `54424d443f3f`） |
| 601 行（新增） | `seq=2, t_ms=3002, ident=601, recog=51, param=0, tag=0, series=0, body_len=6, body_hex=726d3e796f3e, body_sha256=9500c41c…c227d1eb, frame_class=protocol, tail="!", raw_len=24, body_text="ring"` |
| 来源 | 真机向量 `drop_fail_item_id_zero` 的请求参数（`make_index=51` / `name="ring"`）+ 该向量 `immediate_frames` 的头部形态；body 由 `EDCode.cs:175-186` 算法从 `request.name` 推出（`tools/probe/mir2frame.py` 编码 + strict 回环） |

**比较结果**：两侧 rc 0、`compared=2`、0 diff / 0 undecidable / 0 unclassified；
`coverage.ident_rules` 里 600 与 601 均为 `action=compare`。
README 的重新生成脚本已改为同时产出两行，实测输出与库内副本逐字节一致。

## 4. ④ 牙（手工变异实测 + 常驻自检牙）

手工变异在临时副本上进行、每次还原；常驻牙（`selftest_impl.py::t_dropitem_ack`
的 `601 ①-⑦`）每次跑门禁都复验。

| 牙 | 变异 | 实测 |
| --- | --- | --- |
| A：删 601 规则 | 备份后从 `rules.json` 移除 601 条目（47 条） | **RED 302/310，rc=1**；FAIL 8 项（@ 5980961da）：`601 ③`（规则缺失）、`601 ④`（rc 2）、`601 ⑥/⑦`（连锁）、`600 ④/⑤/⑥/⑦`（601 变 UNCLASSIFIED 后整轮非绿灯）。还原后 `rules.json` sha256 回到 `907ffda4…`，GREEN 310/310 |
| B：改坏 601 body 一字节 | rust 侧 `body_hex` 首字节 `72→73`，`body_sha256` 同步重算 | **RED 301/310，rc=1**；直接比较器输出 `{"ident":601,"kind":"body","offset":0,"oracle_byte":114(0x72),"rust_byte":115(0x73)}`（`verdict=FAIL`, rc=1）。还原后夹具 sha256 回到 `9292f05d…`，GREEN |
| C：改坏 601 recog | rust 侧 `recog 51→52` | **RED 303/310，rc=1**；直接比较器输出 `{"ident":601,"field":"recog","oracle":51,"rust":52}` ⇒ 证明 `head_compare` 生效、全局 recog 掩码没吞语义字段。还原后 GREEN |

## 5. ⑤ 门禁（在本卡 head 上实跑）

| 门禁 | 结果 |
| --- | --- |
| `python -I -B tools/conformance/compare_runs.py selftest` | **GREEN 310/310，rc=0**（基线 303/303 ⇒ +7，即 601 ①-⑦；未减少） |
| `python -I -B tools/conformance/card_window.py --selftest` | **GREEN 279/279，rc=0**（与基线相同） |
| `python -I tools/hygiene/conflict_marker_gate.py` | rc=0（0 markers / 7501 tracked files） |
| `python -I tools/hygiene/stale_pointer_gate.py` | rc=0 |
| `python -I tools/hygiene/dynamic_count_stamp_gate.py` | rc=0（0 缺戳，163 个 markdown；复审 F1 修复后复跑，复跑输出 `dynamic-count gate: OK（0 缺戳；6 条段落声明降级 INFO；扫描 163 个 markdown…）`） |
| `git diff --check` | rc=0 |
| `git merge-tree --write-tree walgit/master HEAD` | 无冲突（见收尾记录） |
| Rust 零变更 | 改动只含 `tools/conformance/**` 与本报告；无 `src/**`、`mir2-rs/**`、场景 JSON、服务端 `rules.json` |

## 6. 写集与残留物

- 改动：`tools/conformance/rules.json`（+8 行，仅 601）、
  `tools/conformance/selftest_impl.py`（docstring 第 17 条 + `t_dropitem_ack`
  增补 601 的 ①-⑦ 与共享辅助函数）、
  `tools/conformance/fixtures/dropitem_ack/**`（两侧各 +1 行 + README 更新）、本报告。
- 未碰：`src/**`、`mir2-rs/**`、`tools/probe/scenarios/**`、服务端 `rules.json`、
  `card_window.py`、`compare_runs.py`、600 卡已合并的文件（含 600 报告/rules 600 条目
  原文，未回头改 M-1 指针）。
- 临时物：`E:/tmp/w74-scratch/`（rules/frames 备份、牙 B/C 的 diff 报告、脚本），
  纯临时，可整目录删除；`E:/tmp/w74-selftest-block.py`、`E:/tmp/w74-write-docs.py`
  为一次性生成脚本。

## 7. 残余与越界发现（登记，不在本卡修）

1. **600 卡 review 的 M-1（报告 §5→§7 stale 指针）**：本报告内的指向已按实际章节写
   （残余在本 §7）；按卡要求未回头修改 600 卡已合并的文件。
2. **Rust `CM_DROPITEM` 取错线缆字段（越界发现，仍未改）**：oracle 的 `nParam1` 来自
   线缆 **`Recog`**（`WorldServer.cs:820`），Rust `lib.rs` 把 `cmd.param` 传给
   `handle_ground_cmd`。该差异由另卡（tier R）承接；本卡的 601 规则按 oracle 口径
   `head_compare:["recog"]` 登记，真机若跑到 601 会把该差异**如实判红**，不会掩掉。
3. **普通丢弃恒 601（T-19/T-25）**：`MapItem.ItemId` 全仓无赋值 ⇒ 普通
   `CM_DROPITEM` 在 oracle 恒回 601；600 只在 `ControlDropItem + 便宜物` 分支可达。
   601 是当前 oracle 树上的**常态路径**，登记后覆盖了绝大多数丢弃应答帧。
4. **真机窗口**：本卡 tier=L，只做构造级抽样冒烟（冻结夹具 + 真机向量锚 + 三颗牙），
   不占用真机窗口；已冻结的 `window-run-2` 抓帧中无 601 帧（自检 `missing` 只含 600，
   601 是规则表缺口而非抓帧缺口），故无「登记前后同帧对照」可做，证据链以源码 + 真机
   向量 + 夹具判定路径为准。
