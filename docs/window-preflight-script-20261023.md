# 开窗前置检查一条命令：`tools/conformance/preflight.py`（`openmir2-window-preflight-script`）

- 卡：`openmir2-window-preflight-script`（tier=L）；分支 `card/openmir2-window-preflight-script`（base master `ed4dd05c6`）。
- 目标：把 runbook §0.5 的手工五步做成一条命令，**只读**（不起栈、不连库、端口只 connect 探测）。

## 1. 用法与输出

```sh
python -I -B tools/conformance/preflight.py --profile alt
python -I -B tools/conformance/preflight.py --profile alt --with-dry-run --with-selftest
python -I -B tools/conformance/preflight.py --selftest
```

每步一行 `[OK]/[FAIL] <step>: <细节>`，末行 `PREFLIGHT PASS`（rc=0）/ `PREFLIGHT FAIL`（rc=2）；FAIL 行内直接给修复建议。

| 步骤 | 实现（复用现有件，不另造） |
|---|---|
| `docs` | runbook 用 §0.5 第 0 步的 **DOC-KEYS**：含 `probe_timeout_ms`、不含 profile 形态的 `"timeout_ms":`；scenarios/README 只要求提到 `probe_timeout_ms`（它有合法的**步骤级** `timeout_ms` DSL，不能套同名负断言） |
| `binary` | 直接调 `tools/m2-acceptance/start_mir2_server.ps1 -WhatIf` 并解析 `SERVER_EXE / SERVER_EXE_SHA256 / SERVER_EXE_BUILD` 三行；要求 `Release`，且打印 sha 与本地重算一致（**不另写选路逻辑**）。`--allow-debug` 是显式降级开关（默认硬失败） |
| `ports` | 从 profile 的 `sides[*].ports` 取并去重，逐个 `connect` 探测（**不写死端口表**）；任一被占 ⇒ 点名 |
| `profile` | `probe_timeout_ms` 为正整数；**开了 `require_pid_binaries` 的侧必须有 `pid_file`**；至少一侧开启对账（否则整段失效） |
| 可选 | `--with-dry-run` / `--with-selftest` 才委托 `card_window.py`，默认不跑（不让 preflight 本身变慢） |

## 2. 自测（`--selftest`，11 checks）

用桩覆盖五个失败分支，**断言先收集、最后统一判定**（纪律 15：不存在「先 return 1 再跑检查」）：

① docs 旧键/缺键；② 二进制 `Debug-Fallback` 与「打印 sha ≠ 本地重算」；③ 端口去重与「被占点名」；
④ 缺 `probe_timeout_ms`；⑤ 缺 profile 时 `run_steps` 至少一步红（fail-closed）。

## 3. 牙（真实 rc，实跑记录）

| 牙 | 做法 | 真实结果 |
|---|---|---|
| ① profile 缺 `probe_timeout_ms` | 用 `/e/tmp/pf-prof-nokey.json`（同 profile 去掉该键）跑 | **rc=2**，`[FAIL] profile: 缺 probe_timeout_ms ⇒ 按 runbook §0.5 第 3 步补 profile 键` |
| ② 端口被占 | 本地 `socket.listen(15999)` + `/e/tmp/pf-prof-port.json`（ports=[15999]） | **rc=2**，`[FAIL] ports: 端口被占：[15999] ⇒ 上一轮栈没停净…` |
| ③ debug 构建 | 在本树直接跑 `--profile alt`（本树无 release） | **rc=2**，`[FAIL] binary: SERVER_EXE_BUILD=Debug-Fallback ⇒ 在这棵树里 cargo build --release -p mir2-server` |

> 牙①②的桩 profile 都放在 `/e/tmp`（仓库外），未改仓库内 profile；牙③是**真实环境**的红（不是构造）。

## 4. 门禁

```
preflight.py --selftest          OK（11 checks）
card_window.py --selftest        GREEN 197/197（本卡未改 card_window）
conflict_marker_gate             OK（0 / 7412 tracked files）
t_reservation_gate.py check      GREEN（rows=246 whitelist=246）
```

## 5. 文档

`docs/window-runbook-20261023.md` §0.5 改为「**一条命令 + 五步含义表**」，原手工步骤保留为
「退路：手工步骤（命令不可用时照旧做）」。

## 6. 写集与残留

- 写集：`tools/conformance/preflight.py`（新增）、`docs/`。**未改** `card_window.py` / `rules.json` / `src/**`。
- 残留物：`E:/tmp/pf-prof-nokey.json`、`E:/tmp/pf-prof-port.json`（牙桩）、`E:/tmp/pf-t1.log`、`E:/tmp/pf-t2.log`、`E:/tmp/pf-real.log`。
