# `RM_*` 下发模型：oracle 全入队 vs Rust 直发（系统性顺序差异）

> 状态：**立项（未开工）**。本卡只登记与排期，不改产品代码。
> 来源：8/37（`SKILL_FIREWIND` / `SKILL_ENERGYREPULSOR`）驱动出现 `SM_BACKSTEP(9)` 先于
> `SM_MAGICFIRE(638)`，而 golden 相反；追查后确认**不是该支独有**，而是宿主下发模型的系统性差异。

## 1. oracle 模型（唯一判据）

`SendRefMsg`（`src/M2Server/Actor/BaseObject.Message.cs:383`）以自身为中心扫
`±SystemShare.Config.SendRefMsgRange`（缺省 12 ⇒ 25×25），对每个 `Race == ActorRace.Play`
且非 `Ghost` 的对象调 `targetObject.SendMsg(this, wIdent, …)` —— **施法者自己也在其中**
（它就在自己的格子上），**没有"发给自己的那条立即处理"的快路径**；隐身/固定隐身分支
（`src/M2Server/Actor/BaseObject.Message.cs:389`）只是"只发自己"，**仍然入队**。

`SendMsg` 的尾部统一入队：`MsgQueue.Enqueue(sendMessage, (byte)wIdent)`
（`src/M2Server/Actor/BaseObject.Message.cs:181`）⇒ **每个收件人一条优先队列，键 = `(byte)wIdent`**，
出队序 = 优先级升序（.NET 四叉堆；同优先级按入队序）。自延迟消息（`SendSelfDelayMsg`）走
`MessagePriority.Lowest`（255），不是 `(byte)wIdent`——这一条由 golden 的 `Rm.priority` 记录可证。

## 2. Rust 现状（混用模型）

**直发（绕过任何队列，顺序 = 调用序）**，代表性符号：
- `broadcast_ref_msg`（`bins/game-svr/src/lib.rs`，符号锚）；
- `broadcast_ref_msg_with_self`（同文件）；
- `broadcast_ref_msg_with_self_at`（同文件）；
- `send_self_frame`（同文件）；
- 公共体 `broadcast_ref_msg_opt`（同文件）→ `send_ref_msg_to_targets`（同文件）→ 逐个收件人
  `tx.send(SimToGate::SendFrame)`。

**入缓冲（按优先级出队）**：`SpellFrameBuf::flush`（同文件，符号锚）——`push_actor` /
`push_broadcast_cached` / `push_socket` 入队时按 `(RM_* & 0xFF)` 给优先级，逐收件人灌
`game_svr::msgqueue::NetPriorityQueue` 再出队。

**调用点计数（当前 master 实测）**：直发 **57 处**（`broadcast_ref_msg` 9 + `broadcast_ref_msg_with_self` 30
+ `broadcast_ref_msg_with_self_at` 3 + `send_self_frame` 12 + `send_ref_msg_to_targets` 3）；
入缓冲 **44 处**（`push_actor` 19 + `push_broadcast_cached` 18 + `push_socket` 7）。
另有自带逐收件人队列的模块（castle / ground / guild_official / healthspell 各有
`NetPriorityQueue`）。

⇒ 同一次施法里，"效果体直发的帧"与"缓冲后 flush 的帧"按**两个不同的序**落到同一条连接上。

## 3. 暴露面（golden 机判；口径见 §3.1）

- 42 份 magic golden 里，**有逆序的 stem 集合（18 个）**：
  `amyounsul`、`amyounsul2`、`groupamyounsul`、`groupamyounsul2`、`fire`、`skill10`、`fireboom`、
  `fireboom47`、`lightflower`、`snowwind`、`grouplightening`、`firecharm`、`hbfireball`、
  `lightning-11`、`lightning-45`、`fireball`、`fireball-5`、`showhp`；**逆序总次数 136**
  （口径：只用 golden `Rm` 里**已记录**的 `priority` 作键，查不到即计入 UNMAPPED、不猜）。
- 主要 RM pair：`(8103→8090)`、`(10120→10052)`、`(8103→10052)`、`(8092→10052)`、`(10123→10410)`。
- 也就是说：**"同一 `due` 批次内出现 ≥2 条不同 RM"的场景在 14/42 个 stem 上存在**——产品顺序一旦错，
  暴露面远大于 8/37。

### 3.1 判据与它的限定（**引用时必须带上**）

普查键 = **产生该 SM 帧的那条 RM 的 `(byte)ident`**（不是 SM 的）。反例正是本卡起因：
`SM_MAGICFIRE(638)&0xFF = 126`、`SM_BACKSTEP(9)&0xFF = 9`，按 SM 排会把合法序判成逆序；
真实优先级是 `RM_MAGICFIRE(10120)&0xFF = 136` < `RM_PUSH(8092)&0xFF = 156`。
优先级取值：优先取该收件人 `Rm` 里记录的 `priority`（自延迟消息为 255），缺则 `(byte)RM`。
一条 RM 产出多帧（如 `RM_TURN ⇒ SM_TURN + SM_FEATURECHANGED`）共享同一优先级，**不参与判定**；
同优先级 ties 不看；只抓**同一 `due_ms_rel` 批次内、相邻且来自不同 RM 的严格递减**。

**限定**：golden 的每收件人帧序是**pass 内按优先级、pass 间按时间**（装置多 pass 排水），
本判据**看不见 pass 边界** ⇒ 127 是"非优先级序"的**上界**，**不是**"golden 是伪影"的证明。
定性只能靠时间忠实重放（`StepDrainMs=1` 重出 8/37 的 golden：caster 若变成 `9, 638` 则 golden 旧件
是伪影；若仍 `638, 9` 则 oracle 真按优先级出队、须修产品）。

**已知映射缺口（登记）**：`UNMAPPED = SM 0 / 7 / 52 / 807`——`SM 0` 是短包占位（无 RM）；
`7`（`SM_RUSHKUNG`）与 `52`（`SM_ABILITY`）在本仓 **RM→SM 映射表里没有条目**；`807`
（`SM_SPACEMOVE_SHOW2`）对应的 `RM_SPACEMOVE_SHOW(8100)` 在该收件人的 `Rm` 里没有记录
⇒ 这些帧未参与判定，映射表与 `Rm` 记录面都要补。

### 3.3 与 `openmir2-10` 独立口径的交叉核对（**引用本卡时的标准三句**）

`10` 用**不依赖 SM→RM 映射**的办法：把 `multi_victim_same_tick` 放宽**拿掉**，逐收件人按 golden
帧字段做严格序比对（**在 `0e8988dd8` 的 38 份台账 / 176 个接收者**上跑出，可原样引用）：

- **STRICT 面：0 条顺序差异**；
- **RELAXED 面：22 条**（fire / skill10 / 施毒 4 支），全部落在**已登记**的放宽面内
  （例：`groupamyounsul/s1/caster` golden 第 2 位是 `640`、台账是 `656`；`skill10/s1/obs_in`
  golden `31(recog=1002)`、台账 `53(recog=1003)`——两例都是**不同 pass**）。

⇒ 本卡的三句结论：**① 模型上是系统性差异（Rust 57 处直发、oracle 全入队）；② 产品与 oracle
在 STRICT 面 0 差异、RELAXED 面 22 条且都在已登记放宽内；③ 本卡 §3 的 136 次是"oracle 自身
非优先级序"的**上界**（装置多 pass），**不是缺陷计数**——`10` 的读法与本卡一致：**候选、非缺陷**。**
（验收口径由 opc-19 定死：以 `Sent` 严格序 + 全量回归为准；本卡的普查只作排期依据。）

**让它可判定的改进（登记，未做）**：台账 `Sent` 目前只有 `due_ms_rel`、没有 `delivery`，
而 `Rm` 有 `delivery`；给 `Sent` 补 `delivery`（或 `StepDrainMs=1` 让批边界近似 1ms）后，
本卡的普查就能从"上界"变成"可判定"。

### 3.2 一行可复跑命令（台账长期复用）

```bash
cd E:/Users/gxh/Documents/GitHub/OpenMir2/.worktrees/magic-teeth && python -I - <<'PY'
import json,glob,os
M={9:8092,17:10008,31:8103,53:10052,100:11004,638:10120,639:10121,640:10123,642:10125,656:8090,657:10139,716:10205,807:8100,1100:10410}
rows=[]
for f in sorted(glob.glob("mir2-rs/tests/parity/golden/magic/*.json")):
    g=json.load(open(f,encoding="utf-8")); S=os.path.basename(f)[:-5]; fr=gr=iv=0; pr=set(); un=set()
    for c in g.get("cases",[]):
        for r in c.get("Receivers",[]):
            sent=r.get("Sent",[]);  fr+=len(sent)
            pm={m["ident"]:m.get("priority") for m in r.get("Rm",[])}
            def P(s):
                rm=M.get(s["ident"])
                if rm is None or pm.get(rm) is None: return None,None
                return rm,pm[rm]
            d={}
            for s in sent: d.setdefault(s.get("due_ms_rel","NA"),[]).append(s)
            for _,arr in d.items():
                seq=[]
                for s in arr:
                    rm,p=P(s)
                    if p is None: un.add(s["ident"])
                    else: seq.append((rm,p))
                if len(seq)>=2: gr+=1
                for a,b in zip(seq,seq[1:]):
                    if b[1]<a[1] and b[0]!=a[0]: iv+=1; pr.add((a[0],b[0]))
    rows.append((S,fr,gr,iv,sorted(pr),sorted(un)))
bad=[r for r in rows if r[3]>0]
for r in bad: print(r[0],"|",r[1],"|",r[2],"|",r[3],"|",r[4])
print("SET:",sorted(x[0] for x in bad),"COUNT:",len(bad),"INV:",sum(x[3] for x in bad))
print("UNMAPPED:",sorted({i for r in rows for i in r[5]}))
PY
```

## 4. 为什么现在没有大面积红

1. **多重集口径**：`fire` / `skill10` 的 golden 带 `frame_order_scope = multi_victim_same_tick`
   （与 `docs/issues/openmir2-b4-multivictim-strike-order.md` 同源），比较器只对该接收者放宽**次序**；
   `Rm` 一律多重集比较。
2. **pass 边界**：装置多 pass 排水把一部分"优先级序 vs 调用序"的差异吸收进录制序（见 §3.1 限定）。
3. **恰好同序**：其余严格按序比较的 stem，Rust 的调用序恰好与 golden 序一致（否则早已红）。

## 4.1 证据更正（2026-11-21）：本卡的修复证据**只有 8/37**

早先把 **21（`SKILL_SPACEMOVE`）** 也算作本卡证据是错的：21 当前的驱动/比较器红是
**换图簇缺 `SM_SERVERCONFIG(11029)` 一帧**（`golden 有 rust 无=[(11029,…)]`），属**另一条独立线**
（Session 加字段 + 登录同步，另卡已批）。**21 已从本卡证据中移除**——按队列模型去修 21 不会消掉那条红。
本卡的实证基础只剩 **8/37**：`SM_BACKSTEP(9)` 先于 `SM_MAGICFIRE(638)`，且 `StepDrainMs=1` 重放
仍为 `638, 9`（`43` 的实验）⇒ 不是 100ms 排水伪影。

## 5. 修复方案 v1（两段式）

**机制目标（不变）**：所有 `RM_*` 下发（**含发给自己的那条**）统一进**逐收件人**的
`NetPriorityQueue`，键 = `(byte)RM ident`（自延迟消息 255）；同优先级按入队序出队。

**阶段 1（stopgap，按站点，可独立合）**：只处理已确证的 8/37 —— 把该支效果体产生的
`RM_PUSH` 派生帧（`SM_BACKSTEP`）与共享尾的 `RM_MAGICFIRE` 派生帧（`SM_MAGICFIRE`）收进**同一个**
`SpellFrameBuf`，统一按优先级 flush。写集：`mir2-rs/bins/game-svr/src/lib.rs`（8/37 分支及其
`finish_spell_tail` 调用点）。**不含 21**（见 §4.1）。

**阶段 2（全局）**：把 §2 统计的 57 处直发收敛为入队。写集：同一 `lib.rs`（57 处调用点）+
视需要 `mir2-rs/crates/world-bevy/src/world.rs`（效果体内的发送）+ 比较器删除放宽（见 §6）。

**阶段 2 的前置（必须先做）**：让装置序可当基准——给台账 `Sent` 补 `delivery`（`Rm` 已有）或
全量 `StepDrainMs=1` 重出；否则队列化后 Rust 出优先级序、golden 记的是"pass 内优先级 + pass 间时间"，
会**修了产品、红了装置**（§7 具体例子）。

## 6. 移除条件（逐条可执行；"当 X 成立 ⇒ 删 Y，并要求 Z 绿"）

| # | 现在的放宽 | 移除条件（X） | 删除项（Y） | 验收（Z） |
| --- | --- | --- | --- | --- |
| 1 | `frame_order_scope=multi_victim_same_tick`（6 份 golden：`fire`、`skill10`、`amyounsul`、`amyounsul2`、`groupamyounsul`、`groupamyounsul2`；对应 MG-17/18/13/50） | 阶段 2 合并 **且**这 6 个 stem 的 `Sent` 严格序比对全绿 | 6 份 golden 的 marker + `real_ledger_verify_labels_opts` 的 stem 白名单 + `verify` 的 `multi_victim` 分支 | 全量 `--lib` + 全部 magic 比较器绿 |
| 2 | `Rm` 面通用多重集比较 | 台账 `Rm` 记录 `delivery`/pass 号（装置改动）后 | 把 `Rm` 改为按 `(priority, 入队序)` 严格比 | 全部比较器绿 |
| 3 | 阶段 1 的站点级 `SpellFrameBuf` 收口 | 阶段 2 落地 | 阶段 1 的局部收口代码 | 8/37 对应 stem 的 `real_rust_ledger_*` 严格序绿 |

## 7. 全量修复的预期红（具体前后对比）

- **`fireboom/s1/target`**：golden 现序 `[17, 638, 53, 31, 656]`；按 RM 优先级
  （24 / 68 / 136 / 154 / 167）应为 `[17, 53, 638, 656, 31]` ⇒ **53↔638、656↔31 两组换位**。
- 同类 stem（§3 普查的 18 个里除 6 个已放宽的）：`fireboom`、`fireboom47`、`lightflower`、`snowwind`、
  `grouplightening`、`lightning-11`、`lightning-45`、`fireball`、`fireball-5`、`firecharm`、`hbfireball`、
  `showhp` —— 它们现在绿是**恰好同序**，阶段 2 后**必先红**，必须与装置序基准化同批处理。
- 验收口径（opc-19 定死）：`Sent` 严格序 + 全量回归。

## 关联

- `docs/issues/openmir2-b4-multivictim-strike-order.md`（同源的多受害者同刻次序产品差异）
- `docs/issues/openmir2-magic-640-delay-sweep.md`（同一类"装置多 pass 伪影"的既有实例）
