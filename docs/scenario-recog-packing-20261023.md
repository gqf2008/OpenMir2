# 场景 `recog` 打包口径（CM 动作族）与既有夹具复核

卡 `openmir2-scenario-recog-packing-fix`（tier=L）。写集：`tools/probe/scenarios/`、`tools/probe/scenarios/README.md`、`docs/`。
快照：base `04366adc8`，复核时间 2026-10-23（UTC 12:5x）。**本卡只修夹具，不跑真机窗口。**

## 1. 口径（公式）

`TDefaultMessage` 的 `Recog` 是 32 位；走「动作族」case 组的命令**只用它**：

```
recog = (y << 16) | x        # LoWord = X，HiWord = Y
param 恒 0（服务端不读）
tag   = 方向 / 其它语义（按命令）
```

oracle 证据：

- `src/GameSrv/Word/WorldServer.cs:831-851`：`CM_HORSERUN / CM_TURN / CM_WALK / CM_SITDOWN / CM_RUN / CM_HIT /
  CM_HEAVYHIT / CM_BIGHIT / CM_POWERHIT / CM_LONGHIT / CM_CRSHIT / CM_TWINHIT / CM_WIDEHIT / CM_FIREHIT`
  同一 case 组，两支（`:846` `SendActionMsg(Ident, Tag, LoWord(Recog), HiWord(Recog), 0, "")` /
  `:850` `SendMsg(playObject, Ident, Tag, LoWord(Recog), HiWord(Recog), 0)`）**都只读 `Recog` 的 Lo/Hi，
  `defMsg.Param` 完全没有被读**。
- `src/M2Server/Actor/BaseObject.Message.cs:34/98`：`SendMsg(IActor, wIdent, wParam, nParam1, nParam2, nParam3, sMsg)`
  ⇒ 上句的实参顺序 = `wParam=Tag`、`nParam1=LoWord(Recog)`、`nParam2=HiWord(Recog)`、`nParam3=0`。
- 消费端：`src/M2Server/Player/PlayObject.Message.cs:1059-1060`（`CM_WALK → ClientWalkXY(nParam1=X, nParam2=Y)`）、
  `:1221-1230`（`CM_HIT → ClientHitXY(nParam1=X, nParam2=Y, (byte)wParam=Dir)`）。

## 2. 逐文件复核（`tools/probe/scenarios/` 里凡发送 `CM_*` 的场景）

| 文件 | ident | 当前写法（改前） | oracle 口径 | 结论 | 处置 |
|---|---|---|---|---|---|
| `combat_hit.json` | 3014 `CM_HIT` | `recog=target_x, param=target_y` | 动作族：X/Y 都来自 `recog`；`param` 不读（`WorldServer.cs:831-851`；`PlayObject.Message.cs:1230`） | **错**：Y 恒为 `HiWord(recog)=0` ⇒ 目标落在 `(x, 0)` | 改为 `recog=target_xy`、`param=0`；`target_xy=(target_y<<16)|target_x` 由调用方传入 |
| `move_walk.json` | 3011 `CM_WALK` | `recog=move_x, param=move_y` | 同上（同 case 组；`PlayObject.Message.cs:1060`） | **错**（同一模式） | 改为 `recog=move_xy`、`param=0`；`move_xy=(move_y<<16)|move_x` |
| `bag_equip.json` | 1003 `CM_TAKEONITEM` | `recog=make_index, param=takeon_where` | 该命令走 `WorldServer.cs:796-812` 组：`SendMsg(…, Series, Recog, Param, Tag)` ⇒ `nParam1=Recog`、`nParam2=Param`；`PlayObject.Message.cs:820-821` `ClientTakeOnItems((byte)nParam2=where, nParam1=MakeIndex)` | **对** | 不动 |
| `eat_item.json` | 1006 `CM_EAT` | `recog=make_index` | 不在 switch ⇒ `default`（`WorldServer.cs:856-857`）：`nParam1=Recog`；`PlayObject.Message.cs:826-827` `ClientUseItems(nParam1, Msg)` | **对** | 不动 |
| `pickup_item.json` | 1001 `CM_PICKUP` | `recog=0, param=self_x, tag=self_y` | `default` 同上；`PlayObject.Message.cs:811-815` 要求 `CurrX==nParam2(=Param) && CurrY==nParam3(=Tag)` | **对** | 不动 |
| `gm_time.json` | 3030 `CM_SAY` | 全 0 | `WorldServer.cs:853-854` 只取文本，参数全 0 | **对** | 不动 |

> 其余场景（`dice_chain*`, `king_*`, `merchant_dialog`, `npc_click*`, `npc_script_chain`, `login_only`,
> `gm_batch`, `king_castle_war`）不发送本族的 `CM_*`（只发 `click_npc` / `dlg_select` / `say` / 1011 等），不在本表。

## 3. DSL 现状与设计说明（**本卡不改 DSL**）

`mir2_probe` 的取值只有两种：int，或 `params`/`--var` 里同名变量的十进制 int（`resolve_param`）。
`computed` 只是给 `param_discovery` 的**声明性注记**（只支持 `derived_from` + `delta` 加法），probe 不读它。
⇒ **当前 DSL 没有任何「位运算/打包」表达方式**，所以：

- `combat_hit.json`：`target_x/target_y` 保留在 `required`（供人/工具读），新增 `target_xy`（打包值）；
- `move_walk.json`：`computed` 的 `move_x/move_y` 声明**保留**（`param_discovery` 仍按 `self_x+1 / self_y+0` 推导），
  新增 `move_xy`；
- 打包公式写进两个场景的 `description` / `note` 与本文件，调用方（窗口操作者或未来工具）按公式算好传入。

**建议的后续卡（不在本卡写集内）**：给 `computed` 增加一种打包声明（例如
`{"pack": {"lo": "move_x", "hi": "move_y"}}`），让 `param_discovery` 直接推出 `move_xy`/`target_xy`，
从而 `card_window` 的自动发现能闭环；在那之前这两条场景在 `card_window` 里会因为 `*_xy` 推不出而 **fail-closed**。

## 4. 影响面（改动前必读）

- `move_walk.json` / `combat_hit.json` 的**线上字节变了**（这正是修复点）：`card_window` 自动发现
  `move_xy`/`target_xy` 会报 missing ⇒ **这两条场景在窗口里暂时跑不了**（rc=2，fail-closed，不再是「静默打错坐标」）。
  直接跑 `mir2_probe.py` 时手工传 `--var move_xy=<(y<<16)|x>` 仍可用。
- 引用这两条场景的卡片/文档（需在其窗口项里补打包值或等后续卡）：
  `docs/window-batch-plan-20261023.md` 的 #1/20/28/33/34/36/38/40/44/45/63/65 等、
  `docs/conformance-a-card-scenario-map-20261023.md` §320-321、
  `docs/param-discovery-design-20261023.md` §2.5 的示例输出。
- **本卡写集外的既有不一致（不改，只登记）**：`tools/probe/param_discovery.py` 的 `COMPUTED_ORACLES`
  里 `move_y` 的 oracle 文案写着「`nParam1=X → recog，nParam2=Y → param`」——与 oracle 实际不符
  （`Recog` 里同时装 X/Y，`Param` 不读）。该文件在本卡 `forbidden` 列表内，建议并入 §3 的后续卡一起改。

## 5. 牙

```
# 正确写法（提交后树 5aac39325，sha256=ed722696dc33d83de2e36e414983819cc7b3ce21e03a95a8871b5048eddd78de）
combat_hit.json  ident=3014 recog=2097182(0x20001E) param=0 tag=2 => server(X,Y)=(30,32)
move_walk.json   ident=3011 recog=2097183(0x20001F) param=0 tag=0 => server(X,Y)=(31,32)

# 牙：把 combat_hit 改回 recog=target_x, param=target_y
combat_hit.json  ident=3014 recog=30(0x1E) param=32 tag=2 => server(X,Y)=(30,0)   ← Y 恒 0（错）

# git checkout -- tools/probe/scenarios/combat_hit.json
sha256sum = ed722696dc33d83de2e36e414983819cc7b3ce21e03a95a8871b5048eddd78de   ← 与提交后树一致
combat_hit.json  ident=3014 recog=2097182(0x20001E) param=0 tag=2 => server(X,Y)=(30,32)
```

（脚本 `E:/tmp/w24_recog_tooth.py`：用 probe 自己的 `load_scenario`/`expand_scenario` 打印展开后的线上字段，只读。）

见 §7 实测：把 `combat_hit.json` 改回 `recog=target_x, param=target_y` ⇒ 用 probe 的 `expand_scenario`
打印出来的展开值从 `recog=2097182 (0x20001E = y32/x30), param=0` 变成 `recog=30, param=32`
（服务端看到的目标 = `(30, 0)`），`card_window --dry-run` 的 `--var` 序列也随之变化；逐字节还原后 `sha256sum` 一致。

## 6. 残留物清单

- `E:/tmp/w24-dry-combat/`、`E:/tmp/w24-dry-move/`（`card_window --dry-run` 的预演目录，未起栈）；
  `E:/tmp/w24_recog_tooth.py`（牙的展开值打印脚本，只读）。
