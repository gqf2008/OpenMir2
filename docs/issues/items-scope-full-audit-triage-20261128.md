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

