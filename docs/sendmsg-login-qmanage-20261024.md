# `SENDMSG 0..4` 在 `@Login` / QManage / `@PlayDie` 三条排水路的接线与枚举

卡 `openmir2-sendmsg-login-qmanage-cases`（tier=R）。前置：`openmir2-script-sendmsg-host` 返工
`d592331ca` 已合入 master `01d4f5877`（`@StdModeFunc*` 扇出 + 复用 `fan_out_world_sysmessage`）。
承接 whitelist **T-224 ④** 的残余，本卡登记 **T-267**。

## 1. 目标与边界

把三条路在部署语料里的 `SENDMSG` case **先枚举、再接线**：

- `@Login` / QManage：`NpcHost::run_login` → `QManage.txt [@Login]`（生产入口 = `sim_loop` 进世界块）；
- `@PlayDie`：`FunctionNPC.GotoLable("@PlayDie")`（**oracle 层没有触发点**，见 §2.3）。

硬边界：**复用既有扇出**（`fan_out_world_sysmessage`），不另写一份；**不实现**部署不可达的分支。

## 2. 部署语料枚举（可复跑）

### 2.1 命令

```sh
cd /e/MirServer/M2GameSvr/Envir
# ① 每条 SENDMSG 的行号 + 所属 label（GBK 原文件；label 名可用 iconv 看）
awk 'BEGIN{lbl="?"} /^\[/{lbl=$0}
     /^[[:space:]]*[Ss][Ee][Nn][Dd][Mm][Ss][Gg][[:space:]]+[0-9]/{
       match($0,/[Ss][Ee][Nn][Dd][Mm][Ss][Gg][[:space:]]+[0-9]/);
       print FILENAME":"FNR" ["lbl"] -> "substr($0,RSTART,RLENGTH)}' \
  MapQuest_def/QManage.txt Market_Def/QFunction-0.txt \
  QuestDiary/月卡/月卡系统.txt QuestDiary/游戏登陆/登陆脚本.txt
# ② `@Login` 的 #CALL 链（决定「QManage 本体 0 条」是否等于「@Login 路 0 条」）
grep -n "#CALL" MapQuest_def/QManage.txt
# ③ 假人链（#CALL 的下一跳）是否另有 SENDMSG
for f in QuestDiary/LEGM2.CC假人系统/假人登录脚本.txt \
         QuestDiary/LEGM2.CC假人系统/假人初始化设置.txt \
         QuestDiary/LEGM2.CC假人系统/假人回盟重省.txt; do
  echo "== $f"; iconv -f cp936 -t utf-8 "$f" | grep -n -i sendmsg
done
# ④ @PlayDie 的 oracle 触发点（结论 = 无）
grep -rn "SPLAYDIE\|NPLAYDIE\|@PlayDie" --include=*.cs /e/tmp/wt-worker-58/src | grep -v ScriptFlagConst
```

### 2.2 `@Login` / QManage 实测

| 文件:行 | label | case | 可达 | 引擎 | 宿主（改前） |
|---|---|---|---|---|---|
| `MapQuest_def/QManage.txt:17-38` | `[@Login]` | **无** | — | — | — |
| `QuestDiary/月卡/月卡系统.txt:8` | `[@月卡]`（`#CALL` from `QManage.txt:36`） | 5 | 是 | 已实现（自见 `SysMsg`） | 已排水（`frames`） |
| `QuestDiary/月卡/月卡系统.txt:15` | 同上 | 7 | 是 | 同上 | 已排水 |
| `QuestDiary/月卡/月卡系统.txt:16` | 同上 | 5 | **否**（同 `#ACT` 第二个动作） | 已实现 | 不执行 |
| `QuestDiary/月卡/月卡系统.txt:23` | 同上 | 5 | 是 | 已实现 | 已排水 |
| `QuestDiary/游戏登陆/登陆脚本.txt:17` | `[@登陆设置]`（`#CALL` from `QManage.txt:37`） | 5 | 是（`ISNEWHUMAN`） | 已实现 | 已排水 |
| `QuestDiary/游戏登陆/登陆脚本.txt:18-22` | 同上 | 6×5 | **否**（同 `#ACT` 后续动作） | 已实现 | 不执行 |
| `MapQuest_def/QManage.txt:5-13` | `[@Resume]` | 5×9（仅 `:5` 执行） | **否**（无 `GotoLable("@Resume")`） | 已实现 | 无触发点 |
| `MapQuest_def/QManage.txt:45` | `[@Member]` | 7 | **否**（`MemberFunction` 命令本仓未实现） | 已实现 | 无触发点 |
| 假人链（`假人登录脚本.txt` / `假人初始化设置.txt` / `假人回盟重省.txt`） | — | **0** | — | — | — |

**结论**：`@Login`/QManage 路在部署语料里 **0 条 case 0-4**（全是自见 case5/6/7）。
接线是「任何脚本内容都不该被静默丢弃」，用例按**构造级**（与 T-194 的 MESSAGEBOX 同法）。

**oracle 顺带订正（原始行 ≠ 帧数）**：`GotoLableQuestActionProcess`（`ScriptEngine.cs:886-895`）
对每个 `#ACT` 的 action 列表**只执行第一个已注册动作**后 `return` ⇒
`[@Resume]` 的 9 行只产 1 帧、`登陆脚本.txt` 的 6 行只产 1 帧。本卡枚举表已按「执行」而非「行数」标注。

### 2.3 `@PlayDie` 实测（**oracle 不可达**）

| 文件:行 | label | case | 结论 |
|---|---|---|---|
| `Market_Def/QFunction-0.txt:50/51` | `[@PlayDie]`（`#IF KillByHum`） | 1×2（`:50` 执行） | **不可达** |
| `Market_Def/QFunction-0.txt:55/56` | `[@PlayDie]`（`#ELSEACT`） | 1×2（`:55` 执行） | **不可达** |
| `Market_Def/QFunction-0.txt:62` | `[@假人上线A]`（`DelayCall` from `:52/:54`） | 0 | **不可达**（唯一入口就是 `@PlayDie`） |

依据：oracle `PlayObject.Die()`（`PlayObject.cs:1969-1998`）只做退组 + `RM_MASTERDIEMUTINY`，
**从不** `FunctionNPC.GotoLable(..., "@PlayDie", ...)`；`ScriptFlagConst.SPLAYDIE = "@PLAYDIE"` /
`NPLAYDIE = 41` 在全仓**无消费点**。⇒ 本卡**不实现**，只做构造级用例并登记 T-267。

**「引擎本身没实现」= 0 条**：这三条路上出现的所有 case（0-7）在 `crates/script` 的
`ActionOfLineMsg`（`engine/actions.rs:216-265`）里都已实现。缺口只可能在**宿主排水**。

## 3. 实现

`mir2-rs/bins/game-svr/src/lib.rs`：

- 新增生产排水点 **`drain_login_script_batch`**（`sim_loop` 进世界块调它）：
  1. `emit_login_script_frames`（本例玩家脚本帧，`NpcPlayer.frames` 的 `MsgQueue` 投影；T-194 口径不变）；
  2. `fan_out_world_sysmessage`（`world_broadcasts` 的**其余** `!Ghost` 在线玩家，逐收件人一个
     `NetPriorityQueue`，`Recog` = 收件人自己）。
- `@Login` 脚本块的 out 元组多带一份 `std::mem::take(&mut player.world_broadcasts)`（与 `frames`
  同批、`player` 析构前）；扇出在**写回之前**（T-194 已知残余口径不变）。
- QManage 是隐藏 NPC（`Envir == null`）⇒ 本路**不扇** `SM_HEAR`（`SENDMSG 4` 本就零帧，T-224 ④）。
- `@PlayDie`：**零代码改动**（不可达，登记）。

## 4. 用例（3）与牙（2）

`cargo test -p game-svr --lib -- login_sendmsg qmanage_login_drain playdie_constructed`（3 passed）：

| 用例 | 钉住什么 |
|---|---|
| `login_sendmsg_case0_fans_out_to_other_online_player` | `@Login`（真 GBK 解析器）`SENDMSG 0` ⇒ 触发者 1 帧 + **另一在线玩家** 1 帧，`Recog` 各取自己、逐字节；**Ghost 零帧** |
| `qmanage_login_drain_fans_out_case2_and_keeps_case7_self_only` | QManage：case2 前缀 `[QManage]` 扇出到他人；**case7 只进触发者帧表**（蓝字 `0xFF/0xFC`）；同 ident 两条按 `[0,1]` 出队 |
| `playdie_constructed_case1_fans_out_through_qfunction_drain` | **构造级**（部署不可达）：`(*) ` 前缀逐字节；另一玩家收到、`Recog` 各取自己；Ghost 零帧 |

牙（还原 `git checkout -- <file>`，`sha256sum` == `git show <head>:<path>`）：

1. `drain_login_script_batch` 里 `fan_out_world_sysmessage` 的正文实参换 `&[]` ⇒ 用例 1/2 红；
2. `crates/script/src/engine/actions.rs` 的 case2 前缀 `[{npc_name}]` 改错 ⇒ 用例 2 红。

## 5. 窗口项（R，**未跑**；可复跑命令）

前置照 `docs/window-runbook-20261023.md` §1（重建 `mir2-server` 并记 sha256、十端口全 DOWN、
两侧库各自复位、`ident=100` 先在 `tools/conformance/rules.json` 登记）。影子代理自检：
`python -I -B tools/gw-capture/gate_shadow_proxy.py --selftest` ⇒ GREEN 4/4。

> 三条路都用**构造级脚本**（部署 `@Login`/QManage 路 0 条 case 0-4；`@PlayDie` 不可达）——
> 这是「构造级」而非「真机可达性」证据，报告里必须写明。两侧跑**同一份**脚本。

### W-1 `@Login`

构造 `E:/tmp/w267/login/Envir/MapQuest_def/QManage.txt`：

```
[@Login]
#IF
CHECKLEVELEX > 0
#ACT
SENDMSG 1 构造登录广播
```

```powershell
# 每侧各跑一次（<side> = csharp | rust），影子 SelGate 抓下行
python -I -B tools/gw-capture/gate_shadow_proxy.py --out E:/tmp/w267/<side>/login `
  --map 7101=127.0.0.1:7100 --gate sel --direction s2c
# 第二个观察者在线 → 用 login_only 场景登录触发者，停栈后对账
python -I -B tools/gw-capture/gate_shadow_proxy.py --verify E:/tmp/w267/<side>/login
python -I -c "import json,sys;rows=[json.loads(l) for l in open(sys.argv[1],encoding='utf-8') if l.strip()];print([r['sha256'][:12] for r in rows if r.get('ident')==100])" E:/tmp/w267/<side>/login/frames.ndjson
```

判读：两侧 `ident=100` 的指纹序列**逐项相等**；每条 `Recog` = 该帧**收件人自己**、
`Param = MakeWord(RedMsgFColor, RedMsgBColor)`（现网 `0xFF/0x38`）、尾解码 = `(*) 构造登录广播`；
收件人集合 = 全服所有 `!Ghost` 在线玩家（含触发者）。

### W-2 QManage（`#CALL` 链形状）

构造 `E:/tmp/w267/qmanage/Envir/MapQuest_def/QManage.txt`
（`[@Login]` 只 `#CALL`，脚本本体在被调文件里——与部署 `QManage.txt:36/37` 同形状）：

```
[@Login]
#ACT
#CALL [\ZZ-W267\ZZ-W267.txt] @构造广播
```

`E:/tmp/w267/qmanage/Envir/QuestDiary/ZZ-W267/ZZ-W267.txt`：

```
[@构造广播]
#IF
CHECKLEVELEX > 0
#ACT
SENDMSG 2 构造店招
```

```powershell
python -I -B tools/gw-capture/gate_shadow_proxy.py --out E:/tmp/w267/<side>/qmanage `
  --map 7101=127.0.0.1:7100 --gate sel --direction s2c
python -I -B tools/gw-capture/gate_shadow_proxy.py --verify E:/tmp/w267/<side>/qmanage
```

判读：与 W-1 同口径，但正文前缀必须是 `[QManage]`（case2 用 NPC 名，不是玩家名）；`#CALL`
不改变扇出与收件人集合。两侧指纹序列与尾部解码序逐项相等。

### W-3 `@PlayDie`（构造级；部署不可达）

构造 `E:/tmp/w267/playdie/Envir/Market_Def/QFunction-0.txt`：

```
[@PlayDie]
#ACT
SendMsg 1 构造死亡广播
```

```powershell
python -I -B tools/gw-capture/gate_shadow_proxy.py --out E:/tmp/w267/<side>/playdie `
  --map 7101=127.0.0.1:7100 --gate sel --direction s2c
python -I -B tools/gw-capture/gate_shadow_proxy.py --verify E:/tmp/w267/<side>/playdie
```

判读：**先要在该侧注入一次 `@PlayDie` 触发**（C# 侧临时补一行 `GotoLable(..., "@PlayDie", false)`
或等价探针；两侧脚本内容相同）。比 `ident=100` 的 `Recog/Param/尾`（`(*) ` 前缀）与收件人集合。
**本项只证明「若有触发点则帧面一致」，不证明部署可达**（见 §2.3）。

## 6. 门禁

见 patch body（fmt / clippy `-D warnings` / `cargo test --workspace` / msgcodegen / anchor /
gm_registry / noop `check --strict-lines` + selftest / T 预约表 / 冲突标记 / merge-tree）。

## 7. 残余物清单

- 本卡**未起栈、未占端口、未连库**；`src/**`（C#）未改。
- 临时物：`E:/tmp/w267/**`（窗口期构造脚本，窗口跑完删）；本卡实现期未留脚本。
- 窗口未跑 ⇒ R 卡合并后保持 `in-progress`。
