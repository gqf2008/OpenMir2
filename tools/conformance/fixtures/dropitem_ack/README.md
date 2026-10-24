# dropitem_ack 对拍 fixture（卡 openmir2-compare-ident600/601-registration，tier=L）

`compare_runs.py selftest` 的**冻结输入**：同一条 `case Messages.CM_DROPITEM`
（`src/M2Server/Player/PlayObject.Message.cs:801-810`）的成功/失败两支应答。

| 行 | ident | 含义 | 规则卡 |
| --- | --- | --- | --- |
| `seq=1` | 600 | `SM_DROPITEM_SUCCESS`（`src/OpenMir2/Messages.cs:277`） | `openmir2-compare-ident600-registration` |
| `seq=2` | 601 | `SM_DROPITEM_FAIL`（`src/OpenMir2/Messages.cs:278`） | `openmir2-compare-ident601-registration` |

## 来源：构造等价夹具（不是真机抓帧）

`window-run-2` 的 `pickup_item` 场景未跑到（批跑取消），仓内没有 600/601 的真机
`frames.ndjson`。两张卡（验收判据 ③）允许「probe 单步取证或构造」；本夹具按
**两侧源码的确定性构造**，头部布局另有 oracle 真机向量锚定：

| 项 | 证据 |
| --- | --- |
| 常量 | `SM_DROPITEM_SUCCESS = 600`（`Messages.cs:277`）、`SM_DROPITEM_FAIL = 601`（`:278`） |
| 唯一发送点 | `PlayObject.Message.cs:801-810`：成功支 `:804` `SendDefMessage(600, processMsg.nParam1, 0, 0, 0, processMsg.Msg)`；失败支 `:808` 同形发 601 |
| 字段映射 | `src/GameSrv/Word/WorldServer.cs:796-820`：CM_DROPITEM 走 `SendMsg(…, nParam1=Recog, nParam2=Param, nParam3=Tag, sMsg)` ⇒ `processMsg.nParam1` = 线缆 `Recog` = 请求 `MakeIndex` |
| 头部形态（oracle 真机） | `mir2-rs/tests/parity/m3/fixtures/ground_vectors.json`：`drop_ok_control_price` ⇒ `[{600, recog:52, 0,0,0}]`；`drop_fail_item_id_zero`（request `{make_index:51, name:"ring"}`）⇒ `[{601, recog:51, 0,0,0}]` |
| body 算法 | `PlayObject.Socket.cs:27-38` 6 参 `SendDefMessage` ⇒ `SendSocket(ClientMsg, EDCode.EncodeString(sMsg))`；`EDCode.cs:175-186` = GBK 字节 → `EncryptUtil.Encode`（按**字节数**，非字符数），空串不带尾 |
| Rust 同形 | `smframe.rs:397-406` `sm_dropitem_reply(success, make_index, msg)`；`pickwire.rs:66-79` 把 600/601 分派给它；`ground.rs:746-765` 失败支 `fail_frame(make_index)` |

## 字节

| 行 | 来源参数 | 明文 GBK | EDCode | sha256 |
| --- | --- | --- | --- | --- |
| 600 | `tools/probe/scenarios/pickup_item.json`：`make_index=5001`、`item_name=木剑` | `c4 be bd a3` | `54424d443f3f` | `e8c33c3d97209b6302fde94d6a4987e728b83bcc509096071dbb86067460d746` |
| 601 | 真机向量 `drop_fail_item_id_zero.request`：`make_index=51`、`name="ring"` | `72 69 6e 67` | `726d3e796f3e` | `9500c41cd99da38613aa6c516f404cc2fb09191b2c4c13c41b91a7d1c227d1eb` |

头部：600 = `ident=600, recog=5001(MakeIndex), param=0, tag=0, series=0`；
601 = `ident=601, recog=51(MakeIndex), param=0, tag=0, series=0`。
`raw_len = 17 + body_len + len(tail) = 24`，其中 `17 = 1(#) + 16(EDCode(12B 头))`，
再加 6B body 与 1B `!`。`t_ms`/`seq` 是驱动器标注，进全局掩码，不参与判定。

两侧（csharp/rust）内容**逐字节相同**：这是构造夹具的必然结果——600/601 的头部与
body 在当前判据下全部确定（无墙钟/RNG/会话成分），夹具用于钉死比较器的
**分类与逐字节判定路径**，不用于证明两侧实现一致。两侧实现差异由 `compare_runs`
在真机窗口抓帧上暴露；本夹具的真实性锚点是上表的 oracle 真机向量与两侧源码。

重新生成（与库内副本逐字节一致）：

```sh
python -I - <<'PY'
import json, hashlib, importlib.util
spec = importlib.util.spec_from_file_location('m2f', 'tools/probe/mir2frame.py')
m = importlib.util.module_from_spec(spec); spec.loader.exec_module(m)
rows = []
for seq, (ident, recog, name) in enumerate(
        [(600, 5001, '木剑'), (601, 51, 'ring')], 1):
    plain = name.encode('gbk'); wire = m.encode_bytes(plain)
    assert m.decode_bytes(wire, strict=True) == plain
    rows.append({"seq": seq, "t_ms": 3000 + seq, "phase": "gamegate",
                 "conn": "gamegate", "ident": ident, "recog": recog,
                 "param": 0, "tag": 0, "series": 0,
                 "body_len": len(wire), "body_hex": wire.hex(),
                 "body_sha256": hashlib.sha256(wire).hexdigest(),
                 "frame_class": "protocol", "tail": "!",
                 "raw_len": 17 + len(wire) + 1, "body_text": name})
for side in ('csharp', 'rust'):
    open('tools/conformance/fixtures/dropitem_ack/%s/frames.ndjson' % side,
         'w', encoding='utf-8', newline='\n').write(
        ''.join(json.dumps(r, ensure_ascii=False) + '\n' for r in rows))
PY
```

## 范围

600 与 601 均已登记（`tools/conformance/rules.json`：`action=compare` 全字节比 +
`head_compare:["recog"]`），本夹具两帧在 `compare_runs` 里 `compared=2`、0 diff。
普通 `CM_DROPITEM` 在 oracle 当前树上因 `MapItem.ItemId` 恒 0 恒回 601（T-19），
600 只在 `ControlDropItem + 便宜物` 分支可达（`drop_ok_control_price`）——见各卡
报告的「残余与越界发现」节，不在本夹具纠正。
