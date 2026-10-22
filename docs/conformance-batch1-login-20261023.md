# 对拍批次 1：login_only 双栈真机 run（2026-10-23 05:03–05:05）

- 卡：`openmir2-dualrun-window-burndown`（owner `openmir2-worker-24`）
- 窗口：05:03:35 起 C# 栈 → 05:04:10 跑 probe → 04:04 停 → 05:04:31 起 Rust 栈 → 05:04:36 跑 probe → 05:05 停；两侧端口全释放后才动手，跑完 `Get-Process` 残留 = 0。
- 方法：同端口串行（`E:/tmp/conformance/` 下 `start-csharp3.ps1` / `start-rust.ps1`），同一账号 `winplay0`、同一服务器名 `热血传奇`、同一场景 `tools/probe/scenarios/login_only.json`。

## 1. 命令与产物

```sh
# C# oracle 侧
python -I -B tools/probe/mir2_probe.py --login-gate 127.0.0.1:17000 \
  --account winplay0 --password winplay0 --server-name 热血传奇 --char-name winplay0 \
  --scenario tools/probe/scenarios/login_only.json \
  --out E:/tmp/conformance/runs/w24-csharp-login --timeout-ms 20000
# → exit=0 frames=29

# Rust 侧（首跑同样显式带 --game-gate 17600）
python -I -B tools/probe/mir2_probe.py --login-gate 127.0.0.1:17000 --game-gate 127.0.0.1:17600 \
  --account winplay0 --password winplay0 --server-name 热血传奇 --char-name winplay0 \
  --scenario tools/probe/scenarios/login_only.json \
  --out E:/tmp/conformance/runs/w24-rust-login --timeout-ms 20000
# → exit=0 frames=30

python -I -B tools/conformance/compare_runs.py \
  --oracle E:/tmp/conformance/runs/w24-csharp-login \
  --rust   E:/tmp/conformance/runs/w24-rust-login \
  --out    E:/tmp/conformance/runs/w24-login-report.json
```

| 产物 | sha256 前缀 |
|---|---|
| `runs/w24-csharp-login/frames.ndjson` | `ad2b357be761219bd8fd0c49…` |
| `runs/w24-rust-login/frames.ndjson` | `f2f3f5bcae5ad9b5221aa69d…` |

## 2. 帧序列（ident 顺序）

```
C#  (29): 529 530 520 525 658 51 50 41 213 11029 42 766 54 5008 1106 52 752 811 46 10 41 654 657 657 42 100 100 100 100
Rust(30): 529 530 520 525 658 51 50 41 213 11029 42 766 54 5008 1106 52 752 811 46 [14] 10 41 654 657 657 42 100 100 100 100
```

## 3. 差异（按证据强度排序）

1. **Rust 多一条 `SM_HIT(14)`**（idx 19，紧跟在 46 之后、10 之前）。去掉这条后**两侧 ident 序列完全相同（29 = 29）**。
   对应卡 `openmir2-npc-hit-frames`（「固定 NPC 的 SM_HIT(14) 诊断——Rust 抓到、C# 未见」）：本批次用无头对拍复现了该现象。
2. **两条系统公告顺序颠倒**（ident 100）：
   | 位次 | C# | Rust |
   |---|---|---|
   | 26 | 「禁止使用任何外挂、加速、辅助、脱机…」 | 「唯一客服QQ：79311185 唯一担保QQ：79311185」 |
   | 27 | 「唯一客服QQ…」 | 「禁止使用任何外挂…」 |
   ⇒ 内容两边都在，**发送次序相反**（时序差，R 级口径）。
3. **`SM_ABILITY(52)` body 长度 54 vs 78**（C# 54 / Rust 78）：与首跑一致的布局对 (54,78)，
   规则表白名单仍是 `{40,58}` ⇒ 属规则缺口（`conformance-rules` 车道），不等于行为差异。

## 4. 比较器判定

- `compare_runs.py` 原始输出：`DIFFS: 9` / `VERDICT: UNDECIDABLE (exit 2)`，
  UNDECIDABLE 原因是 **ident 14 在规则表里没有条目（UNCLASSIFIED）**——规则缺口，不是"通过"。
- 9 条 diff 中 7 条是**位置错位**（idx 19 起整体后移一位），根因即差异 1；另 2 条是错位后错比的 100。

## 5. 后续

- `rules.json` 需补 ident 14（以及首跑已知的 100/520/525/529/530/654/657、52 的 (54,78)）——归 `conformance-rules` 车道，本卡只提供证据。
- 公告顺序差异需单独立卡或以 `comment` 挂到相关卡（R 级：时序）。
- 下一批：同窗口跑 `gm_time` / `gm_batch`（无需 `--var`），再按首次 run 的 `recog` 填 `--var` 跑 NPC/商人/背包族。

## 6. 追加（05:06–05:09 第二次窗口）：`gm_time` 两侧都收不到回包

同一窗口内改跑 `gm_time`（`CM_SAY(3030)` 发 `@时间`，等 `SM_SYSMESSAGE(100)`）：

| 侧 | 尝试 | 帧数 | 结果 |
|---|---|---:|---|
| C# | 默认（softclose） | 30 | `steps[1]: timeout waiting for ident(s) [100] after 3000ms` |
| C# | 前置 `sleep 1500` | 30 | 同上 |
| C# | `--finish close` | 30 | 同上 |
| Rust | `--finish close` | 31 | 同上 |
| Rust | 前置 `sleep 1500` + `--finish close` | 32 | 同上 |

- `CM_SAY` 确实发出（`run.json.chain.sends` 里 t≈529ms 有 ident=3030），两栈都**没有任何 100 回包**。
- C# GameSrv 日志在连跑之间反复出现 `角色数据保存中,稍后重新读取. Account:winplay0`，
  疑与 `CM_SOFTCLOSE` 的 BoReconnection 语义（玩家留在世界里等重连）叠加有关；改用 `--finish close` 未改变结果。
- 结论：**gm 族当前不可作对拍证据**，先要解决「进世界后 `CM_SAY` 何时/怎样才被处理」；
  首批应缩到 `login_only`（已跑通）与点击/背包族，gm 族另行排查。

## 7. 追加（05:09–05:12 第三次窗口）：点击族跑通，`SM_HIT(14)` 不可复现、`100` 顺序差可复现

**问题与解法**：C# 的 ActorId 每会话不同（本批 2123442500，前一会话见过别的值），跨 run 复用必然点空。
驱动器新增 `click_npc: "auto:last_turn"`：取进世界后**首个非自己的 `SM_TURN(10)`** 的 recog，
没有就 fail-closed。据此新增 `tools/probe/scenarios/npc_click_auto.json`（无需 `--var`）。

```sh
# C#
python -I -B tools/probe/mir2_probe.py --login-gate 127.0.0.1:17000 --account winplay0 --password winplay0 \
  --server-name 热血传奇 --char-name winplay0 --scenario tools/probe/scenarios/npc_click_auto.json \
  --out E:/tmp/conformance/runs/w24-csharp-npc_auto --timeout-ms 20000 --finish close      # exit=0 frames=30
# Rust（同参数 + --game-gate 127.0.0.1:17600）
python -I -B tools/probe/mir2_probe.py ... --out E:/tmp/conformance/runs/w24-rust-npc_auto    # exit=0 frames=30
```

| 项 | C# | Rust |
|---|---|---|
| 帧数 | 30 | 30 |
| `SM_MERCHANTSAY(643)` | 有 | 有 |
| `SM_HIT(14)` | 无 | **无**（本批两侧都没有） |

`compare_runs.py`：`frames 30/30`、**DIFFS 2**——
`idx=26/27 ident=100 body_len 72↔55`，即 §3 第 2 条的**系统公告顺序颠倒**再次出现（内容对调、长度互换）。
其余 `whitelisted: 46 条`（recog/t_ms 掩码、52 布局）。

**重要修正**：`SM_HIT(14)` 只出现在 05:04 的 `login_only` run（Rust 单侧多一条），
本批 05:11 的 `npc_click_auto` 两侧都没有 —— 说明它是**间歇性**的（与首卡「Rust 抓到、C# 未见」的描述一致），
不能当成每次登录必现的稳定差异；要定位需固定 RNG 种子（`MIR2_RNG_SEED`）后重复采样。
