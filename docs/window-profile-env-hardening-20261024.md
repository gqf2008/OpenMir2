# window 内置 profile/preflight 环境加固（2026-10-24）

- 卡：`openmir2-window-profile-env-hardening`（tier=L）；执行者 openmir2-worker-75；交付 `card/window-profile-env-hardening`。
- 基线：`walgit/master` = `133c0f6a1`；worktree `E:/tmp/wt-worker-75`。
- 依据：`docs/window-run-1-20261024.md` §1 的三处本机环境阻塞 + 本机适配件
  （`E:/tmp/window/run1/profile-alt-ignore14.json`、`profile-windowmagic-ignore14.json`、
  `start-csharp-warmup.ps1`、`start-rust-warmup.ps1`、死锁复现件 `pipe_repro.py`）。
- **写集**：`tools/conformance/card_window.py`、`tools/conformance/preflight.py`、
  `tools/window/start-stack-wmi.ps1`（新增）、`docs/window-runbook-20261023.md`（§0.5 表补 bash 步）、
  本文件。**零**场景 JSON 改动、**零** `src/**`/C# 改动、**零** probe/compare 帧过滤与掩码改动、
  **未起任何服务端栈**（验证全在 dry-run/selftest/冒烟桩层）、`docs/window-run-1-20261024.md` 零触碰。

## 1. 三处阻塞 → 修法位置映射

| window-run-1 §1 | 修法 | 落点 |
|---|---|---|
| ① restore 的 `bash` 解析到 WSL stub（System32 先于 PATH） | 内置 profile 用 Git bash 绝对路径 + 加载期拒绝裸 `bash`/stub | `card_window.py`：`detect_git_bash()`/`GIT_BASH`（`CARD_WINDOW_GIT_BASH` 可覆盖）/`is_wsl_stub_path()`/`check_restore_bash()`（`load_profile` 对**所有** profile 生效）；内置 `alt`/`csharp-default` restore 全部改为 `[GIT_BASH, …]`；`preflight.py` 新增 `bash` 步（`check_bash()`：裸 bash 在 `%SystemRoot%\System32\bash.exe` 存在时必 FAIL；显式路径须存在且非 System32/Sysnative） |
| ② 栈步死锁（孙进程继承 stdout 管道，EOF 永不到来） | 栈命令改 WMI `Win32_Process.Create` 派生 | 新增 `tools/window/start-stack-wmi.ps1`（WMI 派生，父进程 WmiPrvSE 无句柄继承）；`card_window.py` `wmi_stack_argv()` 生成内置 `stack` 命令（`alt`/`csharp-default` 两侧） |
| ③ GameGate 探针占 user 会话槽吃掉 enter-world | 内置 ready_ports 去网关端口；网关就绪由栈包装「全端口监听 + 暖机」兜底，缺端口非零退出 | 内置 `alt`：csharp ready_ports `[15000,17000,17100]`（去 17200）、rust `[15000,17000,17100]`（去 17600）；`csharp-default` 两侧去 7200；网关端口进包装 `-ReadyPorts`（`15000,17000,17100,17200` / `…,17600` / `5000,7000,7100,7200`），包装等齐全后暖 45s，缺端口 `exit 1`（fail-closed） |

派生 profile `window-magic`/`window-testdb` 是 `alt` 的 deepcopy，自动继承三处加固。

## 2. 验收证据（命令 + rc）

```text
card_window.py --selftest                 GREEN 255/255（基线 240/240 + 新增 15：23a×2 / 23b / 23c×12）
preflight.py   --selftest                 OK (18 checks) rc=0（基线 13 + bash 步 5：⑤a stub命中红 / ⑤b 真Git bash绿 /
                                          ⑤c 显式stub红 / ⑤d 解析不到红 / ⑤e 解析到stub红）
preflight.py   --profile alt --with-dry-run   六步全 [OK] ⇒ PREFLIGHT PASS rc=0
                                          （docs / binary=release sha df527c383bfe… / ports 12 个全空 /
                                           profile / bash=C:/Program Files/Git/usr/bin/bash.exe / dry-run rc=0）
18 场景 dry-run（17 场景 --profile alt + spell_cast --profile window-magic）   18×rc=0
                                          spell_cast 在 alt 下 rc=2（缺 magic 前置，设计内，与
                                          docs/window-scenario-dryrun-sweep-20261024.md 基线逐格一致）
window-testdb 抽样（upgrade_retrieve/bag_equip/npc_script_chain dry-run）      3×rc=0
WMI 包装冒烟（临时端口监听 + 6s 自退孙进程桩，未碰任何游戏端口/服务端栈）：
  正向 rc=0 elapsed 4.4s（孙进程睡到 6s ⇒ wrapper 退出即 EOF，无管道死锁）
  负向（缺端口 59999）rc=1 "ready ports missing: 59999"（fail-closed）
```

## 3. 门禁

```text
stale_pointer_gate                    OK rc=0（151 files / 36382 lines；ref=133c0f6a13）
dynamic_count_stamp_gate              rc=1 —— **master 上同样 rc=1、失败集逐字节一致**
                                      （docs/window-run-1-20261024.md:53/:70 缺快照戳；
                                      master 复跑对照 + failure-set diff = 0 ⇒ 本卡零增量；
                                      该文件是 window-run-1 线的写集，本卡红线不碰，残留归该线）
conflict_marker_gate                  OK（0 markers across 7476 tracked files）
git diff --check                      rc=0
git merge-tree --write-tree walgit/master HEAD    rc=0（无冲突，tree=d58d18f22…）
新文件 EOL/BOM                        tools/.gitattributes `*.ps1 text eol=lf` 覆盖；blob LF、无 BOM
```

## 4. 残留物清单

| 路径 | 是什么 | 处置 |
|---|---|---|
| `E:/tmp/window/w75-smoke/`（inner.ps1 / smoke.py） | WMI 包装冒烟桩（临时端口 + 自退孙进程） | 冒烟已完成使命，可清；留作 §2 冒烟证据的复跑件 |
| `E:/tmp/wt-worker-75/mir2-rs/target/release/mir2-server.exe` | 本 worktree 的 release 构建（preflight binary 步要求） | worktree 产物，不进 git；sha256=df527c383bfe… |
| 默认栈 | 未触碰（十端口状态与开工前一致；mysqld 全程未动） | — |
