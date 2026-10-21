# 窗口第二轮实跑记录（2026-10-21，受控登录对拍）

> 授权：E1（测试库克隆）/E2（影子内容）/E4（允许重编）已由 owner 裁决；外部 C# 栈 owner 已同意停止。
> 本记录只覆盖**受控登录**一个场景；其余组未跑。

## 0. 基线与拓扑

| 项 | 值 |
| --- | --- |
| C# oracle | 部署件影子包（`E:/MirServer` 只读复制；部署 `!deployed-from.json git_head=8a621f6a`）。副本三处连接串分别指向 `mir2_test_data`（GameSvr `Server.conf`）、`mir2_test_db`（C# DBSrv `dbsvr.conf`）、`mir2_test_account`（C# LoginSrv `logsrv.conf`） |
| Rust | `ab44031ce`（`window-provenance.json source_head=ab44031ce59f40169fe3d473e267cba54aa8d82b`，六件） |
| 测试库 | 两侧各跑前从生产库 `mysqldump --single-transaction` 重新克隆；`promlist` 实测只连 `mir2_test_*` |
| 拓扑 | 影子 GameGate 7600 + `mir2_proxy` 7200→7600；受控流程账号 `winplay0`，角色摆到 `(289,618)`、Level 30、HP 500/500 |
| 客户端驱动 | `mir_flow.ps1 -WalkClicks 0 -AutoPlaySec 0 -AttackClicks 0 -SkipReenter -SkipLogout` |
| 证据 | C#：`E:/tmp/window/20261023/csharp/proxy/`；Rust：`E:/tmp/window/20261023/rust/proxy/`；报告 `E:/tmp/window/20261023/compare.json` |

## 1. 入图 15 帧：GREEN

`tools/loginfixture/capture_login_compare.py`：
`RESULT GREEN: 15/15 帧，0 非白名单差异（whitelisted=5）`。
白名单只有两类已知项：`recog`（C# 大整数 vs Rust 序号）与 `52` 布局（**40 vs 58**，D-9/client58）。

⇒ **登录前缀在真机上逐帧一致**（除已登记的两类）。

## 2. `654` 位置：修复确认

- C#：6 对 `10/41` 之后 → `654`（idx 27）；
- Rust：5 对 `10/41` 之后 → `654`（idx 25）；
- 字段：`Recog=自己, Param=0, Tag=500, Series=0`，两侧相同。

⇒ `lane654` 的「654 推迟到首次 `SearchViewRange` 之后的同一 tick」在真机成立；
且出现对数量 5 vs 6 时 `811,46` 均保持正确 ⇒ **FixedHideMode 幻影槽修复在 N≠0 下成立**。

## 3. 第二 `42` 与 `100×4`

- 第二 `42`：两侧 body **逐字节相同**；C# 位于 `654,657,657` 之后，Rust 位于 `654` 之后（因缺 657，见 §4）。
- `100`：两侧均为 **4 条**；`Param=65499, Tag=0, Series=1` 相同；**正文集合相同、顺序不同**
  （C#：客服QQ→欢迎→禁止外挂→攻击模式；Rust：攻击模式→…→客服QQ）。
  顺序不确定已登记（观察者队列同优先级堆序，两次真机本身也不一致）⇒ 不作为判据。

## 4. 新缺陷（W2-1）：Rust 登录路径缺 `657×2`

- C# 在 `654` 之后发 **两条 `SM_CHARSTATUSCHANGED(657)`**（`Recog=自己, Param=0, Tag=0, Series=0`）；
- Rust **一条都没有**。
- 第一次窗口（2026-10-22，旧二进制）C# 同样有 `657,657`，Rust 同样没有 ⇒ 与本轮二进制更新无关，是持续缺口。
- 处置：开卡 `openmir2-login-charstatus-657`（tier=R），对照 `RecalcAbilitys` 的 `StatusChanged()` 自回环与
  `SendUpdateMsg(RM_CHARSTATUSCHANGED)` 两条来源，补登录路径的两帧并加牙。

## 5. 其它观察

- 出现对内容两侧不同（位置/数量）——怪物漫游与两侧世界状态不同所致；不作为缺陷，但说明
  `10/41` 对数量是**非确定性**的，判据必须按「811/46 相对序」而不是按对数。
- `50/51` 的 `param` 本轮两侧**相等（289）**。上一轮 289/288 的差在两侧 DB 重置后消失
  ⇒ 确系输入漂移（角色位置不同），不是代码分叉。
- 已知并保留：`52` 40/58 布局差（D-9）；recog 口径差。

## 6. 收窗

- 代理 7200、影子 gate 7600、LoginGate、Rust 主栈全部停止；九游戏端口 DOWN，仅 `mysqld:3306`。
- 测试库保留（`mir2_test_*`，已与生产克隆一致），C# 侧未写生产库。
- 证据目录保留：`E:/tmp/window/20261023/{csharp,rust}/`。
