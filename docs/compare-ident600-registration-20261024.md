# compare 规则表登记 ident 600（SM_DROPITEM_SUCCESS）— 交付报告

卡：`openmir2-compare-ident600-registration`（tier=L）· owner：openmir2-worker-74 ·
分支：`card/compare-ident600-registration`（base = walgit/master `f2ad4f6bb`）· 2026-10-24

## 0. 结论

`window-run-2` 自检的 `missing:[600]` 已闭合：600 以 **action=compare 全字节比 +
`head_compare:["recog"]`** 入 `tools/conformance/rules.json`。冻结夹具
`tools/conformance/fixtures/dropitem_ack/` 上该帧由 **UNCLASSIFIED（rc 2）** 转为
**已比（rc 0，0 diff）**；两颗牙（删规则 / 改 body 一字节）与第三颗牙（改 recog）
均实测红并可定位。**Rust 零变更**。

| 项 | 值 |
| --- | --- |
| 确认 status oid | `d8acfe7f4cd854c90bf1e6c2008b0894b74b5dab` |
| base | `f2ad4f6bb8e639ceed582b9ac0c1018ccdb610d4`（walgit/master） |
| 分支 | `card/compare-ident600-registration` |

## 1. ① C# 源取证（600 语义）

| 项 | 证据 |
| --- | --- |
| 常量名 | **`SM_DROPITEM_SUCCESS = 600`**（`src/OpenMir2/Messages.cs:277`；同处 `SM_DROPITEM_FAIL = 601` :278） |
| 唯一发送点 | `src/M2Server/Player/PlayObject.Message.cs:801-810`（`case Messages.CM_DROPITEM`）：成功支 `:804` `SendDefMessage(SM_DROPITEM_SUCCESS, processMsg.nParam1, 0, 0, 0, processMsg.Msg)`；失败支 `:808` 同行形发 601 |
| 下行编码 | 6 参 `SendDefMessage`（`PlayObject.Socket.cs:27-38`）⇒ `MakeMessage(600, nRecog=nParam1, nParam=0, nTag=0, nSeries=0)` + `SendSocket(ClientMsg, EDCode.EncodeString(sMsg))`；sMsg 为空时不带尾（`:32-37`） |
| body 算法 | `EDCode.EncodeString`（`src/OpenMir2/EDCode.cs:175-186`）= `HUtil32.GetBytes`（GBK）→ `EncryptUtil.Encode(…, bSrc.Length, …)`：**按字节数**编码（3B→4 字符，长度 = n + ceil(n/3)），空串不带尾 |
| 源侧字段映射 | `src/GameSrv/Word/WorldServer.cs:796-820`：CM_DROPITEM 走 `SendMsg(…, wParam=Series, nParam1=Recog, nParam2=Param, nParam3=Tag, sMsg)` ⇒ **`processMsg.nParam1` = 线缆 `Recog` = 请求里的 `MakeIndex`**（不是线缆 `Param`） |
| 头部形态（oracle 真机） | `mir2-rs/tests/parity/m3/fixtures/ground_vectors.json` case `drop_ok_control_price`（ground-probe 实测，`ControlDropItem + 便宜物` 是**当前唯一能成功的丢弃支**）：`immediate_frames=[{ident:600, recog:52, param:0, tag:0, series:0}]`；同 send 点的 `drop_fail_*` 全为 601 同形 |
| Rust 同形 | `mir2-rs/bins/game-svr/src/smframe.rs:397-406` `sm_dropitem_reply(success, make_index, msg)` = `command_header(make_index, ident, 0, 0, 0)` + `gm_data_body(edcode::encode(msg))`；接线 `lib.rs:3372/3384-3389` 把 CM_DROPITEM 尾的 GBK 原始字节传给 `edcode::encode` |

**字段口径汇总**：`ident=600`；`Recog=MakeIndex`（语义值，来自请求/持久状态）；
`Param=Tag=Series=0`（源码常量）；`body=EDCode(GBK(请求物品名))`（为空则无 body）。
**无墙钟/RNG/会话成分** ⇒ 不满足噪声帧例外，登记为全字节比，不 ignore、不掩码。

**为什么必须 `head_compare:["recog"]`**：全局 `recog` 掩码是为「会话相关 ActorId」
设的（rules.json `global[0]`）；600 的 recog 是 MakeIndex。若不取回，两侧 MakeIndex
不同会被静默吞掉（同类先例：54/5008/1106/752/811/766/213 的取回规则与卡
`openmir2-conformance-rules-recog-unmask`）。牙 C 专门钉死这一点。

## 2. ② 规则登记

`tools/conformance/rules.json` 追加第 47 条（append 位，ident 段尾部）：

```json
{"ident": 600, "action": "compare", "tier": "L",
 "head_compare": ["recog"],
 "reason": "SM_DROPITEM_SUCCESS(600) 是真实协议帧（Messages.cs:277）……（全文见 rules.json）",
 "approved_by": "PENDING owner/coordinator review (draft: openmir2-worker-74)"}
```

- 不登记 `head_mask` / `ranges` / `ignore`：头部常量与 body 全部照比。
- `approved_by` 保持 **PENDING**，未自批；批准归 owner/coordinator。

## 3. ③ 冻结夹具（构造等价夹具）

| 项 | 值 |
| --- | --- |
| 路径 | `tools/conformance/fixtures/dropitem_ack/{csharp,rust}/frames.ndjson`（两侧逐字节相同） |
| 来源 | 构造等价夹具（卡验收 ③ 允许「构造」）：`window-run-2` 的 `pickup_item` 未跑到，仓内无 600 真机抓帧 |
| 场景参数 | `tools/probe/scenarios/pickup_item.json`：`make_index=5001`、`item_name=木剑` |
| body | GBK `c4bebda3` → EDCode **`54424d443f3f`**（6B），sha256 `e8c33c3d97209b6302fde94d6a4987e728b83bcc509096071dbb86067460d746` |
| 头部 | `ident=600, recog=5001(MakeIndex), param=0, tag=0, series=0` |
| 真实性锚 | 头部形态由上面的 ground-probe 真机向量 `drop_ok_control_price` 钉住（selftest ② 每次复验）；body 由 `EDCode.cs:175-186` 算法与 Rust `edcode::encode` 双向一致推出，夹具生成时用 `tools/probe/mir2frame.py` 编码并 strict 解码回环验证 |

**比较结果**：两侧 600 帧 rc 0、`compared=1`、0 diff / 0 undecidable /
0 unclassified；`coverage.ident_rules` 里 600 为 `action=compare`。
**删 600 规则的同输入 ⇒ rc 2**、`unclassified_ident ident=600`（见 §4 牙 A）。

## 4. ④ 牙（全部实测、全部还原）

| 牙 | 变异 | 实测 |
| --- | --- | --- |
| A：删 600 规则 | 备份后从 `rules.json` 移除 600 条目 | selftest **RED 299/303，rc=1**；`③ 规则表` FAIL、`④ 冻结夹具` FAIL（rc 2，UNCLASSIFIED 复现）、`⑥/⑦` 连锁 FAIL；`⑤ 牙A` PASS（点名 `unclassified_ident ident=600`）。还原后 rules.json sha256 回到 `e954f2b6…` |
| B：改坏 body 一字节 | rust 侧夹具 `body_hex` 首字节 `54→55`，`body_sha256` 同步重算（保证到达比较器） | selftest **RED 299/303，rc=1**；diff 精确定位 **`ident=600, kind=body, offset=0, oracle_byte=84(0x54), rust_byte=85(0x55)`**（见 ⑦ 的 detail）。还原后两侧夹具 sha256 都回到 `fcd59eca…` |
| C：改坏 recog | rust 侧 `recog 5001→5002` | selftest **RED 299/303，rc=1**；diff `ident=600, field=recog, oracle=5001, rust=5002` ⇒ 证明 `head_compare` 生效、全局 recog 掩码没有吞掉语义字段。还原后 GREEN |

牙 A/B/C 同时**常驻**在 `selftest_impl.py::t_dropitem_ack`（⑤⑥⑦），每次跑门禁都会复验，
不是一次性手工证据。变异都在临时目录/恢复副本上进行，库内夹具与规则表始终是最终形态。

## 5. ⑤ 门禁（在本卡 head 上实跑）

| 门禁 | 结果 |
| --- | --- |
| `python -I -B tools/conformance/compare_runs.py selftest` | **GREEN 303/303，rc=0**（基线 296/296 ⇒ +7，未减少） |
| `python -I -B tools/conformance/card_window.py --selftest` | **GREEN 279/279，rc=0**（与基线相同） |
| `python -I tools/hygiene/conflict_marker_gate.py` | rc=0 |
| `python -I tools/hygiene/stale_pointer_gate.py` | rc=0 |
| `python -I tools/hygiene/dynamic_count_stamp_gate.py` | rc=0 |
| `git diff --check` | rc=0 |
| `git merge-tree --write-tree walgit/master HEAD` | 无冲突（见收尾记录） |
| Rust 零变更 | `git diff --stat walgit/master..HEAD` 只含 `tools/conformance/**` 与本报告；无 `mir2-rs/**`、`src/**`、场景 JSON、服务端 rules.json |

## 6. 写集与残留物

- 改动：`tools/conformance/rules.json`（+8 行）、`tools/conformance/selftest_impl.py`
  （docstring 第 16 条 + `t_dropitem_ack`，+147 行）、
  `tools/conformance/fixtures/dropitem_ack/**`（新建 5 文件）、本报告。
- 未碰：`src/**`、`mir2-rs/**`、`tools/probe/scenarios/**`、服务端 `rules.json`、`card_window.py`、`compare_runs.py`。
- 临时物：`E:/tmp/w74-scratch/`（rules/frames 备份、插入脚本、门禁日志），纯临时，可整目录删除。
- 批准状态：600 条目 `approved_by` 为 PENDING，未自批。

## 7. 残余与越界发现（登记，不在本卡修）

1. **601 未登记**：`SM_DROPITEM_FAIL(601)` 与 600 同 send 点、同头部/body 形态
   （`ground_vectors.json` 的 `drop_fail_*` 全部 `{601, recog=MakeIndex, 0,0,0}` + 同名尾）。
   `window-run-2` 自检 `missing` 只含 600，本卡按写集只登记 600；601 仍会 fail-closed
   判 UNCLASSIFIED。建议下一张同肌肉卡一并登记（证据已在本报告 §1）。
2. **`pickup_item` 的 600 在当前两边实现下不可达**：oracle `MapItem.ItemId` 全仓无赋值点
   （`src/Modules/SystemModule/Data/MapItem.cs` 构造器里赋值被注释），`Envir.AddItemToMap`
   首行 `ItemId == 0 ⇒ false`，故普通 `CM_DROPITEM` 恒回 601（whitelist T-19；Rust 1:1 复刻）。
   当前唯一成功支是 `ControlDropItem && Price < CanDropPrice`（ground-probe case
   `drop_ok_control_price`）。**本卡只闭合「600 未登记 ⇒ UNCLASSIFIED」**，不改变
   该场景端到端可达性；要真跑到 600 需另开卡（改场景参数/配置或修 ItemId）。
3. **Rust CM_DROPITEM 的 MakeIndex 取错线缆字段（越界发现，未改）**：
   oracle 的 `nParam1` 来自线缆 **`Recog`**（`WorldServer.cs:820`），而 Rust
   `lib.rs:21111-21113` 把 `cmd.param` 传给 `handle_ground_cmd`（其 doc 亦写
   「nParam1（param）= MakeIndex」，`lib.rs:3231`）。场景 `pickup_item.json` 的
   C2S 步骤是 `recog=make_index`，故 Rust 实际会读到 `param=0` ⇒ 丢弃匹配失败、
   回 601 且 recog=0（oracle 侧 601 recog=5001），与 oracle 不等价。
   这是**行为差异（非本卡写集，R 级候选）**：本卡的 600 规则按 oracle 口径
   `head_compare:["recog"]` 登记，若真机跑到 600/601 会把该差异**如实判红**，
   不会掩掉。建议 coordinator 另开卡处理（改 Rust 取 `recog` + 复验 ground 用例）。
