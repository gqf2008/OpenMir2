# recv_packet_null 同族 follow-up：`SM_TURN` / `SM_DISAPPEAR` / `SM_ABILITY` / `SM_WINEXP`

卡：`openmir2-botsrv-recv-null-residual`（同卡 follow-up；base = 已合并 master `c527e93f8`）。
写集：`src/BotSrv/**` + `docs/**`；**未改** census 类别定义与白名单。

上一卡已给 `SM_WALK/RUN/HORSERUN`、`SM_HIT` 族、`SM_BUTCH`、`SM_SITDOWN` 加门控。
本卡按同一方法论处理复审 P3-1 指出的「同一 defers-null 机制、本轮窗口有流量、尚未加门控」站点：
**先离线证明 `MShare.MySelf == null` 时该帧必 NRE，能抛才改；抛不出来写「不可证」，不改。**

行号均指 `src/BotSrv/Player/RobotPlayer.Operate.cs`（base `c527e93f8` 与修后各一列）。

## 1. 可证 / 不可证结论

| 站点 | ident（窗口计数） | base 解引用行 | 离线实测 NRE 行 | 结论 | 修后门控行 |
| --- | --- | --- | --- | --- | --- |
| `SM_TURN` 带名尾部 | 10 = 5,038,528 | 384 | **384** | 可证，已门控 | 386 |
| `SM_DISAPPEAR` | 30 = 6,279 | 846 | **846** | 可证，已门控 | 855 |
| `SM_ABILITY` | 52 = 500 | 632 | **632** | 可证，已门控 | 637 |
| `SM_WINEXP` | 44 = **0** | 658 | **658** | 可证，已门控 | 666 |
| `SM_DAYCHANGING` | 46 = 500 | 654-656 | **不抛** | **不可证**（空 case），不改 | 无 |

**对复审清单的一处更正**：清单把 `:658` 记在 `SM_DAYCHANGING(46)` 名下。源码
`src/BotSrv/Player/RobotPlayer.Operate.cs:654-656` 的 `SM_DAYCHANGING` 是**空 case**；
`:658` 的 `MShare.MySelf.Abil.Exp` 属于 `SM_WINEXP(44)`。离线喂 ident 46（`MySelf=null`）
不抛异常（`null-daychanging-empty-*` 两条用例在修复前后都 PASS），故 `SM_DAYCHANGING` 不改；
真正可证的 `:658` 已按 `SM_WINEXP(44)` 门控。窗口里 44=0、46=500，两者流量归属以 ident 为准。

## 2. 门控形态与等价语义

- `SM_TURN`：在 `ArrestStringEx(...)` 之后、`string.Compare(data, MySelf.UserName, ...)` 之前加
  `if (MShare.MySelf == null) break;`。旧代码在下一行抛 NRE，本帧此后的 SlaveObject 维护与
  `Actor.NameColor` 一并被丢弃；`break` 出去与「丢弃本帧剩余部分」等价。
- `SM_DISAPPEAR`：`if (MShare.MySelf != null && MShare.MySelf.RecogId != msg.Recog)`；
  MySelf 为空时跳过 `PlayScene.SendMsg(SM_HIDE, ...)`，与旧代码「抛异常⇒该帧被丢弃」等价。
- `SM_ABILITY`：case 首行加 `if (MShare.MySelf == null) break;`。旧代码在本帧只做五个 MySelf
  赋值 + `AbilityCodec.Decode` + `ABILITYVIEW` 日志；空 MySelf 时旧代码在第一个赋值处抛，
  后两者也被丢弃；`break` 与之一致。
- `SM_WINEXP`：case 首行加 `if (MShare.MySelf == null) break;`。旧代码在 `MySelf.Abil.Exp`
  抛，经验值提示一并被丢弃；`break` 与之一致。

非 null 路径不变：门控只在 `MySelf == null` 时短路，`MySelf != null` 时条件/执行序列与旧代码相同。

## 3. 离线证据（扩展 `--selftest-recv-null-context`）

构造 `_inWorld=true` + `MyActor==null` 的 per-packet 上下文，经
`ClientManager.ProcessOnePacket` 注入真编码帧；`SM_TURN` 体 = 11 字符 `CharDesc` 前缀 +
加密名字尾部（`x(name)`），并预建目标 actor；`SM_ABILITY` 体 = 58B 冻结客户端 `TAbility`。

### 3.1 红（只带上一卡门控、本卡门控未加时）

```
SELFTEST_RC=1
RECVNULL FAIL null-turn-no-recv-error        delta null=1
RECVNULL FAIL null-turn-direct-no-throw     NullReferenceException @ ...:line 384
RECVNULL FAIL null-disappear-no-recv-error  delta null=1
RECVNULL FAIL null-disappear-direct-no-throw NullReferenceException @ ...:line 846
RECVNULL FAIL null-ability-no-recv-error    delta null=1
RECVNULL FAIL null-ability-direct-no-throw  NullReferenceException @ ...:line 632
RECVNULL FAIL null-winexp-no-recv-error     delta null=1
RECVNULL FAIL null-winexp-direct-no-throw   NullReferenceException @ ...:line 658
RECVNULL PASS null-daychanging-empty-*      （不可证：ident 46 空 case，不抛）
RECVNULL SELFTEST FAIL (8)
```

非 null 对照组在红树已全 PASS（`nonull-turn-slave-add`、`nonull-disappear-*`、
`nonull-ability-applied`、`nonull-winexp-applied`）。

### 3.2 绿（本卡门控加齐）

```
SELFTEST_RC=0
RECVNULL PASS null-turn-* / null-disappear-* / null-ability-* / null-winexp-*
RECVNULL PASS nonull-turn-slave-add          slaves=1 sent=True
RECVNULL PASS nonull-turn-name-mismatch-skips slaves=1
RECVNULL PASS nonull-disappear-other-deletes deleted=True
RECVNULL PASS nonull-disappear-self-skips    kept=True
RECVNULL PASS nonull-ability-applied         gold=4321 job=2 ip=3 gamegold=458852 lvl=23
RECVNULL PASS nonull-winexp-applied          exp=777
RECVNULL PASS null-daychanging-empty-*       （ident 46 仍不抛）
RECVNULL SELFTEST PASS   (39 PASS)
```

非 null 调用序列等价：`SM_TURN` 名字相等 ⇒ `MySelf.SlaveObject` 恰好追加 1 个目标 actor，
名字不等 ⇒ 不追加（slaves 保持 1）；`SM_DISAPPEAR` Recog 不等 ⇒ 目标 actor 被删、
Recog 相等 ⇒ 目标 actor 保留；`SM_ABILITY` 五个字段 + `Abil.Level=23` 落盘；`SM_WINEXP`
`Abil.Exp=777` 落盘。

### 3.3 反向牙

删除本卡四处门控（3 处 `if (MySelf == null) break;` + 1 处 `MySelf != null &&`）⇒ 重建
`SELFTEST_RC=1`、8 条 FAIL（NRE 行在变异树为 386/852/636/664，对应 base 的 384/846/632/658）；
恢复文件后 `sha256=84c6bd19…` 相等，重建 `SELFTEST_RC=0`。

## 4. 仍未门控的站点（本轮窗口计数为 0）

同一机制但本轮真机栈未体现、且**本卡要求范围外**，保持原样并登记：
`SM_SPACEMOVE_HIDE/HIDE2`（800/806=0，base 436）、`SM_SPACEMOVE_SHOW/SHOW2`（801/807=0，base 459）、
`SM_RUSH/RUSHKUNG`（6/7=0，base 477）、`SM_LEVELUP`（45=0，base 665）、
`SM_TAKEON_OK`/`SM_TAKEOFF_OK`/`SM_WEIGHTCHANGED`/`SM_GOLDCHANGED`/物品与交易族（均=0）。
若下一轮窗口这些 ident 出现且 `recv_packet_null` 仍非 0，按本卡方法逐点先证后改。

## 5. 门禁与写集

- `dotnet build src/BotSrv -c Release`：0 error；
- `BotSrv.exe --selftest-recv-null-context`：rc=0，39 PASS；
- `docs_anchor_gate --prose --base master`：见 patch entry；
- `git diff --check`：见 patch entry；
- 写集以 `git diff --numstat c527e93f8..HEAD` 为准（仅 `src/BotSrv/**` + `docs/**`）。

## 6. 未做项

1. 未跑真机窗口：`recv_packet_null` 归零仍需下一轮 500 档 ≥360s 复验；
2. 未改 `SM_DAYCHANGING(46)`（空 case，不可证）与第 4 节列出的 0 计数站点；
3. 未改 census 类别/白名单；未删归档。
