# openmir2-runtime-sysmsg-text-frames

> 状态：**登记卡（2026-11-20，opc-07 发布）**。触发：IT-14（修复油）设计时确认——夹具记录面能"对拍通过"，但**运行期宿主不会按 oracle 发出带文本的系统消息帧**，玩家看到的提示缺失。
> 关联：`docs/issues/openmir2-items-parity.md`（IT-14/IT-15）、C2 之前的 `StdMode1` 食物同类缺口

## 1. 现状（事实，带锚）

- oracle：`SysMsg(<文本>, Green, Hint)` → `SendMsg(RM_SYSMESSAGE, 0, GreenF, GreenB, 0, "<文本>")`（队列通知）。
- 宿主：`QueuedNotice` 是**5 字段定长**（ident + 4 个数字参数），`notice_frame` 的 `RM_SYSMESSAGE` 走 `session.sys_msg`（**静态文本**）。
- ⇒ 夹具层面：`notices` 只记 `ident + 颜色参数`，**不含文本** ⇒ **对拍能通过**；运行期层面：**这句话不会按 oracle 发**，玩家看不到。
- 已确认同类：**StdMode1 食物** 的 `RM_SYSMESSAGE` / `RM_CHARSTATUSCHANGED`、**Shape9 修复油（IT-14）** 与 **Shape10 特修油（IT-15）** 共用的 `WeaponRepairSuccess` 绿字、**Shape11 彩票（IT-16）** 的 `WinLottery{N}Msg`（绿）与 `NotWinLotteryMsg`（红）。

## 2. 目标

让"带文本的系统消息帧"在**运行期**与 oracle 一致：送达同一收件人、带同一文本与颜色，且**进了比较面**（不能只有夹具层面通过）。

## 3. 可选路径（需评审取舍）

1. **扩 `QueuedNotice` 携带字符串**：破"5 字段定长"不变量，牵动 `pickwire` 字面量与多处构造点——影响面大但语义直接；
2. **旁路表**：`QueuedNotice` 只带 `ident + 消息 ID`，文本由宿主按 ID 从配置/常量表还原——不改定长结构，但要求 oracle 文本与宿主表逐字一致（需落表并机检）；
3. **只登记不实现**：明确"运行期缺文本帧"，覆盖率口径中该条**不计入 verified**（宁少不多）。

## 4. 验收

- 任一方案落地后，**至少一条"运行期帧"证据**：同一场景下宿主实际下行帧含 oracle 的同文本与颜色（不是只看夹具 notices）；
- 若选路径 3，则把受影响的条目从 verified 降级并在报告里给数字变化（**代价接受**）。

## 5. 不做

- 不在比较器里"忽略文本"当作已覆盖；
- 不把"夹具对拍通过"表述成"运行期一致"。

## 6. 实施（2026-11-20，`card/sysmsg-text`）

- **数据面**：`QueuedNotice` 增 `text: NoticeText { None, Inline(String), SessionPickup }`。
  `Inline` 由调用点给出（禁药红字、Shape9/10 修复油绿字、彩票 `WinLottery{N}Msg` / `NotWinLotteryMsg`）；
  `SessionPickup` **只服务取件一条**（`ground::pick_up` 拿不到配置）；`None` 不产帧。
  **不做全域回退**——否则会话里留着的取件文案会串到吃饭/彩票帧上。
- **文本源**：`strings.rs` 新增九个 `[String]` 键，键名逐字照 `StringConf` 的读键，
  缺键回落 `MessageSettings` 源码默认（**逐键**回落）。编码路径只有一条：
  配置层 cp936 解码一次、出帧 `gbk()` + EDCode 一次。
- **比较面**：`eat_vectors.json` 的 `notices[].text` 存 oracle `EDCode.EncodeString(Buff)` 的
  **原始字节 hex**；Rust 侧对每条 `RM_SYSMESSAGE` 逐字节比（多重集 + 反空集断言），
  另有整帧全等用例（12B 头 ++ 文本尾），彩票批每例恰好一条 SysMsg 且档位与文案一一对应。
- **夹具重生成**：必须 `MIR2_RNG_SEED=20261120`（不设则彩票块为空，会静默抹掉已有 12 例，
  见 `eat-probe/README.md`）；`config.exps_conf` 只记 basename（绝对路径不可复现），
  Rust 侧另有显式归一守卫用例。

## 7. 结果

- `cargo test -p game-svr --lib`：**2078 passed / 0 failed / 61 ignored**；
  五条牙全部红并可还原：忽略 `Inline` ⇒ 彩票帧序缺 `100`；全域回退 ⇒ 负例红；
  `sysmsg_text` 丢文本 / 文本改一字节 ⇒ 逐字节比红；`Inline` 不做 gbk ⇒ 整帧对拍红；
  夹具写回绝对路径 ⇒ 归一守卫红。
- **IT-14 / IT-15**：通知文本字节与 oracle 相同 ⇒ 可升 `verified`（待 QA 核其余既有条件）。
- **IT-16 不升**：`RM_CHARSTATUSCHANGED(10139)` 仍未建模、`IncGold` 失败支无 oracle 证据 ⇒
  保持 `implemented_unverified`。

## 8. 登记缺口（不在本卡）

- `RM_MENU_OK` 的尾文案 `MessageSettings.CanotUseItemMsg`（`use_item` / drop 守卫 / 脚本三处
  同 ident）——夹具照实记录，比较器以**集合白名单 `{RM_MENU_OK}`** 钉住：未建模，但不静默。
- `move_eat_vectors.json`（e1 车道的 `move-eat-probe`）需同步 `notices[].text` schema。
