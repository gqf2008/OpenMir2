# compare 规则表登记 ident 211（SM_SENDMYMAGIC）— 交付报告

卡：`openmir2-compare-ident211-classification`（tier=L）· owner：openmir2-worker-74 ·
分支：`card/compare-ident211-classification`（base = walgit/master `da519ab04`）· 2026-10-24

## 1. ① C# 源取证（211 语义）

| 项 | 证据 |
| --- | --- |
| 常量名 | **`SM_SENDMYMAGIC = 211`**（`src/OpenMir2/Messages.cs:225`；同号 `SS_211`/`ISM_CHANGECASTLEOWNER` 是上行/服务器间命名空间，非 s2c 语义） |
| 触发 | 登录/魔法变更：`src/M2Server/Player/PlayObject.cs:1390` `SendPriorityMsg(RM_SENDMYMAGIC, 0,0,0,0, "", High)`（RM_SENDMYMAGIC=10122，Messages.cs:584）→ `src/M2Server/Player/PlayObject.Message.cs:1797-1800` `case RM_SENDMYMAGIC: SendUseMagic()` |
| 发送点 | `src/M2Server/Player/PlayObject.Base.cs:1052-1070` `SendUseMagic()`：`MakeMessage(SM_SENDMYMAGIC, 0, 0, 0, (short)MagicList.Count)`（:1067）+ `SendSocket(ClientMsg, sSendMsg)`（:1069）；列表空 ⇒ 整帧不发（IsNullOrEmpty 门） |
| 头部 | recog=param=tag=**0（源码常量）**；series=**MagicList.Count（语义值）** |
| body | 逐条 `EDCode.EncodeBuffer(ClientMagic) + '/'`（:1063-1065）。ClientMagic 84B/条（`ClientMagic.cs` WritePacket：Key(1)+Level(1)+pad(2)+CurTrain(4)+MagicInfo 76B；`MagicInfo.cs` WritePacket：MagicId(2)+MagicName(1+14)+EffectType(1)+Effect(1)+pad(1)+Spell(2)+Power(2)+TrainLevel(4×1)+MaxTrain(4×4)+TrainLv(1)+Job(1)+pad(2)+DelayTime(4)+DefSpell(1)+DefPower(1)+MaxPower(2)+DefMaxPower(1)+Desc(1+15)+pad(3)；WriteAsciiString=len 字节+定长区） |
| 尺寸自洽 | 84B → EncryptUtil.Encode（3B→4 字符）112 字符 + `'/'` = **113** = 冻结 fixture seq18 `body_len`（且 body_hex 末字节 `2f`=`'/'`） |
| Rust 同形 | `mir2-rs/bins/game-svr/src/smframe.rs:225-231` `sm_sendmymagic(count, tail)`，doc 注释逐字引用 C# 构造 |

**结论**：body 是魔法库静态定义 + 玩家持久化状态（Key/Level/CurTrain + MagicInfo），**无墙钟/时间戳/会话成分** ⇒ 不满足「噪声帧」例外，登记为**全字节比**（不是 ignore/掩码）。

## 2. ② 规则登记

`tools/conformance/rules.json` 追加（第 46 条，append 位）：

```json
{"ident": 211, "action": "compare", "tier": "L",
 "reason": "SM_SENDMYMAGIC(211) 是真实协议帧（Messages.cs:225）……（全文见 rules.json）",
 "approved_by": "PENDING owner/coordinator review (draft: openmir2-worker-74)"}
```

不加 `head_compare`：recog 恒 0，全局 recog 掩码对它无副作用（同 635/636 先例）；series 语义值照常比。冻结 fixture 上两侧 211 帧各 1 条（**seq18 / 行 18**，`body_sha256` 同为 `bd17f4f6…`）：

**比对结果：UNDECIDABLE → 已比，0 diff**；报告 rc=2→**rc=1**，残余 diffs 恰为已知真差异 **ident 53 ×3**（frame 30/32，body_len/tag；归 SM_HEALTHSPELLCHANGED 卡，该红仍红，本卡不许也不曾掩掉）。`ignored_ident_rules` 仍 ==[14] 不变。

`selftest_impl.py`：旧断言「⑤ 唯一 UNDECIDABLE = ident 211 未分类」按其自预留的更新义务替换为 ⑥ 三断言（211 已入表 action=compare；211 帧已比 0 diff 且 UNDECIDABLE/unclassified 清零；残余 diffs=ident 53 ×3 该红仍红）。

## 3. ③ 牙（均实测、均还原）

| 牙 | 变异 | 实测 |
| --- | --- | --- |
| A：删 211 规则 | rules.json 移除 211 条目（备份后改写） | selftest **RED 293/295，rc=1**；`控制帧 ⑥ 已入规则表` FAIL `(2, [])` + `⑥ UNDECIDABLE 清零` FAIL，`unclassified_ident ident=211` 复现 ⇒ 回到 unclassified。还原后 GREEN 295/295 |
| B：改坏 211 body 一字节 | rust 侧行 18 `body_hex` 首字节 `61→62`，`body_sha256` 同步重算（保证到达比较器） | selftest **RED 293/295，rc=1**；diff 精确定位：`ident 211, frame_index 17, kind body, offset 0, oracle_byte 97('a') vs rust_byte 98('b'), diff_bytes 1`。`git checkout` 还原后 GREEN 295/295 |

## 4. ④ 门禁（全部在本卡 head `8eb089a2f` 实跑）

| 门禁 | 结果 |
| --- | --- |
| `python -I -B tools/conformance/compare_runs.py selftest` | **GREEN 295/295，rc=0**（承前 293 ⇒ +2，未减少） |
| `python -I -B tools/conformance/card_window.py --selftest` | **GREEN 255/255，rc=0** |
| `python -I tools/hygiene/conflict_marker_gate.py` | OK（0 markers / 7486 tracked files），rc=0 |
| `python -I tools/hygiene/stale_pointer_gate.py` | OK，rc=0 |
| `python -I tools/hygiene/dynamic_count_stamp_gate.py` | OK（0 缺戳），rc=0 |
| `git diff --check` | 干净，rc=0 |
| `git merge-tree --write-tree walgit/master HEAD` | 干净（tree `484f05d31e…`），rc=0 |
| Rust 零变更 | `git diff --stat da519ab04..HEAD` 仅 `tools/conformance/rules.json`、`tools/conformance/selftest_impl.py`、本文档；card_window --selftest 在 amend 后 head 复跑 GREEN 255/255；无 `mir2-rs/**`/`src/**` 路径 ⇒ fmt/clippy/test 承前 master 基线（`da519ab04` 合并门禁全绿），本卡未触碰 Rust |

## 5. 写集与残留物

- 改动：`tools/conformance/rules.json`（+7 行）、`tools/conformance/selftest_impl.py`（⑤注释+⑥断言）、本文档。
- 未碰：场景 JSON、`src/**`、服务端 `rules.json`/`whitelist.md`、`card_window.py`（worker-76 夹具段未冲突）、`compare_runs.py`（登记机制无需改）。
- 残留物：`E:/tmp/w74-scratch/`（rules/frames 备份与 scratch 报告，纯临时，可整目录删除）。
- 批准状态：211 条目 `approved_by` 为 PENDING，未自批；批准归 owner/coordinator。
