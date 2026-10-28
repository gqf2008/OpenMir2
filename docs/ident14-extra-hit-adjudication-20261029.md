# ident14 额外 SM_HIT 受控裁决（T-270）

- 卡：`openmir2-ident14-extra-hit-adjudication`（tier=R，window_required=true）
- 登记：`mir2-rs/tests/parity/whitelist.md` T-270；规则表 `tools/conformance/rules.json` 的
  `rules[ident=14].oracle_extra_shapes`
- 实现：`tools/conformance/compare_runs.py`（`oracle_extra_explainable` + 库比完之后的放行闸）
- 关系卡：`openmir2-ignore-rule-count-compare`（counts+shape 门槛）、
  `openmir2-real-machine-verification-master`（真机窗口项）

## 1. 结论（先给判据）

C# 侧多出的 `SM_HIT(14)` 在**逐帧验证为零可观测副作用**后，按**形状白名单 + 四条件**受控放行；
不恢复整类 `ignore`，也不放宽 `ignore_rule_census` 的 counts+shape 门槛。任何一条不成立即保持
`UNDECIDABLE`。

## 2. A1：逐帧证据（零可观测副作用）

`SM_HIT(14)` 是**展示帧**：`BaseObject.SendRefMsg(RM_HIT, Dir, CurrX, CurrY, 0, "")`
（`src/M2Server/Actor/BaseObject.Message.cs:383` 的签名 `SendRefMsg(wIdent, wParam, nParam1, nParam2, nParam3, sMsg)`）
经 `PlayObject.Message.cs:1433-1437` 转成 `MakeMessage(SM_HIT, ActorId, nParam1, nParam2, wParam)`
⇒ 线上 `param=X / tag=Y / series=Dir`，**body 恒空**，不带 HP/伤害/序号。

| 运行 | 场景 | 额外 14 | 帧位置 | recog（actor） | param/tag（X,Y） | series（Dir） | body | 该 actor 的位置帧 | 其它帧 | 库快照 |
|---|---|---|---|---|---|---|---|---|---|---|
| run12 | softclose_relogin | 1 | 流末第 72 帧（t=11971ms） | 60597838 | 318,336 | 4 | 空（`e3b0c442…`） | ident 10 `recog=60597838` 正是 (318,336) | 其余 71 帧逐字节相同，diffs=0 | 比过，diffs=0 |
| run16 | login_only | 0 | — | — | — | — | — | — | — | 比过，diffs=0 |
| run17 | move_walk | 0 | — | — | — | — | — | — | — | 比过，diffs=0 |
| run21 | gamepoint_exchange | 2 | 第 32/34 帧（t=5800/7137ms） | 29729721 | 49,49 | 5 | 空（`e3b0c442…`） | ident 10 `recog=29729721` 正是 (49,49) | 其余 32 帧逐字节相同，diffs=0 | 比过，diffs=0 |

⇒ 额外帧**只有显示语义**：actor 在自己所在坐标、朝自己的朝向「受击」；前后帧、目标 HP 类帧
（本场景无 31/65）、库后态全都不变。「C# 多发一条展示帧」不等于「Rust 少发一条真帧」——
后者会在 counts 上体现为 rust 侧多出/形状不符，本裁决一律不放行。

## 3. 机制（compare_runs.py）

两段式：帧面先只判「形状对得上」（`oracle_extra_explainable`），放行与否等库比完再定。

`oracle_extra_explainable(row, rule)` 返回 True 的全部必要条件：

1. 规则声明了 `oracle_extra_shapes`，且没有解析不了的条目（`param,tag,series` 三段整数）；
2. **rust 侧没有 oracle 没有的形状，也没有 rust 计数 > oracle 计数的形状**（Rust 多发不算）；
3. 所有计数不等的形状都在白名单里，且 `body_sha256` 是空体哈希（有载荷不放行）。

库比完之后的放行闸（四条件，缺一即保留原 `UNDECIDABLE` 并写
`adjudication: "T-270"` + `adjudication_blocked_by`）：

1. 帧面除该 ignore 差异外**没有别的 diffs**；
2. 两侧库快照都读到且比过（`state=compared`）；
3. 库无 diffs；4. 库无 undecidable。

放行时写 `frames.oracle_extra_hits[]`（ident / 每形状两侧计数 / 卡号 / T-270 / tier=R / `verified`），
并进 `whitelisted`（`kind=oracle_extra_hit`）——**显式登记，不是静默掩码**。
`reason` 与 `verified` 按**本次实际条件**生成（`db_state` / `db_diffs` / `db_undecidable` / `frame_diffs`），
不预写「库比过且无差异」（reviewer-21 F2）。

## 4. 牙与变异（`compare_runs selftest`，离线合成输入）

| 牙 | 断言 |
|---|---|
| 正向 | oracle 多 1 条白名单形状空体 14、无其它帧差、库全等 ⇒ **rc 0** 且 `oracle_extra_hits` 非空、`whitelisted` 含 `oracle_extra_hit` |
| 伴随库差 | 同一帧面但 rust 侧 Level 7→8 ⇒ **rc 2**，`adjudication_blocked_by` 点明库 |
| 伴随帧差 | 同帧面但 41 的 body 改一字节 ⇒ **rc 2**，`blocked_by` 点明帧面 |
| 形状不符 | 额外帧 param/tag=7,7 ⇒ **rc 2** |
| body 非空 | 额外帧带 `b"x"` ⇒ **rc 2** |
| Rust 多发 | 把同形帧放到 rust 侧 ⇒ **rc 2** |
| 库未比（reviewer-21 F1） | 两侧都没有库快照（`state=absent_both`）⇒ **rc 2**，`blocked_by` 点明「库未比」；实测 head `55a38412` 上删掉 run12 两侧快照**不会**变 PASS（rc 2 / `absent_both` / 无 hits） |
| 库判不了（reviewer-21 F1） | 库 `state=compared`、diffs=0 但库侧有 undecidable（run.json 缺 `--char-name`）⇒ **rc 2**，`blocked_by` 点明「判不了」 |

变异（逐次恢复并核 sha256，无 MUTATION 残留；`compare_runs.py` 恢复后
`5a5ac4d165eff1070ec61dc98b9ce105e118592ca21aabe8c4840ee948fad315`）：

| 变异 | 结果 |
|---|---|
| 删白名单形状检查 | 形状不符牙红 |
| 删空体检查 | body 非空牙红 |
| 删「rust 多发」检查 | Rust 多发牙红 |
| 删「帧面其它差异」条件 | 伴随帧差牙红 |
| 删「库有差异」条件 | 伴随库差牙红 |
| 删「库未比」条件 | 库未比牙红（rc 0 / `absent_both` / 无 hits） |
| 删「库判不了」条件 | 库判不了牙红 |

五条变异各自只打红对应那颗牙（其余 383 条仍绿）。

## 5. 离线重算（A4 的离线部分）

用当前分支工具对既有窗口产物重算（命令：`compare_runs.py compare --oracle <run>/<sc>/csharp
--rust <run>/<sc>/rust --out …`）：

| 运行 | 判定 | 说明 |
|---|---|---|
| run5 softclose_relogin | **UNDECIDABLE**（rc 2） | 额外帧形状 `318,332,7` **不在**白名单 ⇒ 保持原判（父卡的兜底未被本卡洗绿） |
| run12 softclose_relogin | **PASS**（rc 0） | `oracle_extra_hits` 1 条（318,336,4） |
| run16 login_only | PASS | 无 14 帧 |
| run17 move_walk | PASS | 无 14 帧 |
| run21 gamepoint_exchange | **PASS**（rc 0） | `oracle_extra_hits` 2 条（49,49,5） |

除上面两条放行外，仓库里其它窗口运行（run3 castle/dice/move_walk、run4 butch、run5 upgrade_retrieve 等）
的 14 形状都**不在**白名单，仍按 `ignore_rule_mismatch` 判 `UNDECIDABLE`——本裁决不是「整类 14 放行」。

## 6. 残余风险 / 未完成

- **白名单是坐标级的**：`318,336,4` 与 `49,49,5` 是这两次运行里 actor 的真实坐标+朝向；换地图/换站位
  会出现新的合法形状，需要按同样证据逐个登记（不会自动放行）。
- **只证明「这两次运行零副作用」**：不能推出「所有 C# 额外 14 都零副作用」。确定性支（城堡商人
  `FixedHideMode` 解除补发，`Merchant.cs:152-157`）与攻击命中仍走原路径，没有被本裁决覆盖。
- **真机复跑未做（A4 的窗口部分）**：窗口受 Leader 排期，本卡不自行起栈；待窗口内在当前 master
  复跑至少一条场景并把 `oracle_extra_hits` 与库/帧面证据入卡。
- 父卡的真机项（ident14「不该出现的一侧不发」）仍在 `openmir2-real-machine-verification-master`
  （`fdc4bbb27`）。
