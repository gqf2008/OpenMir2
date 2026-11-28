# UserLogon 登录门未移植批（总监审计 2026-11-28）

## 来源与结论

MG-35 ① 收口审计（总监 opc-66，2026-11-28）对 oracle `PlayObject.UserLogon()`
（`src/M2Server/Player/PlayObject.cs:1152`）逐分支对照 Rust 侧，发现 **5 个分支 Rust 无对应物**
（grep 证据：相关配置键/常量在 `mir2-rs/bins/game-svr/src` + `mir2-rs/crates` 均为 0 命中，
master `34f27ac79` 时点）。登记本卡，一次批实现。

## 五个门

| # | oracle 锚 | 语义 | 配置键（ServerConf.cs 同名段） | 缺省 | 触发条件 |
| --- | --- | --- | --- | --- | --- |
| G1 | `src/M2Server/Player/PlayObject.cs:1322-1331` | 版本比对踢人：`!TryPlayMode && SoftVersionDate < Config.SoftVersionDate` ⇒ 3 条红 SysMsg（`ClientSoftVersionError`/`DownLoadNewClientSoft`/`ForceDisConnect`）+ `BoEmergencyClose=true; return` | `[Setup] SoftVersionDate` | 20020522（`src/Modules/SystemModule/Conf/Model/GameSvrConf.cs:1437`） | 客户端上报版本 < 20020522（真客户端 120020522 不触发；旧版/篡改客户端触发） |
| G2 | `src/M2Server/Player/PlayObject.cs:1332-1341` | 老客户端分支：`SoftVersionDateEx == 0 && boOldClientShowHiLevel` ⇒ 蓝 `ClientSoftVersionTooOld` + 红 `DownLoadAndUseNewClient`；若 `!CanOldClientLogon` 再 3 条红 + 踢 | `[Setup] OldClientShowHiLevel` / `[Setup] CanOldClientLogon` | true / true（`src/Modules/SystemModule/Conf/Model/GameSvrConf.cs:1746` / `:1438`） | 版本 == 0 客户端**且门槛 = 0**。**卡面订正（2026-11-28 实现期发现，总监复核裁定）**：`SoftVersionDateEx` 与 `SoftVersionDate` 同源同值（`src/GameSrv/Word/WorldServer.cs:468-469`），且 G1 先于 G2——门槛 > 0 时 version-0 客户端必被 G1（`:1324`）先踢 return，**G2 仅在「无版本要求」（门槛 0）部署可达**；原「缺省配置下只发两条不踢」的描述有误（那要求 version-0 能走到 `:1332`，缺省门槛下不可达）。实现侧对应处置：`soft_version_gate_and_config_frames` 旧测夹具门槛改 0 保住 `Ex==0` 关门面前提（断言未动） |
| G3 | `src/M2Server/Player/PlayObject.cs:1376-1383` | 在线人数门：`PlayObjectCount > Config.TestUserLimit && Permission < 2` ⇒ 红 `OnlineUserFull` + 红 `ForceDisConnect` + `BoEmergencyClose=true` | `[Server] TestServerUserLimit` | 1000（`src/Modules/SystemModule/Conf/Model/GameSvrConf.cs:1258`） | **普通玩家即达**（在线 > 上限且权限 < 2）。`Permission` 来源 `GetHumPermission`（`src/GameSrv/Word/WorldServer.cs:1022-1037`）：缺省 `StartPermission`（0，`[Setup] StartPermission`），AdminList 命中则用 admin 级 |
| G4 | `src/M2Server/Player/PlayObject.cs:1402-1415` | 体验模式块：`PayMent == 1 && !TryPlayMode` ⇒ 红 `YouNowIsTryPlayMode` + `GoldMax = HumanTryModeMaxGold`；`Abil.Level > TryModeLevel` ⇒ 红×2 + 踢 | `[Setup] HumanTryModeMaxGold` / `[Setup] TryModeLevel` | （实现时按 `src/Modules/SystemModule/Conf/Model/GameSvrConf.cs` 缺省逐个取） | PayMent==1 且非 TestServer/ServiceMode 部署（dev 栈 TestServer/ServiceMode ⇒ PayMent 强制 3，`mir2-rs/bins/game-svr/src/enter.rs` 的 `apply` 已接，该分支死） |
| G5 | `src/M2Server/Player/PlayObject.cs:1434`（`CheckDenyLogon` 在 `src/M2Server/Player/PlayObject.Base.cs:3387-3405`） | 禁止登录名单：IP / Account / ChrName 三张名单逐个查，命中 ⇒ 红 SysMsg（`YourIPaddrDenyLogon`/`YourAccountDenyLogon`/`YourChrNameDenyLogon`）+ 直接 return（不走后续登录序） | 名单文件（实现时核 `M2Share.GetDeny*List` 装载点） | 空 ⇒ 不触发 | **卡面订正（2026-11-28 实现期核实，oracle 死结构）**：`src/M2Server/M2Share.cs:1591/1654/1711` 三个 `GetDeny*List` 的比较体**全部注释**（for 扫描在、比较注释 ⇒ 恒 false），装载 `LoadDeny*List`（`:1564` 等）读文件体同样全注释 ⇒ `CheckDenyLogon` **恒 false，本门在 oracle 任何部署不可达**（BUG-REPLICA 死结构家族，与 CloseSpeedHackCheck 门/状态衰减循环同型）。Rust 同构复刻：扫描发生（名单可注入非空）、比较注释 ⇒ 恒 `None`；装载函数不读文件；命中支机制（槽 17/18/19 + emergency + return 截断 + `@Login` 守卫）由注入测试可达并配反向牙 |

## 优先级与验收关联

- **G3 优先**：口径 A（单分片 5000–8000 在线）下 oracle 忠实部署**必须**调高
  `TestServerUserLimit`（≥ 8000）或给压测号权限 ≥ 2，否则 oracle 会踢人。Rust 侧若不实现该门，
  两侧在 >1000 在线时行为分歧（Rust 不踢、oracle 踢）。**验收部署清单同步加一条**：
  压测配置 `[Server] TestServerUserLimit` 必须 ≥ 目标在线数。
- G1/G2/G4/G5 均为缺省配置不可达或配置面可达，忠实实现 + 单测牙即可。

## 已核对为已覆盖（不在本卡范围，勿重复实现）

- 攻击模式 SysMsgs + `StartChangeAttackModeHelp` + TestServer 欢迎 + PayMent==3 免费模式
  （`mir2-rs/bins/game-svr/src/enter.rs` 的 `apply` 已接线，3+1 条）
- PayMent==3 强制（`mir2-rs/bins/game-svr/src/enter.rs` 的 `apply`）
- VentureServer / ERGUM(+LNG) / boNORECONNECT（各自已有接线）
- RM_LOGON 50/41/213、52/752、46、658、11029/5008/1106、811、766、54、42 等
  （`docs/M2顺序判据②-对账台账.md` §3 全绿）
- TestServer TestLevel/TestGold 提升、`PlayObjectCount` 判定所用计数口径：实现时核对
  `UserLogon` 开头（`:1155-1175`）Rust 侧现状，若已有则只补缺

## 附带澄清：MG-35 ① 遗留面闭合（2026-11-28 总监审计）

MG-35 清单 scope ①「`SoftVersionDateEx` 非 0 的生产值来源未在此叶对拍」**实际已闭合**：

1. 解析同构：`mir2-rs/bins/game-svr/src/cert.rs` 的 `get_certification` 与 oracle
   `GetCertification`（`src/M2Server/Net/ChannelMessageHandler.cs:237-289`）同构
   （`sMsg[1..]` 跳 0 → EDCode 解 → `**` 判 → `/` 切 7 段 → `StrToInt(ver, 0)`）；
2. 真包级证据：`mir2-rs/bins/game-svr/src/cert.rs` 的 `parses_oracle_login_body`
   （`ORACLE_LOGIN_BODY` 常量）用**真 C# GameGate 实测 98B 登录包**
   （`E:\tmp\openmir2-gamegate-imp\oracle-login-cap\out-csharp-raw.json`）钉死
   `client_version = 120020522`；
3. 生产装配：`mir2-rs/bins/game-svr/src/lib.rs` 的 `LoadDbInfo` 装配点
   `soft_version_date = cert.client_version`（oracle
   `src/M2Server/Net/ChannelMessageHandler.cs:329` `SoftVersionDate = nClientVersion` 对应）；
4. 全路径守卫：`mir2-rs/bins/game-svr/src/lib.rs` 的 `soft_version_date_paths_enumerated`。

⇒ 本卡 G1 就是该值链上唯一未接的**消费方**（版本比对踢人）。

## 收官（2026-11-28）

五门全部并入 master：G3 `ca57a7272` → G1 `5977d615a` → G2 `c77c62215` → G4 `4b628c900`（清洁 `0248c5e30`）→ G5 `7e53ee408`。每门独立验证链：双远端 sha 核对 + 祖先关系 + diff 全读 + detach 克隆钉死 head 独立复跑（fmt/clippy/workspace --lib 全量输出落盘 + 真 rc）。期间两处卡面可达性描述订正（G2、G5，均实现期核实 oracle 语义后由总监裁定留痕）。**部署侧**：容量档执行前 `[Server] TestServerUserLimit` 必须 ≥ 目标在线数（已入 `E:/OPC/docs/capacity-ladder-plan-20261124.md` §1.4）；`TestServer=TRUE` ⇒ PayMent 强制 3 ⇒ G4 现部署死支（配置面可达，实现忠实保留）。
