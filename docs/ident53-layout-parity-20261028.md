# ident53 布局对拍分诊与专用 layout profile（2026-10-28，2026-10-29 返工）

卡：`openmir2-ident53-layout-parity`；**R，`window_required=true`**。

## 结论

run5 n17 的 `SM_HEALTHSPELLCHANGED(53)` body `oracle=0B / Rust=16B` 是**对拍 profile 丢 D-9
布局 pin**，不是生产默认错，也不是新的客户端布局偏差：

1. 冻结 Delphi 客户端 `HIGHHP=True`，53 分支从 body 解 16B `TCharDesc`；
2. oracle C# 只发 header，不发尾；这是 D-9 已裁决的「oracle 布局与冻结客户端不可同时满足」；
3. 生产默认必须保留客户端 16B（客户端冻结），跨侧对拍必须显式
   `MIR2_CLIENT_ABILITY_LAYOUT=0` 取 oracle 0B；
4. 内置 `alt` 此前既没有该 env，也没有 `-PinChecks`，所以 run5 Rust 用了默认 16B。修法是
   **保持 alt 清洁**，新增仅用于 53 对拍的 `window-layout` 派生 profile，并把 n17 映射过去；
   **不改 `src/**`、不 ignore/掩 body**。

2026-10-29 返工：首版把 pin 注入 `alt` 后被 run8 preflight `pin_static` 正确判红（alt 是
窗口基线，不能被窗口 pin 污染）。本版撤销 alt 注入，pin 只存在 `window-layout`；
`window-testdb` 仍继承干净的 alt，`window-magic` 保持原有 pin。最终仍需当前 master 真机
复跑 n17 验 53 body；R 卡不因离线修复 closed。

## 1. A1：run5 当时是否 pin

- run5 `card-window.json` 的 Rust stack 命令是：
  `start-stack-wmi.ps1 -Script start-rust.ps1 -ReadyPorts 15000,17000,17100,17600`
  —— **没有** `-PinChecks` / `-PinLog`。
- 同批 `card-window.log:722-726` 的包装自证是
  `env forwarding: 116 vars (MIR2_*: ; ...)`：所有 `MIR2_*` 均为空，未转发布局 pin。
- 以 run5 的 `git_head=9fdaddac9` 回看该版 `card_window.py:461-523`，内置 `alt` 的
  `sides.rust.env` 为 `None`，stack 也是不带 pin 参数的 `wmi_stack_argv(...)`。
  ⇒ **profile 漏 env**，不是宿主没转发已存在的 pin。

对应产物：`E:/tmp/window/run5/upgrade_retrieve/report.json` 中唯一 53 diff 为
`frame_index=60, kind=body_len, oracle=0, rust=16`；两侧帧内容其余 header 字段相同。

## 2. A2：冻结客户端为何要求 16B，oracle 为何是 0B

冻结客户端（只读）：

- `E:/Users/gxh/Documents/GitHub/MirClient/Source/Common/Grobal2.pas:39`：`HIGHHP = True`；
- `.../Source/MirClient/ClMain.pas:10220-10236`：`SM_HEALTHSPELLCHANGED` 在 `HIGHHP` 下
  `DecodeBuffer(body, @desc, SizeOf(TCharDesc))`，再取
  `HP=desc.Feature / MP=desc.Status / MaxHP=desc.StatusEx`；
- `.../Source/Common/Grobal2.pas:3515-3519`：`TCharDesc` 是三个 `Integer`，合计 12B，
  EDCode 后线缆 16B。

oracle：`src/M2Server/Player/PlayObject.Message.cs:1736-1739`
`MakeMessage(53, …HP,MP,MaxHP)` + `SendSocket(ClientMsg)`，无 body。

Rust：`mir2-rs/bins/game-svr/src/smframe.rs:262-345`；默认（env 未设）走
`sm_healthspellchanged_client` 16B，`MIR2_CLIENT_ABILITY_LAYOUT=0` 走 oracle header-only。
因此 16B 是**冻结客户端的生产正确布局**，0B 是对拍时 oracle 腿的布局；既不是 Rust 生产 bug，
也不需要新增 ignore/mask。D-9 的既定口径见 `mir2-rs/tests/parity/whitelist.md`。

## 3. A3：修复 = 独立 `window-layout` profile + 可观测自检

**alt 保持无布局 env**（run8 preflight 的 `pin_static` 明确要求
`alt.rust.env` 不含 `MIR2_CLIENT_ABILITY_LAYOUT`；它是所有窗口场景的干净基线）。
新增内置派生 profile `window-layout`（`card_window.py` 的
`_derived_layout_profile`）：

- `window-layout` 的 Rust `sides.rust.env.MIR2_CLIENT_ABILITY_LAYOUT = "0"`；
- Rust stack 接
  `with_wmi_pin_checks(wmi_stack_argv(...), ["MIR2_CLIENT_ABILITY_LAYOUT=0"], RUST_STACK_PIN_LOG)`；
- 包装起栈后读 `game-svr.out.log` 的
  `CLIENT_ABILITY_LAYOUT layout=oracle-40（MIR2_CLIENT_ABILITY_LAYOUT=0）` 自证；pin 丢失/未生效
  ⇒ 非零退出；
- `window-testdb` 仍从 alt 深拷贝（无 pin）；`window-magic` 继续在自身 profile 加 pin，
  三者互不污染；
- `tools/window/run3-scenarios.json` 的 n17 `upgrade_retrieve` 从 `alt` 改指
  `window-layout`；`run_batch.sh` 的 public preflight 也纳入 `window-layout`，避免新 profile
  未 dry-run。

新增 `card_window --selftest` 15e 永久断言：alt rust env 无 pin、stack 无 `-PinChecks`；
`window-layout` env=`0` 且 `-PinChecks/-PinLog` 可观测；`window-testdb` 无 pin；
把 alt 污染成含 pin、或删掉 `window-layout` env 的副本，都必须不满足。实测
`card_window selftest 469/469 GREEN`。外部负例“给 alt 注入 pin”由 15e 与 preflight
`pin_static` 双重点名。

未改生产默认，也未改 `rules.json`：53 仍按原规则 fail-closed 比较，本轮不新增掩码。

## 4. A4：真机与收口

当前 master 的 n17 `upgrade_retrieve`（`profile=window-layout`）复跑时，先查起栈日志自证行
确为 `oracle-40`；随后 53 两侧应均为 `body_len=0`（oracle header-only / Rust pin=0），
其余 53 head 字段仍逐字节比。若仍出现 16B，先判 pin 是否真的到达（看 `-PinChecks` 自证与
`game-svr.out.log`），不得用 `ignore` 或 body 掩码处理。

本卡为 R：真机项按 owner 新规合并后追加到 `openmir2-real-machine-verification-master`；
未跑真机前保持 in-progress 语义，不把离线配置修复当窗口证据。
