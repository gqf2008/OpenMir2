# rules.json PENDING 裁定落定（2026-10-25）

- 卡：`openmir2-rules-pending-adjudication`（issue `062972b6581e9fe4fd15370bdbbb4f8a4e3758ce`；tier **L**；owner `openmir2-dev-1`）。
- 基线：`master a60e04ad9`；写集：`tools/conformance/rules.json`、`tools/conformance/rules_approve.py`（新增）、`tools/conformance/selftest_impl.py`、`tools/conformance/compare_runs.py`（新增 `pending_review_used` 附属证据）、本文件。
- **裁定人**：`openmir2-orchestrator`（Leader/coordinator，2026-10-25）。**不是 owner 的裁定**；owner 可推翻（见 §3）。


## 6b. 后续裁定：ident 14 **保持 PENDING，不转正**（2026-10-25）

**背景**：`openmir2-ignore-rule-count-compare`（合入 `e335578bc`）给 `ignore` 规则加了**形态多重集比较**（键含 `param/tag/series|body_sha256`、两侧补 0 侧）。原计划是「计数牙绿 ⇒ 转正 14」，后经 reviewer-21 第二意见实测推翻：

| 14 的状态 | 计数/形态**相等** | 计数/形态**不等** |
|---|---|---|
| **PENDING（现行）** | 放行（记为 `ignored_ident_rules_counted`） | **整轮 UNDECIDABLE** 并点名差额 |
| 已转正 | 放行 | 仅进 census，**verdict 仍可绿** |

⇒ **保持 PENDING 更严**：把「可判」与「可放行」分开。转正等于取消兜底。

**代价（必须一起接受）**：保持 PENDING 时，**任何** 14 的计数/形态差（包括随机支合法地多触发一次）都会让该轮**仍是 UNDECIDABLE**——那不是「绿」，是「判不了」。写判据时按「**可判 = 变成 PASS/FAIL；不可判 = 仍 UNDECIDABLE 并点名**」表述，**不要**承诺「C 类场景从此都能判」。

**残余风险（reviewer-21 N1）**：形态比较是**顺序无关的多重集** ⇒ 剔帧的相对顺序差异不会触发 UNDECIDABLE，只能靠 census 观察。

## 1. 裁定

| 组 | 条数 | 裁定 |
|---|---:|---|
| `global` 掩码 | 4 | 全部转正 |
| `rules` 数组 | 48 | 转正 45；**保留 PENDING 3**（ident 10/14/50） |
| `db` | 1 | 转正 |

翻正后的 `approved_by` 格式（保留原草稿人）：

```
openmir2-orchestrator (coordinator review 2026-10-25, decision=062972b6581e9fe4fd15370bdbbb4f8a4e3758ce; draft: <原草稿人>)
```

## 2. 逐条（ident / action / tier / 草稿人 / 裁定）

| 条目 | action | tier | 草稿人 | 裁定 | 依据（reason 摘录） |
|---|---|---|---|---|---|
| global:recog | mask_fields | L | openmir2-worker-4 | ✅ 转正 | CommandMessage.Recog 多数消息里是会话相关 ActorId（ActorMgr.GetNextIdentity()），两侧必然不同；个别消息（52 的 recog=Gold）在 ident 规则里用 h… |
| global:t_ms | mask_fields | L | openmir2-worker-4 | ✅ 转正 | 驱动器墙钟毫秒，两次独立运行不可能相等；它不是协议内容 |
| global:conn | mask_fields | L | openmir2-worker-4 | ✅ 转正 | 驱动器会话标签（oracle/rust 两侧命名不同），只用于人读；对齐按帧序，不按 conn |
| global:seq | mask_fields | L | openmir2-worker-4 | ✅ 转正 | 驱动器本地序号（每侧自增），不是服务端协议字段；帧对齐按文件顺序，seq 不参与判定 |
| ident=10 | compare | R | openmir2-worker-4 | ⏸ 保留 PENDING | SM_TURN 是真实协议帧（Messages.cs:161），body/param/tag 仍逐字节比；掩的是 **整个 series word = MakeWord(Dir, Light)**（低字节 Dir、高字节… |
| ident=41 | compare | L | openmir2-worker-4 | ✅ 转正 | 41=SM_FEATURECHANGED 是真实协议帧（Messages.cs:202；downlink.rs 明确「SM_TURN+SM_FEATURECHANGED 一组两帧」且有逐字节黄金断言），默认必须逐字节比较… |
| ident=14 | ignore | R | openmir2-worker-4 | ⏸ 保留 PENDING | SM_HIT：固定 NPC / 商人的受击帧。**两个发点，性质不同**：① 随机支——`GuildOfficial.Run` 的 `Random(30)==0`（src/GameSrv/Npc/GuildOfficia… |
| ident=31 | mask_body_ranges | L | openmir2-worker-68; rework after review 926d1b67 | ✅ 转正 | SM_STRUCK(31) 是真实协议帧（src/OpenMir2/Messages.cs:197）。下行点 src/M2Server/Player/PlayObject.Message.cs:1601（本卡已复核）：`… |
| ident=46 | mask_fields | L | openmir2-worker-4 | ✅ 转正 | SM_DAYCHANGING 的 param=Bright、tag=DayBright()，由游戏内小时/光照决定（C# PlayObject.Message.cs RM_DAYCHANGING）；两次运行启动时刻不同 … |
| ident=50 | mask_fields | L | openmir2-worker-4 | ⏸ 保留 PENDING | SM_LOGON 的 series=MakeWord(Dir, Light)，Light 是游戏内光照（墙钟相关，C# PlayObject.cs:1131）；body 与 param/tag 仍比 |
| ident=51 | mask_fields | L | openmir2-worker-4 | ✅ 转正 | SM_NEWMAP 的 series=DayBright()（墙钟相关，C# PlayObject.Message.cs:1681）；body 与 param/tag 仍比 |
| ident=52 | ability_layout | L | openmir2-worker-4 | ✅ 转正 | SM_ABILITY：oracle 40B（Ability.cs 声明序）/ 冻结客户端 58B（record.rs::encode_ability_client）。布局表从 tools/loginfixture/cap… |
| ident=658 | compare | L | openmir2-worker-4 | ✅ 转正 | SM_SENDNOTICE 系统公告：body 文本 + 头部全部按规则比（recog 走全局掩码） |
| ident=42 | compare | L | openmir2-worker-4 | ✅ 转正 | SM_USERNAME：recog=ActorId 走全局掩码；param 颜色码、body 名字逐字节比 |
| ident=54 | compare | L | openmir2-worker-4 | ✅ 转正 | SM_MAPDESCRIPTION 地图描述文本逐字节比 **Recog=nMusicid（地图音乐号，语义值，非 ActorId）**：下行 `src/M2Server/Player/PlayObject.Base.c… |
| ident=5008 | compare | L | openmir2-worker-4 | ✅ 转正 | SM_GAMEGOLDNAME 元宝名称文本逐字节比 **Recog=GameGold（元宝数量，语义值）**：下行 `PlayObject.Base.cs:938` `SendDefMessage(SM_GAMEGOL… |
| ident=1106 | compare | L | openmir2-worker-4 | ✅ 转正 | SM_VERSION_FAIL 客户端版本校验参数逐项比 **Recog=ClientFile1_CRC（版本校验 CRC1，语义值）**：下行 `PlayObject.Message.cs:1690` `MakeMes… |
| ident=752 | compare | L | openmir2-worker-4 | ✅ 转正 | SM_SUBABILITY 子属性 body 逐字节比 **Recog=AntiMagic（语义值；本帧无 body）**：下行 `PlayObject.Message.cs:1675`/`:1935` `SendDef… |
| ident=811 | compare | L | openmir2-worker-4 | ✅ 转正 | SM_ADJUST_BONUS 3x20B 分段体（frame.rs 的 Segmented 契约）逐字节比；分段解码由驱动器负责，这里比最终 body 字节 **Recog=BonusPoint（语义值）**：下行 `… |
| ident=766 | compare | L | openmir2-worker-4 | ✅ 转正 | SM_AREASTATE 区域状态 body 逐字节比 **Recog=n8（区域状态位，语义值；本帧无 body）**：下行 `PlayObject.Base.cs:285` `SendDefMessage(SM_AR… |
| ident=11029 | compare | L | openmir2-worker-4 | ✅ 转正 | SM_SERVERCONFIG 服务端配置文本逐字节比 |
| ident=213 | compare | L | openmir2-worker-4 | ✅ 转正 | SM_ATTACKMODE 攻击模式逐项比 **Recog=(byte)AttatckMode（攻击模式，语义值；本帧无 body）**：下行 `PlayObject.cs:1146` `SendDefMessage(S… |
| ident=100 | multiset_by_tick | L | openmir2-worker-26 | ✅ 转正 | SM_SYSMESSAGE 系统提示：body 是 GBK 静态文案。首跑 4 条实测（docs/conformance-first-dual-run-20261023.md §2.2）=「欢迎进入记忆传奇…www.ar… |
| ident=520 | compare | L | openmir2-worker-4 | ✅ 转正 | SM_QUERYCHR 角色列表：body = 每角色 `名字/等级/性别/职业/…` 文本，逐字节比。该帧的 recog 不是 ActorId 而是 nChrCount（C# src/DBSrv/Services/Im… |
| ident=525 | compare | L | openmir2-worker-4 | ✅ 转正 | SM_STARTPLAY：body = GameGate 的 ip/port 文本（首跑两侧都是 `127.0.0.1/17200`，同端口方案下应逐字节相同 ⇒ 地址/端口必须比、不做掩码）；recog/param/t… |
| ident=529 | compare | L | openmir2-worker-4 | ✅ 转正 | SM_PASSOK_SELECTSERVER：body = `服务器名/服务器状态/` 文本。首跑已暴露真差异 C#「热血传奇/Idle/」vs Rust「热血传奇/0/」（docs/conformance-first-… |
| ident=530 | mask_body_ranges | L | openmir2-worker-4 | ✅ 转正 | SM_SELECTSERVER_OK：body = `SelGateIp/SelGatePort/SessionID` 文本，线上是 **EDCode 编码后的线缆字节**（C# src/LoginSrv/Service… |
| ident=654 | compare | L | openmir2-worker-4 | ✅ 转正 | SM_CHANGELIGHT：无 body（header-only）；recog=ActorId 走全局掩码；param=Light、tag=nClientKey（首跑 500）、series=0（C# src/M2Se… |
| ident=657 | compare | L | openmir2-worker-4 | ✅ 转正 | SM_CHARSTATUSCHANGED：无 body（header-only）；recog=ActorId 走全局掩码；param/tag = nParam1 的低/高字（状态位）、series=HitSpeed（C#… |
| ident=11 | compare | L | openmir2-worker-70 | ✅ 转正 | SM_WALK 是真实协议帧（src/OpenMir2/Messages.cs:165）。下行点 src/M2Server/Player/PlayObject.Message.cs:1403-1410：`MakeMess… |
| ident=13 | compare | L | openmir2-worker-70 | ✅ 转正 | SM_RUN 是真实协议帧（src/OpenMir2/Messages.cs:167）。下行点 src/M2Server/Player/PlayObject.Message.cs:1423-1430：与 SM_WALK(… |
| ident=200 | bagitems_records | R | openmir2-worker-70 | ✅ 转正 | SM_ADDITEM 是真实协议帧（src/OpenMir2/Messages.cs:220）。下行点 src/M2Server/Player/PlayObject.Base.cs:225-227：`MakeMessag… |
| ident=201 | bagitems_records | R | openmir2-worker-70 | ✅ 转正 | SM_BAGITEMS 是真实协议帧（src/OpenMir2/Messages.cs:221）。下行点 src/M2Server/Player/PlayObject.Operate.cs:38-64 ClientQue… |
| ident=615 | compare | L | openmir2-worker-70 | ✅ 转正 | SM_TAKEON_OK 是真实协议帧（src/OpenMir2/Messages.cs:284）。下行点 src/M2Server/Player/PlayObject.Operate.cs:458：`SendDefMe… |
| ident=616 | compare | L | openmir2-worker-70 | ✅ 转正 | SM_TAKEON_FAIL 是真实协议帧（src/OpenMir2/Messages.cs:285）。下行点 src/M2Server/Player/PlayObject.Operate.cs:493-496：`Sen… |
| ident=635 | compare | L | openmir2-worker-70 | ✅ 转正 | SM_EAT_OK 是真实协议帧（src/OpenMir2/Messages.cs:297）。下行点 src/M2Server/Player/PlayObject.Operate.cs:690：`SendDefMessa… |
| ident=636 | compare | L | openmir2-worker-70 | ✅ 转正 | SM_EAT_FAIL 是真实协议帧（src/OpenMir2/Messages.cs:298）。下行点 src/M2Server/Player/PlayObject.Operate.cs:698：`SendDefMes… |
| ident=643 | compare | L | openmir2-worker-70 | ✅ 转正 | SM_MERCHANTSAY 是真实协议帧（src/OpenMir2/Messages.cs:304）。链路：src/M2Server/Npc/NormNpc.cs:1615 `SendMsg(this, RM_MERC… |
| ident=644 | compare | L | openmir2-worker-70 | ✅ 转正 | SM_MERCHANTDLGCLOSE 是真实协议帧（src/OpenMir2/Messages.cs:306）。下行点 src/M2Server/Player/PlayObject.Message.cs:1806-18… |
| ident=645 | compare | L | openmir2-worker-70 | ✅ 转正 | SM_SENDGOODSLIST 是真实协议帧（src/OpenMir2/Messages.cs:307）。链路：src/GameSrv/Npc/Merchant.cs:985-991 逐商品拼 `名字/子菜单/价格/库… |
| ident=763 | compare | L | openmir2-worker-70 | ✅ 转正 | SM_BUILDGUILD_FAIL 是真实协议帧（src/OpenMir2/Messages.cs:376）。下行点 src/M2Server/Player/PlayObject.Message.cs:1940-194… |
| ident=767 | compare | L | openmir2-worker-70 | ✅ 转正 | SM_MENU_OK 是真实协议帧（src/OpenMir2/Messages.cs:379）。下行点 src/M2Server/Player/PlayObject.Message.cs:1952-1954：`SendD… |
| ident=202 | bagitems_records | R | openmir2-worker-66 | ✅ 转正 | SM_DELITEM 是真实协议帧（src/OpenMir2/Messages.cs:222）。下行 src/M2Server/Player/PlayObject.Base.cs:1663 `MakeMessage(SM… |
| ident=203 | bagitems_records | R | openmir2-worker-66 | ✅ 转正 | SM_UPDATEITEM 是真实协议帧（src/OpenMir2/Messages.cs:223）。下行三处同形（`SendUpdateItem` / `SendUpdateItemWithLevel` / `Send… |
| ident=642 | compare | L | openmir2-worker-66 | ✅ 转正 | SM_DURACHANGE 是真实协议帧（src/OpenMir2/Messages.cs:303）。处理器 src/M2Server/Player/PlayObject.Message.cs:1803-1805：`Se… |
| ident=709 | compare | L | openmir2-worker-66 | ✅ 转正 | SM_DELITEMS 是真实协议帧（src/OpenMir2/Messages.cs:357）。下行 `SendDelItemList`（src/M2Server/Player/PlayObject.Base.cs:1… |
| ident=53 | compare | L | openmir2-worker-58 | ✅ 转正 | SM_HEALTHSPELLCHANGED(53)（src/OpenMir2/Messages.cs:211）是真协议帧。oracle 下行链：BaseObject.HealthSpellChanged()（src/M2… |
| ident=638 | compare | L | openmir2-worker-58 | ✅ 转正 | SM_MAGICFIRE(638)（src/OpenMir2/Messages.cs:300）是真协议帧。oracle 发点：MagicManager.DoSpell 共享尾（src/M2Server/Magic/Mag… |
| ident=639 | compare | L | openmir2-worker-58 | ✅ 转正 | SM_MAGICFIRE_FAIL(639)（src/OpenMir2/Messages.cs:301）是真协议帧。oracle 发点：ClientSpellXY 的 default 支在 MagicManager.Do… |
| ident=211 | compare | L | openmir2-worker-74 | ✅ 转正 | SM_SENDMYMAGIC(211) 是真实协议帧（src/OpenMir2/Messages.cs:225）。下行链：登录魔法同步 PlayObject.cs:1390 `SendPriorityMsg(RM_SEN… |
| ident=600 | compare | L | openmir2-worker-74 | ✅ 转正 | SM_DROPITEM_SUCCESS(600) 是真实协议帧（src/OpenMir2/Messages.cs:277）。唯一发送点 src/M2Server/Player/PlayObject.Message.cs:… |
| ident=601 | compare | L | openmir2-worker-74 | ✅ 转正 | SM_DROPITEM_FAIL(601) 是真实协议帧（src/OpenMir2/Messages.cs:278）。与 600 同一条 `case Messages.CM_DROPITEM`（src/M2Server/… |
| db | presence | L | openmir2-worker-4 | ✅ 转正 | 两次独立运行的墙钟必然不同：时间列默认只比 NULL/非 NULL（presence）。需要逐时刻比时用 --db-time-mode exact/normalized；跨运行自增 Id 不同时用 --db-key-mo… |

## 2b. 报告字段口径（Leader 裁定 2026-10-25）

`compare_runs.py` 的报告里三个字段各司其职、**不能互相替代**：

| 字段 | 口径 | 用途 |
|---|---|---|
| `rules_pending_review` | **保守主标志**：规则文件里只要还有**任何** PENDING（global/rules/db 任一，含本卡保留的 10/14/50）就是 `true` | 读者判断「这份报告的判据是否**全部**审过」。宁可恒真，也不让「没碰到 pending ident」的报告被误读成全审过 |
| `pending_review_used` | **本次影响面**：global 恒计入；db 仅在本次比了库时计入；ident 规则与两侧出现过的 ident 求交集 | 判读「这条证据实际受哪几条 PENDING 影响」；**不是**信任闸门 |
| `pending_review` | 全量 PENDING 清单（审计用，含条目定位） | 复核/审计 |

⇒ 在 10/14/50 全部转正之前，`rules_pending_review` 会**恒为 true**——这是刻意的保守语义。

## 3. 保留 PENDING 的三条与解封条件

| ident | 为什么保留 | 解封条件 |
|---|---|---|
| 14（ignore） | reviewer-2 在 `46e479fc`/`2f4b311f` 明写须等 `openmir2-castle-merchant-fixedhide-hit` 的 R 级窗口⑧；代码虽已合 `0119e47b9`，窗口未跑 | **2026-10-25 裁定：暂不转正（见 §6b）**——工具侧「计数/形态比较」已落地（`e335578bc`，reviewer-2 保留条件的后半句已满足），但转正会**取消**「形态不等 ⇒ 整轮 UNDECIDABLE」的兜底，反而更松 |
| 10（compare + head_mask:[series]） | 掩掉整个 `MakeWord(Dir,Light)`，连 Dir 一起丢 ⇒ 原地转身缺帧级判据 | 工具支持「只掩 Light 半字节」后重评 |
| 50（mask_fields:[series]） | reason 只论证 Light 是墙钟、没论证 Dir | 同上 |

## 4. owner 可推翻与回滚

- **本裁定由 Leader/coordinator 做出**（`rules.json` 的 note 把复核责任放在 owner/coordinator）。owner 若不同意，可推翻：
  1. 回滚代码：`git revert <本卡合并提交>`（或把 `approved_by` 恢复为 `PENDING owner/coordinator review (draft: …)`）；
  2. 重新裁定：运行 `python tools/conformance/rules_approve.py --check` 会列出待翻正项，按新裁定改 `KEEP_PENDING_IDENTS`/`APPROVED_TMPL` 或直接手工改 `approved_by`；
  3. **判据内容（action/head_mask/mask_fields/tier/reason）本卡一字未改**——推翻只涉及 `approved_by` 文案与自测断言，不涉及比较语义。

## 5. 牙（改判据 = 改验收口径，必须仍有牙）

| 牙 | 变异 | 结果 |
|---|---|---|
| 核心 | 把 ident=100 的 `approved_by` 改回 `PENDING` | `compare_runs.py selftest` **红**（ident=100 正式批准断言） |
| 保留项 | 把 ident=10 翻正 | **红**（keep-pending 断言） |
| 标记口径 | 把主标志改成「按本次涉及」（`bool(used)`） | **红**（520-only 用例期望主标志恒 True、`pending_review_used` 为空） |
| 校验器 | tier 非法 / reason <20 / PENDING 缺草稿人 / 他人批准格式 | `rules_approve --selftest` 各有一条红 |

## 6. 门禁

- `python tools/conformance/compare_runs.py selftest` ⇒ **316/316 GREEN**（含 520-only 主标志恒 True、ident=10 时 used 命中的两条用例）；
- `python tools/conformance/rules_approve.py --check` ⇒ rc=0（flip=0 keep=3 errors=0）；`--selftest` ⇒ 11 checks OK；
- 7 项卫生门（`stale_pointer`/`dynamic_count` 带 `--ref master`）、`git diff --check <base> <head>`、`merge_tree_helper`：见 patch body。
