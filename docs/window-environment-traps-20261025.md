# 窗口环境陷阱（可粘进 runbook；卡 `openmir2-dir-persistence-writeback` §8）

> 一条纪律：**先确认环境状态，再解释差异**。以下每条都给「跑这条命令 / 看这个值 / 怎么判 / 不要做什么」。
> 共同形态：**工具没报错，只是没检查/没保留**——差异看起来像行为不符，其实是环境状态。

## T1 存档在途 ⇒ 服务端拒动作

- **现象**：C# 侧发了 `CM_WALK`，但帧里**没有 `SM_WALK`**，只有若干 `SM_TURN(10)` + `SYSMESSAGE(100)`；
  probe 报 `steps[n]: timeout waiting for ident(s) [11,13,10,100]`。
- **可判处置**：

  ```bash
  grep -c '角色数据保存中' <side>/game-svr.out.log      # 期望 0
  grep '角色数据保存中' <side>/game-svr.out.log | tail -3
  ```

- **判读规则**：起栈/登录后该行**仍出现** ⇒ `InSaveRcdList` 未清 ⇒ **等它消失再发第一条动作命令**；
  已出现且动作已发的，该轮**判不了**（UNDECIDABLE），不得记成 FAIL。
- **不要做**：不要因为「walk 没生效」就改场景/改判据；不要把 `SYSMESSAGE` 当成断言对象。

## T2 RNG 账本被裁 ⇒ 「抽数没发生」

- **现象**：账本里查不到本该发生的抽数（如 butch 的两抽）；其余帧正常。
- **可判处置**：

  ```bash
  python -c "import json;d=json.load(open('card-window.json'));print({s:d['sides'][s].get('rng_log_trim',{}).get('trimmed') for s in ('csharp','rust')})"
  ```

- **判读规则**：任一侧 `trimmed=true` ⇒ **该侧证据作废**（`trim_rng_ledger` 只保留 head 行，
  而抽数在运行后半段；实测 C# ≈ 7.2k 行/s，>≈28s 就有风险）⇒ **重跑**，**不得判 FAIL**。
- **不要做**：不要把「账本被裁」写成「行为不符」；不要用被裁后的账本当对拍输入。

## T3 检出/构建不全 ⇒ 假红

- **现象**：`oracle_vectors` 或对照工具报红，但源码/期望值看着没问题。
- **可判处置**：

  ```bash
  python -c "import json;d=json.load(open('E:/tmp/conformance/rust-logs/mir2-server.pid.json'));print([r['exe'] for r in d['roles']])"
  ls -l mir2-rs/target/release/mir2-server.exe   # 比对构建时间与当前 head
  git log --oneline -1
  ```

- **判读规则**：pid 里的 `exe` **必须**是本检出的产物、且构建时间晚于/对齐当前 head；
  `require_pid_binaries` fail-closed 是**正确行为**，不要绕过；不匹配 ⇒ 重建/重起栈后再判。
- **不要做**：不要用别的车道（如 tester-1）的旧二进制跑本线窗口；不要用「路径对」代替「构建对」。
