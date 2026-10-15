# 对账门（coverage recon gates）—— 完成定义 §4-1 / §4-2

卡：`openmir2-coverage-recon-gates`。依据：`docs/移植计划重审-20261015.md` §1（红名单）/ §4-1 / §4-2。

本目录提供**两道常驻对账门**，把「计划内实现片合完 ≠ 该实现的都实现了」这一系统盲区变成**可复算、
可证伪的红检**；`docs/移植计划重审-20261015.md` §1 的三张红名单（R0 内容 / R1 入站派发 / R2 出站编码器）
即本门第一批输出。

## 文件

| 文件 | 作用 |
| --- | --- |
| `recon_gates.py` | 门本体（`content` / `dispatch` / `selftest` / `mutate` 四子命令，纯标准库，Python 3.8+）。 |
| `cards.json` | 卡登记：`known_cards`（看板上存在的卡 id）+ `assignments`（缺口 key → 卡 id）。 |

## 判红口径（缺一即不红）

- **内容门**：`oracle 有该 Load 调用 ∧ 部署有该内容文件 ∧ Rust 无实现 ∧ 无卡` ⇒ **RED**。
- **派发门**：`oracle 有该 case/帧 ∧ Rust 无派发臂/编码器 ∧ 无卡` ⇒ **RED**。

「无卡」= `cards.json` 的 `assignments` 未给该缺口指派一个存在于 `known_cards` 的卡 id。
「Rust 实现」= 在 Rust 语料（去注释/字符串字面量后）出现该标识符（派发）或文件名字面量（内容）。
空部署文件（0B/空目录）按 `n/a-empty` 处理——oracle 的 `File.Exists` 门后读 0 行，与「不加载」行为无差
（对齐 §1 备注：`MapEvent.txt`/`DisableMoveMap.txt` 空文件本部署无差）。

### 部署前置（fail-closed）

`--envir-dir`/`--notice-dir` **不存在 / 不是目录 / 不可读** ⇒ 内容门（与 `mutate`）判 **`PREREQ_RED`**：
印 `PREREQ_MISSING` + `RESULT: PREREQ_RED` 且退出码 `2`——**判不了缺口就拒跑**，绝不因部署面缺失而
静默报「无缺口」假绿（对抗验证 v5 F1：旧版目录缺失 ⇒ 每件 `find_deployed→None` ⇒ 全表 `n/a-deployed`
⇒ 假绿退出 0）。诊断需要看表时用 `--allow-missing-deploy` **显式**降级为 `WARN` 放行（仅诊断，缺口结论不可作数）。

## 用法

```sh
cd mir2-rs
python tests/parity/coverage/recon_gates.py content            # 内容加载对账门
python tests/parity/coverage/recon_gates.py dispatch --show-rm # 协议派发对账门（附 RM→SM 映射）
python tests/parity/coverage/recon_gates.py selftest           # 夹具自测（含「构造缺口必红」牙）
python tests/parity/coverage/recon_gates.py mutate             # 变异实测（真实输入构造缺陷必红）
```

- 退出码：`0` = 无 RED（缺口全部「已实现或已卡」）；`1` = 存在 RED；`2` = 用法/环境错误**或前置缺失
  `PREREQ_RED`**（部署目录不存在/不可读/不是目录）。`--allow-missing-deploy` = 诊断用逃生开关（降级为 WARN）。
- `--repo <OpenMir2 根>`（默认从脚本位置推）、`--envir-dir`/`--notice-dir`（默认 `E:/MirServer/M2GameSvr/{Envir,Notice}`）、
  `--cards <json>`、`--json <out>`。

## 数据来源与口径

| 面 | 来源 |
| --- | --- |
| oracle 内容 | `src/GameSrv/GameApp.cs` 全部 `Load*` 调用点（脚本解析，非硬编码行号）+ `RobotManage.cs:25`（显式声明行）。 |
| oracle 派发 | `src/M2Server/Player/PlayObject.Message.cs` 的 `case Messages.CM_*`（入站）与文件内 `Messages.SM_*` 引用（出站）。 |
| 部署面 | `E:/MirServer/M2GameSvr/{Envir,Notice}` 文件系统（大小写不敏感，Windows 语义）。 |
| Rust 面 | 工作区全部 `mir2-rs/{crates,bins}/*/src`（派发门排除 `crates/protocol/src`——那里只有常量定义与通用编解码，非发射点）。 |
| 卡 | `cards.json`（可 `walgit collab ls` 刷新 `known_cards`）。 |

`CONTENT_MAP` 把 oracle 方法名映射到内容件（kind/envir·notice·dir·db·other + 源锚），随 oracle 漂移时同步更新。

## 变异实测（`mutate`，三条必红）

1. **M1 注入缺口**：临时 oracle 加一条合成 `CM_ZZMUT_GAP` ⇒ 派发门新增 1 条 RED。
2. **M2 撤卡**：删掉 `outbound:SM_DISAPPEAR` 的卡指派 ⇒ 该项 `tracked → RED`。
3. **M3 读侧方向**：空掉 Rust 语料 ⇒ 真仓库里 `implemented` 的 `GuardList.txt` 必转 RED
   （证明门真读 Rust 代码，而非只看 oracle/部署面——针对 `docs/移植计划重审-20261015.md` §2 的「RED-1 读错对象」教训）。

## 接入本地门禁

`.walgit/ci.toml` 或合并前 checklist 追加（**report-only 起步**，RED 清零后翻 gate）：

```sh
python mir2-rs/tests/parity/coverage/recon_gates.py selftest   # 必绿
python mir2-rs/tests/parity/coverage/recon_gates.py mutate     # 必绿
python mir2-rs/tests/parity/coverage/recon_gates.py content    # 出表；RED 见快照
python mir2-rs/tests/parity/coverage/recon_gates.py dispatch   # 出表；RED 见快照
```

## 维护约定

- 缺口**要么实现要么立卡**：实现后门自动转 `implemented`；立卡后在 `cards.json` 加一行
  `assignments["<kind>:<name>"] = "<card-id>"`（`<card-id>` 须在 `known_cards` 里）。
- 数值随实现推进**递减**：`tracked`/`RED` 计数是进度表（`docs/移植计划重审-20261015.md` §4-6）。
- oracle 漂移（新增 `Load*` 调用 / 新 `case`）⇒ 内容门会以 `unmapped` 行暴露未归类项；派发门自动纳入。
