# 652（SM_SENDDETAILGOODSLIST）编码层 A1 钉定 —— oracle=双层，Rust 不改行为

- 卡：`openmir2-gm-detail-652-single-layer`（tier=R，high；owner `openmir2-dev-1`）
- 日期：2026-10-30
- 基线：walgit/master `6638fae46d81ca559e2114667c53b1a9999ae674`
- 结论一句话：**oracle 线上 652 尾本来就是双层 EDCode（内层逐件一次 + 外层整串一次），Rust 现状与之逐字节同构；
  卡片前提「oracle 真机=单层」被 A1 证否 ⇒ 不做单层翻转**。A2 的括号约定「以 A1 钉定为准」据此落为“不改线缆”。

## 1. A1：oracle 652 发送链逐层钉定（每层一次/零次编码）

| # | 位置 | 动作 | 编码 |
| --- | --- | --- | --- |
| 1 | `src/M2Server/Player/PlayObject.Operate.cs:210-211` | `CM_USERGETDETAILITEM` → `merchant.ClientGetDetailGoodsList(this, sMsg, nZz)` | — |
| 2 | `src/GameSrv/Npc/Merchant.cs:1339` | `sSendMsg = sSendMsg + EDCode.EncodeBuffer(clientItem) + "/"`（逐件，≤10 件） | **内层：`EncodeBuffer` 一次/件** |
| 3 | `src/GameSrv/Npc/Merchant.cs:1349` | `playObject.SendMsg(RM_SENDDETAILGOODSLIST, 0, ActorId, nItemCount, nInt, sSendMsg)` | 入队原样 |
| 4 | `src/M2Server/Actor/BaseObject.Message.cs:82-94` / `:344-351` | `Buff = sMsg`；出队 `msg.Msg = sendMessage.Buff` | **零次编码（原样传递）** |
| 5 | `src/M2Server/Player/PlayObject.Message.cs:1830-1831` | `SendDefMessage(SM_SENDDETAILGOODSLIST, nParam1, nParam2, nParam3, 0, processMsg.Msg)` | — |
| 6 | `src/M2Server/Player/PlayObject.Socket.cs:27-42` | `SendDefMessage(…, string sMsg)` → `SendSocket(ClientMsg, EDCode.EncodeString(sMsg))` | **外层：`EncodeString` 一次/帧** |
| 7 | `src/M2Server/Player/PlayObject.Socket.cs:101-129` | `HUtil32.GetBytes(sMsg)` 后原样拼进帧体尾 | 落线 |

⇒ **oracle 线上尾 = GBK(EncodeString(EncodeBuffer(item1)+"/"+…+EncodeBuffer(itemN)+"/")) = 双层。**

唯一发射点核实（全树 grep，`src/` 内，实测输出）：

```
$ grep -rn "SENDDETAILGOODSLIST" src --include="*.cs"
src/BotSrv/Player/RobotPlayer.Operate.cs:1227          （BotSrv 客户端侧接收，非发射）
src/GameSrv/Npc/Merchant.cs:1349                       （RM 发射，见上第 3 步）
src/M2Server/Player/PlayObject.Message.cs:1830          （case 分支，见上第 5 步）
src/M2Server/Player/PlayObject.Message.cs:1831          （泵→SendDefMessage，见上第 5 步）
src/OpenMir2/Messages.cs:314                           （SM 常量 652）
src/OpenMir2/Messages.cs:595                           （RM 常量 10135）
```

部署与源码一致性（R 级证据根基）：

```
$ "E:\MirServer\M2GameSvr\GameSrv.exe" VersionInfo.ProductVersion
1.0.0+1f5132b2c63a6698b0669879af614403f49b942c      # 2026-10-22 构建自本仓 commit 1f5132b2c
$ git diff 1f5132b2c63a6698b0669879af614403f49b942c HEAD --stat -- src/GameSrv/Npc/Merchant.cs src/M2Server/Player/
 src/GameSrv/Npc/Merchant.cs | 25 ++++++++++++++++---------
 1 file changed, 16 insertions(+), 9 deletions(-)      # 全部是 RefillGoods 节拍（T15），不涉 652 链
```

即 **oracle 真机二进制与当前 master 在 652 发送链上逐字节同语义**（第 2/3/5/6 步文件均无 diff）。

外部核验（本次实测）：

- 部署二进制 `sha256 = ab6550080736e6610c86de226751c66a319e61e6be2176630a2374ea5819392f`
- `git cat-file -t 1f5132b2c…` = `commit`；`1f5132b2c 2026-10-22 01:12:42 +0800`

## 2. 对照 645（RM_SENDGOODSLIST）——同为外层一次，但内无预编码

`Merchant.cs:987`：`sSendMsg = sSendMsg + sName + '/' + nSubMenu + '/' + nPrice + '/' + nStock + '/'`（**纯文本**），
再走同一个 `SendDefMessage`（第 6 步）⇒ 线上尾 = `EncodeString(纯文本)` = **单层**。
所以「旧栈/新栈 645 同构」只证明外层 `EncodeString` 两侧实现一致，**不涉及 652 的内层**。

## 3. 卡片「oracle 真机=单层」的两条证据都不判别层数

1. **「旧栈药品 StdMode≤4 短路成功购买」**：买入门 `Merchant.cs:1249`
   `if (stdItem.StdMode <= 4 || stdItem.StdMode == 42 || stdItem.StdMode == 31 || userItem.MakeIndex == nInt)`
   —— StdMode≤4 分支**根本不看客户端的 `nInt`**，所以药品能买成功与 652 的层数无关。
2. **「旧栈/新栈 645 同构」**：见 §2，只覆盖外层。

## 4. 冻结客户端真身：**两遍解**（E:/Users/gxh/Documents/GitHub/MirClient）

`Source/MirClient/ClMain.pas:13594-13639`（`ClientGetSendDetailGoodsList`）：

```pascal
bodystr := DecodeString(bodystr);                      // :13606  第一遍：撤外层 EncodeString
while True do begin
  bodystr := GetValidStr3(bodystr, data, ['/']);        // :13611  按 '/' 切件
  if data <> '' then begin
    New(pc);
    DecodeBuffer(data, @(pc^), SizeOf(TClientItem));    // :13615  第二遍：撤内层 EncodeBuffer
    g_MenuItemList.Add(pc);
  end else break;
end;
```

⇒ 双层线缆**正是冻结客户端期望的形态**。BotSrv 里被注释的同款移植（`src/BotSrv/Player/RobotPlayer.cs:6151-6192`）
也是两遍（`DeCodeString` + 逐段 `DecodeBuffer`）。

## 5. 651/param=0 的真因（不是编码层，与上张卡分支 c 一致）

服务器族 `ClientItem` = **124B**（`src/OpenMir2/Packets/ClientPackets/ClientItem.cs:25-89` 的 `WritePacket`，
`ClientPacketConst.ItemNameLen = 14` ⇒ `Name` 15B、`MakeIndex@100`、`Dura@104`、`DuraMax@106`、`Desc@108..121`；
Rust 侧 `itemwire::encode_client_item` 同布局）。冻结客户端：

- `Source/Common/Grobal2.pas:112` `ItemNameLen = 20`（本冻结树**无条件**；`mir2-ref/BLUEM2/.../Grobal2.pas:46-50`
  是 `{$IF VER_176} 14 {$ELSE} 20`，两条分支正是历史遗留的双口径）
- `Grobal2.pas:1981-2010` `TClientStdItem` = 116B（`string[20]`=21B，`Eva`=`TEvaluation` 26B）
- `Grobal2.pas:3507-3512` `TClientItem` = `TClientStdItem(116B) + MakeIndex@116 + Dura@120 + DuraMax@122`
  ⇒ 总长同为 **124B**，但 **`MakeIndex@116`**

客户端把解出的 124B 按自家结构 cast：`MakeIndex` 读到服务器的 `Desc[8..11]`（全零区）⇒ 0 ⇒
`CM_USERBUYITEM` 的 `param=0` ⇒ 武器 `MakeIndex == nInt` 门不过 ⇒ `651 recog=1`。
**同一份 652 字节发给 C# 参照栈结果相同**（上张卡 `openmir2-gm-buy-stock-instances` 已定性并合入 `e879a0afb`）。

> MirClientZig 字节表的「双层解码后 @100=55432」正是**两遍解**的恢复结果——它证明实例能解出来，
> 不证明客户端解析失败；真正读错的是后续的**结构体字段偏移**（116B 口径）。

## 6. 判定

- **不改线缆、不改布局**（卡片 forbidden 亦明确不动 `itemwire` 的 124B 布局）：Rust 现状 = oracle 逐字节等价，
  651 现象在 oracle 侧同样发生 ⇒ 是「行为等价优先于修 bug」下的忠实复刻。
- 「让冻结客户端买得动」须偏离 oracle 线缆结构（按客户端 116B 口径重排 `ClientItem` 或改用客户端布局），
  属 D 类（客户端冻结优先 vs oracle 等价，参照 whitelist `D-9` 先例），**另卡另裁**，不在本卡夹带。
- 文档口径已订正：`shop.rs` 模块头新增「652 编码层 A1 钉定」节，`detail_goods_list` 尾口径注释补齐
  内层/外层与客户端两遍解的行号锚；原注释里的「与 `msg.Ident` 的 RM→SM 双重编码口径」一句已替换为
  具名分层描述（A1 证明原说法**在结论上正确、在表述上易被误读为“多编了一层”**）。

## 7. A3 牙（构造级，已跑）

`mir2-rs/bins/game-svr/src/shop.rs::tests::detail_goods_list_652_wire_is_double_edcode_and_two_pass_recovers_instance`

- 从**真发射面** `crate::shop_frame_body`（宿主 `emit_shop_frames` 同一函数）取线上帧体，`body[12..]` = 尾；
- 第一遍 `edcode::decode(wire_tail) == frame.tail`（外层恰好一遍）；且 `len != 124`、仍含 `'/'`
  —— 反证**单遍必出坏帧**；
- 第二遍按 `'/'` 切件再 `edcode::decode` ⇒ 124B、`MakeIndex@100 == 55432`（非零，字节表实证值）；
- 变异（两个单层方向，任一必红）：
  - 去掉内层 `EncodeBuffer`（尾 = 裸 124B + `'/'`）⇒ 第二遍解 ≠ 124B ⇒ 红；
  - 去掉外层 `EncodeString`（`tail_encoded=true`）⇒ `decode(wire) != tail` ⇒ 红。

> 卡片原拟的 A3（「**单遍**解码后 `MakeIndex@100` 可读」）在本口径下不可满足——它要求的正是被 A1 证否的
> 单层线缆形态。交付牙在两遍恢复上钉死同一组数值（@100=55432），并多钉了外层一遍。

## 8. 余量 / 真机项（tier=R，`window_required: true`）

代码侧无余量；**真实机判别项**留给 MirClientZig 用同一命令在**旧栈（C# oracle）**上重跑
「点开卫家店-0103 木剑详情 → 买入」，预期与**新栈**观测一致：客户端出站 `1014 param=0`、服务端回 `651 recog=1`
（若旧栈出现不同结果，则推翻本文 §5 并在卡片线程回传证据，再重开裁决）。

## 9. 残留物清单

- 无临时文件：仅改 `mir2-rs/bins/game-svr/src/shop.rs`（注释 + 1 条测试）与新增本文件；
- 未启动任何服务端/客户端/假人进程，`E:\MirServer` 十端口状态与开工前一致；
- 未改 `src/**`（C#）、`sql/**`、`itemwire` 布局与 645 路径。

## 10. 改后哈希（本交付）

- `mir2-rs/bins/game-svr/src/shop.rs` sha256 `9122d700a2df10ba894ae2337cc7949ca960d600a91949b0c688f1920df3a5c4`
- 冻结客户端参照：`ClMain.pas` sha256 `ac6ca85722ab9d67ca66a27bc443532ff311f37674737b5c5a152642724b8bd4`、
  `Grobal2.pas` sha256 `b91816afe405b55c95c1f525ec3a67e019344c12d8771c91d72df270c30cb40b`
- 部署 oracle 二进制：`ab6550080736e6610c86de226751c66a319e61e6be2176630a2374ea5819392f`
