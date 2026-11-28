# items_scope_full_audit 分诊——60 条 scope≠full 三档分类（§28 口径）

> 分诊日期：2026-11-28｜分母：coverage-report @ master `176307bad`（已验 96 条中 60 条部分范围：技能 52 · 物品 8）。
> 三档定义（总监派工单）：①加输入即 full（一条旋钮/一个注入）②需要新机制面 ③oracle 本身不可观测
> （死结构/不可达/残差归他卡——走定性关账）。
> **本笔只分诊不动手**；批后 ① 档逐条落地。

## 汇总

| 档 | 条数（按主档计） | 预估 |
| --- | --- | --- |
| ① 加输入即 full | **16**（其中 4 条为拆分条的主面） | 每条 S～S-M：夹具数值/旋钮/注入复用既有装置件（prebuff 注入、config 变体、边界值），合计 1.5～2.5 个工作日 |
| ② 需要新机制面 | **2** | MG-12 真实墙钟相位（真实定时器×tick 网格，S-M）；MG-59 U-3 buff 超时（buff 生命期面，M）——建议各立小卡 |
| ③ 定性关账候选 | **42**（含 8 条拆分条的副面） | 不动测试；逐条改 scope 备注为「定性关账（理由编号）」或确认残差归他卡；纯文书 0.5 日 |

## ③ 档判据说明（为什么这些不算"缺口"）

- **A 类：残差 = 已登记 wire 偏差**（client 布局尾，责任卡 `openmir2-client-e2e-enter-gaps`）：
  MG-06、MG-09、MG-10、MG-11、MG-19、MG-27、MG-37、MG-44、MG-45、MG-49、MG-53、MG-56、MG-28、MG-29、MG-50。
- **B 类：残差 = 已登记产品差异/他卡归属**：MG-17、MG-24（refmsg-queue-model 卡；656 归 v2 卡）、
  MG-34（656→656 归 `openmir2-monster-actors-v2`）、MG-35-④（spacemove-visible-actors-reset 卡）。
- **C 类：oracle 本身不可达/无该行为（机检或探针结论）**：MG-47（冻结 magics 无 MagicId=35）、
  MG-41（生产不可召唤，opc-27 裁决）、MG-55（③探针=oracle 无寿命代码）、MG-60/61/62/63（case 体只有
  `boTrain=true`，无私有效果体可验）、MG-07（DoMotaebo 真支=实现缺口 DEV-MG-07，另开实现卡）、
  MG-13/MG-14 的 Shape 0/>2 面（Rust 未见对应处理=DEV-MG-13/14，另卡）。
- **D 类：残差已被后续批闭合（scope 备注过期）**：MG-18（白名单负例已补）、MG-28/29（强度牙已补）、
  MG-32（怪分支已验）、MG-44（六臂已覆盖）、IT-17 的 657 HitSpeed/CharStatus（m3 批已点亮为真值）。
- **E 类：golden 留锚不比较（记录面）**：MG-32 的 `ActiveCount--`。

## ① 档清单（16 条，含拆分）

| 条目 | 残差 | 要加的输入 | 预估 |
| --- | --- | --- | --- |
| IT-01 | 单侧封顶未覆盖（HP 顶/MP 不顶及反向） | 夹具数值：一侧到顶另一侧不到顶两例 | S |
| IT-03 | 百分比药单侧截断边界 | 同上（百分比口径） | S |
| IT-04 | `if(AC>0)`/`if(MAC>0)` 单侧分支 | 夹具：AC>0 且 MAC=0（及反向） | S |
| IT-09 | 766/54 的 Recog(n8/nMusicid)、54 非空 MapDesc 尾、634 series(DayBright)、801 series(Dir) 夹具为占位 | 夹具字段点亮：地图带 MusicId/非空 MapDesc/有 DayBright 的图 + Dir 旋钮 | S-M |
| IT-10 | 同上（同簇） | 同上（同一批夹具改造覆盖 09/10/11/13） | S-M |
| IT-11 | 同上（同簇） | 同上 | S-M |
| IT-13 | 同簇 + （castle 分支归 ③） | 同上 | S-M |
| IT-17 | 622 校验和夹具 weight=0（字段已在 IT-16 点亮，复用同源） | 夹具 weight 非零一支 | S |
| MG-13（位号面） | `10139.p1` 跨 buff 位号（夹具恒 int.MinValue） | 复用 MG-64/65 已落地的 prebuff 注入到 amyounsul stem | S-M |
| MG-14（位号面） | 同上 | 同上 | S-M |
| MG-15 | ①Shape 0（生成器 `amulet_shape` 旋钮放开）；②`0x40000000` 位路径（目标预置 buff——prebuff 注入已落地，复用） | 旋钮 + 注入复用 | M |
| MG-23 | 等级差面（小幅度 +1/+50 不可观测，需 ≥+172） | 夹具 `caster_level=202+` 旋钮一支（判定推导已在 B15） | S（低优先） |
| MG-46 | Run 时刻衰减（装置不跑该 tick；撤销条件=可注入世界时钟） | 可注入时钟的单测（notes 明示路径） | S-M |
| MG-48 | `MabMabe*` 四键非缺省部署副本重出一支（行为面点亮） | config 变体（`rand0/rand1` 先例） | S-M |
| MG-59（U-1/U-2） | 非自身 proper-friend 目标 / CheckAmulet 失败→639 | 目标旋钮 + 护身符旋钮 | S-M |
| MG-64/65 | 非 target 对象的预置 buff（装置只支持 target） | 装置注入扩展：怪物目标 prebuff | M |

## ② 档清单（2 条）

| 条目 | 残差 | 需要的机制面 | 预估 |
| --- | --- | --- | --- |
| MG-12 | 真实周期 tick 的**墙钟相位**（现用显式虚拟 ms 列表） | 真实定时器 × tick 网格对齐的 harness（非虚拟时钟） | S-M |
| MG-59（U-3） | DoSpell 后 buff 加成与超时 | buff 生命期/超时面（tick 或世界时钟族） | M |

## ③ 档清单（42 条，逐条一行理由）

| 条目 | 档内判据 | 关账动作建议 |
| --- | --- | --- |
| MG-06 | A：client 布局尾归 enter-gaps 卡 | scope 备注追加「残差归他卡，定性关账」 |
| MG-09 | 同上 | 同上 |
| MG-37 | 同上（短注） | 同上 |
| MG-56 | 同上（短注） | 同上 |
| MG-49 | 同上 | 同上 |
| MG-45 | 同上（伤害/33 专属牙已全） | 同上 |
| MG-44 | 同上（六臂已覆盖） | 同上 |
| MG-10 | 同上（伤害+边界+训练面均闭合） | 同上 |
| MG-11 | 同上（+对象分支已验） | 同上 |
| MG-19 | 同上（+undead 1.5x 两轴已验） | 同上 |
| MG-27 | 同上（+monster 支已验；ArmRingAmulet 失败臂由同族 golden 覆盖） | 同上 |
| MG-38 | 同上（+undead 支已验） | 同上 |
| MG-53 | 同上（+对象分支已验） | 同上 |
| MG-28 | D：强度牙已补；残余 layout | 同上 |
| MG-29 | D：同上 | 同上 |
| MG-17 | B：多受害者帧序=已登记产品差异（b4 卡） | 同上 |
| MG-18 | D+A：白名单负例已补；32ms 容差=装置口径 | 同上 |
| MG-24 | B：帧序归 refmsg-queue-model 卡；656 替身盲区归 v2 卡 | 同上 |
| MG-32 | D+E：怪分支已验；ActiveCount-- = golden 留锚 | 同上 |
| MG-34 | B：656→656 归 monster-actors-v2 卡；60005 支已由 P3-B3 关闭 | 同上 |
| MG-35 | B/C：①②已闭合；③共享判据；④归 spacemove-visible-actors 卡 | 同上 |
| MG-41 | C+B：生产不可召唤（opc-27 裁决）；RNG 已纳入；640 tag/AOI 登记 | 同上 |
| MG-47 | C：冻结 magics 无 MagicId=35（机检）；效果体语义已验 | 同上 |
| MG-50 | A：group 主链已验；残余 layout | 同上 |
| MG-55 | C：①已闭合（mg55-hide-aoi）；③探针=oracle 无寿命代码 | 可直接改 scope=full |
| MG-60 | C：case 体只有 boTrain=true，无私有效果体；共享尾全判 | 可直接改 scope=full |
| MG-61 | 同上 | 同上 |
| MG-62 | 同上 | 同上 |
| MG-63 | 同上 | 同上 |
| MG-07 | C：DoMotaebo 真支=实现缺口 DEV-MG-07（非测试缺口，另开实现卡） | 定性关账+引用 DEV 卡 |
| MG-13（Shape 面） | C：Shape 0/>2 时 Rust 未见对应处理（DEV-MG-13） | 同上 |
| MG-14（Shape 面） | C：DEV-MG-14（取数点分离/可测性） | 同上 |
| IT-13（castle 面） | C：部署 Castle.MasterGuild 恒 null ⇒ 生产不可达（synthetic 不作生产验证） | 同上 |
| IT-17（item_list 面） | B：另卡 `openmir2-itemlist-element-compare.md` | 同上 |
| MG-12（主面） | ② 档（真实墙钟相位）→ 不在 ③ | —（列于 ②） |
| MG-15 | ① 档（复用 prebuff 注入 + Shape 旋钮） | — |
| MG-23 | ① 档（+172 旋钮；判定推导已在 B15） | — |
| MG-46 | ① 档（可注入时钟） | — |
| MG-48 | ① 档（非缺省配置副本）；STONE/怪目标依赖队列模型归 ③（附注） | — |
| MG-59 | ①（U-1/U-2）+ ②（U-3）拆分 | — |
| MG-64/65 | ① 档（非 target prebuff 注入）；Shape>=3 生产不可达归 ③（附注） | — |
| IT-17 | ①（weight）+ ③（item_list 另卡）；657 HitSpeed/CharStatus 残余已被 m3 批消除（备注过期） | — |

## 建议

1. **③ 档 42 条走定性关账**：不新增任何测试。两种动作——残差归他卡的（A/B 类）在 scope 备注追加
   「残差归他卡（卡名），定性关账」；oracle 无该行为/不可达的（C 类）追加「oracle 不可达（机检/探针留痕）」；
   D 类（已被后续批闭合）直接把过期的残差描述改为现状。**其中 MG-55/60/61/62/63 五条可在备注更新后
   直接 scope=full**（其"部分"只剩已被证明不存在或已闭合的面）。
2. **① 档 16 条按上表顺序落地**（每条一提交一变异自证，同五门口径）；同簇四条（IT-09/10/11/13）共用
   一批夹具改造，建议合并为一笔。预估合计 1.5～2.5 日。
3. **② 档 2 条各立小卡**（MG-12 墙钟相位、MG-59 U-3 buff 超时），不混入本批。
4. 分诊若获批：先做 ③ 档备注批（0.5 日，scope 数字即刻改善 42 条），再按 ① 清单落地。

—— 分诊人：openmir2-a0（card/items-scope-triage @ 176307bad）

## 补记（2026-11-28，总监裁决后）
IT-09/10/11/13 同簇拆分判：**MUSIC 面（54 recog=nMusicid 非缺省支 + 766 n8 同源）降 ③-C**——
总监机检：冻结部署 Envir（E:/MirServer/M2GameSvr/Envir）MapInfo.txt 全文 MUSIC 旗标 0 处 ⇒
 生产 nMusicid=-1 恒定、不可观测；机制面已证（装置 2a 批注入点亮 54 recog -1→7，2026-11-27 已并）。
**MapDesc 尾面维持 ①**（冻结地图名行带非空中文描述，样例 `[jiaren|0150 家人回归]`——选非空描述图
 即可，不触内容红线；夹具地图集绑定点定位中，若在装置内嵌数据则回报再裁）。
DayBright(634)/Dir(801) 维持 ①（运行期旋钮）。原 ① 表中该四条的行按此拆分执行。

**落地记录（2026-11-28，card/items-scope-triage，openmir2-10）**：该四条按本补记拆分执行完毕——
MapDesc 尾（ctx.map_desc 逐图非空、落地目标图绑定）+ DayBright（DARK 臂=1，冻结 MapInfo.txt
DARK 4 处 ⇒ 生产可达）+ Dir（实体旋钮 5 替代 ctor 常量 4）三面点亮，探针两侧重出 golden，
rig 同值；四组变异红→还原留痕（M1 尾/M2 DARK 臂/M3 Dir 活值/M4 逐图绑定，详见 manifest
四条 scope）。801 series 高字节 Light=0（ctor 值，生产由装备派生）未点亮，manifest 已登记如需另立。

**升满落账（2026-11-28，opc-66 验收并 master 189226ca0）**：IT-09/10/11/13 四条 scope=full
（删 scope 键、原文+验收引文入 notes——coverage 口径 full=无 scope 键，1abe1f9cb 先例）。
复验：四条簇测试 18 用例全绿 rc=0 + M2 变异抽验红→还原绿 + workspace --lib 4231/0 + schema GREEN。
全板 scope≠full 计数：分诊时 60 → 五条升满批（dddbb9609，MG-55/60/61/62/63）后 55 → 本笔 51。
① 档余量：IT-01/03/04/17、MG-13/14/15/23/46/48、MG-59（U-1/U-2）、MG-64/65 共 12 条（MG-12/59-U-3 归 ②）。

**IT-01/03/04 落地记录（2026-11-28，card/items-scope-triage，openmir2-10）**：动手前机检发现
三条的处方（夹具数值边界）**已于 B3 批满足**——92a7af77e（2026-11-20）把六个单侧例
（Shape1 只 HP/只 MP 顶、Shape3 百分比两向单侧、default 仅 AC/仅 MAC）并入了 eat_vectors
（37 例全量进 `matches_oracle_eat_fixture` 比较、一直绿），但三条 manifest 漏更（evidence.cases
未引、scope 仍标 uncovered）⇒ 属 ③-D 类「残差已被后续批闭合、台账过期」。本笔=台账订正 +
变异自证：M-IT01（HP 截顶臂连带顶满 MP）首红=clamp_hp_only、M-IT03（Shape3 同型污染）首红=
percent_clamp_hp_only、M-IT04（池臂 AC/MAC 互换+MAC==0 错臂定向失效态）首红=inc_pool/ac_only；
三条 scope=full（evidence.cases 补引六例），全板计数 51→48。

**MG-64/65 非 target 扩展落地（2026-11-28，card/items-scope-triage，openmir2-10）**：装置
`PreBuffs.Who` 放开 `mon<k>`（位判据改对象分组组内序，既有件零变化；golden 增
`pre_buff_status_by_object` 逐对象实测位）；绿/红毒 stem 各加一支双怪例（`prebuff_mon` /
`prebuff_mon_red`——非目标怪预挂 Defense，群体臂逐对象投毒 8037，Rust 侧 10139.p1 按对象
取值 + 657 recog→tag 逐怪身份钉）。变异（预 buff 错挂 mon0）红→还原；还原后同 id
16 驱动 + 双比较器全绿（mgs64-mon-final）。残余定性：被毒怪自身 Operate 的 10139/657
发射面 oracle 不可观测（装置 v1 怪物只记录不 Operate，撤销=装置 v2）；Shape>=3 ③-C 不变。
MG-13/14 位号面（target-prebuff 复用）与 MG-15 随后。

**MG-64/65 升满落账（2026-11-28，opc-66 验收并 master 916d7e308）**：两条 scope=full（删
scope 键、原文+验收引文入 notes）。复验：全量默认门 Both 模式（新 RunId、无 -Only）驱动
217/217 + 比较器 55/55、零零匹配 PARITY_GATE_OK + workspace --lib 4231/0 + 金标准纯插入
不变量核过。657 白名单放宽（恒 0x8000→毒高位在位）随笔确认入 notes。全板 scope≠full
计数 48→46（按 coverage-report 实际输出为准）。

**MG-13/14 位号面落地（2026-11-28，card/items-scope-triage，openmir2-10）**：amyounsul
（绿，Shape=1）新支 `prebuff_bit2`——目标预挂两条非毒 buff（位 0/1）⇒ 绿毒落位 2 ⇒
`10139.p1=0xE0000000`（既有三支恒 int.MinValue 的位号面就此点亮；MG-14 Shape=1 面与
MG-13 主链位号面共用该例）。装置 target-prebuff 复用（零新装置面）、金标纯插入、点名表
29、变异（驱动跳过第二条 buff）红→还原、同 id 7 驱动+双比较器全绿。两卡 residual 余：
护身符 Shape>2（③-C 生产不可达）与 Shape 0（归 MG-15 旋钮批）。MG-15 随后
（amulet_shape 旋钮 + 红臂 0x40000000 例）。

**MG-15 落地（2026-11-28，card/items-scope-triage，openmir2-10）**：两面全落——① **Shape 0
旋钮**：装置新 stem `amyounsul-shape0`（AmuletShape=0 放开，产 `amyounsul-shape0.json`
新金标）× s1/s5/s13，单目标臂 fall-through 面（不投毒零 8037 / 护身符照扣 642=500 /
训练尾 640 在）；② **红臂 0x40000000 位路径**：amyounsul2 新支 `prebuff_red`（目标预挂
一条非毒 buff ⇒ 红毒落位 1 ⇒ 10139.p1=0xC0000000 含推算位）。变异两组（红→还原）：
产品 match 加 0 臂 ⇒ shape0 红；`get_buff_status` 位号恒 0 ⇒ prebuff_red/prebuff_bit2
双红（第一版改 wParam 毒型不咬——位号来自 buff 插入序，诚实记录为非有效变异）。
还原后同 id（mg15-final）11 驱动 + 三比较器全绿。MG-13/14/15 三条 residual 均只剩
Shape>2 ③-C（生产不可达）⇒ 可一并升满（46→43，待总监复验本笔后落账）。

**IT-17 weight 面落地 + 升满（2026-11-28，card/items-scope-triage，openmir2-10）**：
readbook-probe 新例 `eat_ok_book_ergum_second_book_keeps_weight`（包内第二本书读后保留 ⇒
WeightChanged 重算 Weight=5≠0 ⇒ 622 Recog=5、校验和覆盖 12；probe Ctx.ExtraBag +
ctx.bag 条件键——既有 8 例仅加 "bag": null 键、内容零变化；rig 背包从 ctx.bag 同形建袋）。
变异自证：readbook WeightChanged Recog 钉死 0 ⇒ 新例帧逐字节红（Cmd(0,622,0,7,36684) vs
golden Cmd(5,622,0,7,36679)）而仅旧例的 ergum 单例测试保持绿（判别面）。IT-17 scope=full
（删键；657 已由 m3 批点亮、item_list ③-B 归另卡）——**物品域 scope 键清零（0/19）**。
全板 partial 计数待总监按 coverage-report 实际输出落账（IT-17 一条：预计 43 或按主档 42）。

**MG-46 改判记录（2026-11-28，card/items-scope-triage，openmir2-10）**：① 表该行的处方
（可注入时钟补「Run 后归 0」对拍）**前提经机检不成立**——oracle 的 StatusTimeArr 周期递减
循环在 BaseObject.Base.cs Run 内**整段被注释**（死代码）；全仓唯一活递减 =
DamageBubbleDefence（事件驱动、只 BubbleDefenceUP）⇒ 毒计时器从不衰减、「Run 后归 0」
在 oracle 不存在（同 MG-55「oracle 无寿命代码」判型）；Rust 1:1 不实现且 components.rs:218
显式登记 ⇒ 两侧一致非缺口，原登记 DEV-MG-46-RUNDECAY 作废（撤销条件 = oracle 恢复递减
循环）。MG-46 改 ③-C 定性关账 + scope=full（DoSpell 四支矩阵已对拍）——**① 档余量由
处方条目变为实际落地条目：本行移出 ①**。全板 partial 46→45（按 coverage-report 实际
输出为准）。

**MG-48 sr30 批落地（2026-11-28，card/items-scope-triage，openmir2-10）**：follow-up
「非缺省配置值点亮行为面」落第一支——新变体 `mabmabe-sr30`（`MabMabeHitSucessRate=30`
vs 部署 21；完整 Server.conf 副本 + sha 钉 bef26b0d…，C# Variants/Rust
server_conf_expected 两份表互钉）× s1/s5/s13，命中骰模数由配置来。变异（驱动 conf 应用
处吃回缺省 21）⇒ 绝对值合同红；还原后同 id 9 驱动 + 三比较器全绿。四键进度：RandRate ✓
（rand0/rand1）、SucessRate ✓（本批）、MinLvLimit/MabeTimeRate 未点亮（time10 成功臂仍受
refmsg-queue-model 卡阻塞）。**MG-48 保持 scope 键**——残留（怪物目标、其它值域、
time10）按总监新纪律升满先报裁定。

**MG-59 U-1/U-2 落地（2026-11-28，card/items-scope-triage，openmir2-10）**：
- **U-1**（非自身 proper-friend 目标）：装置 `friend_target` 支（targetNull=false ⇒ AttPowerUp
  落玩家 target——target Sent=[17,52,638,657,657,100]、CasterExtraAbil0=0）；rig 前提=实体落格
  （CretInNearXy snap 扫格）+ 点击坐标=目标格。
- **U-2**（CheckAmulet 失败→639）：装置新旋钮 `AmuletOff`（不装配 ArmRingl）+ RunMagicCase 补
  caller 失败分支（Attack.cs:1278-1281 RM_MAGICFIREFAIL ⇒ 639×3 收件人）；Rust rig 据 golden
  `amulet_off`/`target_null` bool 键装配，Sent 补 `due_ms_rel`（施法批 0/交付批 elapsed）。
- 变异红→还原：M-U1 错传 self（比较器红：buff 帧/ExtraAbil 归 caster）；M-U2 照装护身符
  （比较器红：amulet_off_fail 639 缺、成功帧 7 条出现——首版两变异叠加的台账互相污染，
  分开后各自咬中）。还原后同 id（mg59-deliver）5 驱动 + 比较器全绿。
- MG-59 **保持 scope 键**（U-3=②档卡 openmir2-mg59-u3-buff-timeout.md 未落地）——升满待
  U-3 卡落地或总监裁定。

**① 档"可落地项"至此清零**：本表 16 条中 15 条落地/改判（IT-09/10/11/13、IT-01/03/04、
IT-17、MG-64/65、MG-13/14、MG-15、MG-48、MG-59 U-1/U-2）+ MG-46 改判 ③-C；MG-23 收尾
待做。余量=IT-17/MG-48 的他卡阻塞升满 + ② 档 2 卡 + 三张真未覆面归他卡项。

