# `param_discovery` 的 computed 打包文案订正（`openmir2-param-discovery-oracle-text-fix`）

- 卡：`openmir2-param-discovery-oracle-text-fix`（tier=L）；分支 `card/openmir2-param-discovery-oracle-text-fix`；写集 `tools/probe/param_discovery.py`、`docs/`。
- 基线：`walgit/master = a112e59b5`（开工时）。来源：`openmir2-scenario-recog-packing-fix` 返工时点名、但落在该卡写集之外的残留；`docs/scenario-recog-packing-20261023.md`（返工版）已成表。

## 1. oracle 逐条出处（本卡自己核过）

| 事实 | 出处 |
|---|---|
| 动作族（`CM_TURN`/`CM_WALK`/`CM_SITDOWN`/`CM_RUN`/`CM_HIT`/`CM_*HIT`…）**打包**：`recog = (Y<<16) | X`、`Tag = Dir`；**Param 不携带坐标** | `src/GameSrv/Word/WorldServer.cs:832-857`：`SendActionMsg(Ident, Tag, LoWord(Recog), HiWord(Recog), 0, "")` 或 `SendMsg(…, Tag, LoWord(Recog), HiWord(Recog), 0)` |
| `CM_WALK`/`CM_RUN` 的坐标是**目标格** | `src/M2Server/Player/PlayObject.Message.cs:1059-1060`（`ClientWalkXY(ident, (short)nParam1, (short)nParam2, …)`）→ `src/M2Server/Player/PlayObject.Attack.cs:1365-1428` |
| `CM_HIT` 一族的坐标是**攻击者自身格** | `PlayObject.Message.cs:1221-1230`（`ClientHitXY(ident, nParam1, nParam2, (byte)wParam, …)`）→ `PlayObject.Attack.cs:823`/`:883` 的 `nX == CurrX && nY == CurrY` 门 |
| `CM_SPELL` 是**异类**：不走动作族，坐标 X=Recog、Y=Param、目标 actor=Tag | `WorldServer.cs` 的 `default` 支（`SendMsg(Ident, Series, Recog, Param, Tag, …)`）→ `PlayObject.Message.cs:1339-1341` 的 `ClientSpellXY(…, (short)nParam1, (short)nParam2, nParam3)` |

> 旧文案的错在最后半句：`move_y` 写「`nParam1=X → recog，nParam2=Y → param`」——把 **Y 说成 Param**，与 `WorldServer.cs:832-857` 的 `HiWord(Recog)` 相反；这正是会把场景作者引向错误写法的根因。

## 2. 改了什么（只改文案 + 一条文案断言）

1. 模块 docstring 的 `move_x`/`move_y` 行：改为「同一公式、语义按 ident 区分」并带四条出处（见 §1）。
2. `COMPUTED_ORACLES["move_x"]`：写明打包口径（`recog=(Y<<16)|X`、`Tag=Dir`、动作族 Param 不携带坐标）+ `CM_WALK/CM_RUN` 目标格 / `CM_HIT` 族自身格 两义；
   `COMPUTED_ORACLES["move_y"]`：写明 `Y = HiWord(Recog)`，并单列 `CM_SPELL` 的特例（X=Recog、Y=Param）。
3. `selftest` 新增用例 **㉘**：断言上述关键短语与禁止旧错说法（`"nParam1=X → recog，nParam2=Y → param" not in …`）。

**未改行为**：`computed` 的推导/打包实现与 DSL 语义一行未动；`mir2_probe.py`、`tools/conformance/**`、场景 JSON、Rust 源码均未触碰。

## 3. 判据与证据

| 项 | 结果 |
|---|---|
| `python -I -B tools/probe/param_discovery.py selftest` | **OK（39 cases + 11 teeth）**（改前 38 cases；+1 用例，teeth 仍 11） |
| `python -I -B tools/probe/mir2_probe.py --selftest` | **OK（9 checks）**——条数前后一致 |
| 牙：把 `move_y` 改回旧错说法 | `selftest` **rc=1** 并点名 `㉘ … 同上（nParam1=X → recog，nParam2=Y → param）`；`git checkout -- tools/probe/param_discovery.py` 还原，`sha256(file)==sha256(git show HEAD:<path>)` **True**，还原后 `selftest` rc=0 |
| `t_reservation_gate check` | GREEN（本卡未动 T 号） |
| 冲突标记门 | OK（0 markers） |
| `git merge-tree --write-tree walgit/master HEAD` | rc=0 |

## 4. 残留

- 只改文案与一条文案断言；`COMPUTED_ORACLES` 仍只列 `move_x`/`move_y`（其余 computed 变量走通用文案）。
- `CM_SPELL` 的坐标字段布置已写进 `move_y` 的说明，但本工具没有法术场景的 computed 用法（`spell_cast` 的 `required` 不含坐标变量）⇒ 不新增行为。
- 无临时产物；变异/还原证据见 §3（sha256 校验）。
