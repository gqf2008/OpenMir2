# ident53 布局对拍分诊与 alt pin 修复（2026-10-28）

卡：`openmir2-ident53-layout-parity`；**R，`window_required=true`**。

## 结论

run5 n17 的 `SM_HEALTHSPELLCHANGED(53)` body `oracle=0B / Rust=16B` 是**对拍 profile 丢 D-9
布局 pin**，不是生产默认错，也不是新的客户端布局偏差：

1. 冻结 Delphi 客户端 `HIGHHP=True`，53 分支从 body 解 16B `TCharDesc`；
2. oracle C# 只发 header，不发尾；这是 D-9 已裁决的「oracle 布局与冻结客户端不可同时满足」；
3. 生产默认必须保留客户端 16B（客户端冻结），跨侧对拍必须显式
   `MIR2_CLIENT_ABILITY_LAYOUT=0` 取 oracle 0B；
4. 内置 `alt` 此前既没有该 env，也没有 `-PinChecks`，所以 run5 Rust 用了默认 16B。修法是给
   `alt`（及其派生 profile）补 pin 与起栈自证，**不改 `src/**`、不 ignore/掩 body**。

本轮只改 `tools/conformance/card_window.py`，并新增文档。最终仍需当前 master 真机复跑
n17 验 53 body；R 卡不因离线修复 closed。

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
- `.../Source/MirClient/ClMain.pas:10220-10232`：`SM_HEALTHSPELLCHANGED` 在 `HIGHHP` 下
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

## 3. A3：修复 = alt profile 显式 pin + 可观测自检

`tools/conformance/card_window.py` 的 `BUILTIN_PROFILES["alt"]`：

- Rust `sides.rust.env.MIR2_CLIENT_ABILITY_LAYOUT = "0"`；
- Rust stack 改为
  `with_wmi_pin_checks(wmi_stack_argv(...), ["MIR2_CLIENT_ABILITY_LAYOUT=0"], RUST_STACK_PIN_LOG)`；
- 包装起栈后会读 `game-svr.out.log` 的
  `CLIENT_ABILITY_LAYOUT layout=oracle-40（MIR2_CLIENT_ABILITY_LAYOUT=0）` 自证；pin 丢失/未生效
  ⇒ 非零退出。这样不会静默跑回 client-58。
- `window-magic` 本已有同 pin，`with_wmi_pin_checks` 幂等；`window-testdb` 从 alt 派生，
  自动继承 pin 与自证。

新增 `card_window --selftest` 15e 永久断言：alt env=`0`、stack 含对应 `-PinChecks` 和
`-PinLog`、`window-testdb` 继承；把 alt 副本的 env 去掉后该谓词必须为假。实测
`card_window selftest 469/469 GREEN`。另将该断言的外部变异（删除 alt env）作为本卡负例。

未改生产默认，也未改 `rules.json`：53 仍按原规则 fail-closed 比较，本轮不新增掩码。

## 4. A4：真机与收口

当前 master 的 n17 `upgrade_retrieve` 复跑时，先查起栈日志自证行确为 `oracle-40`；随后
53 两侧应均为 `body_len=0`（oracle header-only / Rust pin=0），其余 53 head 字段仍逐字节比。
若仍出现 16B，先判 pin 是否真的到达（看 `-PinChecks` 自证与 `game-svr.out.log`），不得用
`ignore` 或 body 掩码处理。

本卡为 R：真机项按 owner 新规合并后追加到 `openmir2-real-machine-verification-master`；
未跑真机前保持 in-progress 语义，不把离线配置修复当窗口证据。
