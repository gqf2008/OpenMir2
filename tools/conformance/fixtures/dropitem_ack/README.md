# dropitem_ack 对拍 fixture（卡 openmir2-compare-ident600-registration，tier=L）

`compare_runs.py selftest` 的**冻结输入**：ident 600 = `SM_DROPITEM_SUCCESS`
（`src/OpenMir2/Messages.cs:277`）的 pickup 链等待帧。

## 来源：构造等价夹具（不是真机抓帧）

`window-run-2` 的 `pickup_item` 场景未跑到（批跑取消），仓内没有 600 的真机
`frames.ndjson`。本卡（验收判据 ③）允许「probe 单步取证或构造」；本夹具按
**两侧源码的确定性构造**，头部布局另有 oracle 真机向量锚定：

| 项 | 证据 |
| --- | --- |
| 常量 | `SM_DROPITEM_SUCCESS = 600`（`src/OpenMir2/Messages.cs:277`） |
| 唯一发送点 | `src/M2Server/Player/PlayObject.Message.cs:801-810`：`SendDefMessage(600, processMsg.nParam1, 0, 0, 0, processMsg.Msg)`（成功支 :804） |
| 字段映射 | `src/GameSrv/Word/WorldServer.cs:796-820`：CM_DROPITEM 走 `SendMsg(…, wParam=Series, nParam1=Recog, nParam2=Param, nParam3=Tag, sMsg)` ⇒ `nParam1 = 线缆 Recog = MakeIndex` |
| 头部布局（oracle 真机） | `mir2-rs/tests/parity/m3/fixtures/ground_vectors.json` case `drop_ok_control_price`（ground-probe 实测）：`immediate_frames=[{ident:600, recog:52, param:0, tag:0, series:0}]` |
| body 算法 | `PlayObject.Socket.cs:27-38` 6 参 `SendDefMessage` ⇒ `SendSocket(ClientMsg, EDCode.EncodeString(sMsg))`；`EDCode.cs:175-186` = `GBK 字节 → EncryptUtil.Encode`（按**字节数**，非字符数），空串不带尾 |
| Rust 同形 | `mir2-rs/bins/game-svr/src/smframe.rs:397-406` `sm_dropitem_reply`；接线 `lib.rs:3372/3384-3389` 把 CM_DROPITEM 尾的 GBK 原始字节交给 `edcode::encode` |

## 字节

场景 `tools/probe/scenarios/pickup_item.json` 的参数：`make_index=5001`、
`item_name=木剑`。请求物品名按 C# 口径 `HUtil32.GetBytes` 取 GBK：

```
明文 GBK:  c4 be bd a3   (木剑)
EDCode:    54 42 4d 44 3f 3f   (6B)
sha256:    e8c33c3d97209b6302fde94d6a4987e728b83bcc509096071dbb86067460d746
```

头部：`ident=600, recog=5001(MakeIndex), param=0, tag=0, series=0`。
`raw_len = 17 + body_len + len(tail) = 24`（与真机 600/601 帧同口径：
`'#' + EDCode(12B 头) + EDCode(body) + '!'` = `1+16+6+1`，加 1B 线缆前缀偏移）。
`t_ms`/`seq` 是驱动器标注，进全局掩码，不参与判定。

两侧（csharp/rust）内容**逐字节相同**：这是构造夹具的必然结果——600 的头部与
body 在当前判据下全部确定（无墙钟/RNG/会话成分），夹具用于钉死比较器的
**分类与逐字节判定路径**，不用于证明两侧实现一致。两侧实现差异由 `compare_runs`
在真机窗口抓帧上暴露；本卡的真实性锚点是上表的 oracle 真机头部向量与两侧源码。

重新生成（与库内副本逐字节一致）：

```sh
python -I - <<'PY'
import json, hashlib, importlib.util
spec = importlib.util.spec_from_file_location('m2f', 'tools/probe/mir2frame.py')
m = importlib.util.module_from_spec(spec); spec.loader.exec_module(m)
name = '木剑'; plain = name.encode('gbk'); wire = m.encode_bytes(plain)
assert m.decode_bytes(wire, strict=True) == plain
for side in ('csharp', 'rust'):
    row = {"seq": 1, "t_ms": 3001, "phase": "gamegate", "conn": "gamegate",
           "ident": 600, "recog": 5001, "param": 0, "tag": 0, "series": 0,
           "body_len": len(wire), "body_hex": wire.hex(),
           "body_sha256": hashlib.sha256(wire).hexdigest(),
           "frame_class": "protocol", "tail": "!", "raw_len": 17 + len(wire) + 1,
           "body_text": name}
    open('tools/conformance/fixtures/dropitem_ack/%s/frames.ndjson' % side,
         'w', encoding='utf-8', newline='\n').write(
        json.dumps(row, ensure_ascii=False) + '\n')
PY
```

## 范围

只冻结 600。同 send 点的 `SM_DROPITEM_FAIL(601)` 同形但**未登记**
（`window-run-2` 自检 missing 只含 600）；`rules.json` 仍把它判 UNCLASSIFIED
fail-closed，属下一张卡的显式缺口，见报告 §5。
