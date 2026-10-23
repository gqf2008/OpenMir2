# `HostNpc` 覆写 `envir_root`：文件列表类脚本条件/动作接通（T-220）

- 卡：`openmir2-script-envir-root-host`（tier=R）；分支 `card/openmir2-script-envir-root-host`，base `d3393f182`
- 作者：`openmir2-worker-24`；worktree `OpenMir2-wt-cronclaim`

## 问题

`HostNpc`（`bins/game-svr/src/npc.rs`）没有 `ScriptNpc::envir_root` 覆写 ⇒ 缺省 `None` ⇒
`check_string_list` / `envir_file_path` 全部按「文件不存在」处理：`CHECKNAMELIST`(15)、
`CHECKACCOUNTLIST`(238) 恒 false，`ADDNAMELIST`(9)/`DELNAMELIST`(2)/`DELACCOUNTLIST`(238)/
`ADDUSERDATE`(2)/`DELUSERDATE`(2) 不写盘。

## 实现

- `HostNpc` 新增 `envir_root: Option<String>`；
- `run_goto_label` 增加注入形参：`NpcHost::goto_label` 传 `self.envir_root`（空 `PathBuf` ⇒ `None`），
  **`MerchantScriptBook::goto_label` 传 `None`**（商人表未持 Envir 根，保持接线前行为，另卡登记）；
- `impl ScriptNpc for HostNpc` 新增 `fn envir_root`；
- `check_string_list` / `envir_file_path` 的语义（`Trim()` + `OrdinalIgnoreCase`、`..` 前缀剥离、
  逐段 `PathBuf::push`）保持不变。

## 用例与牙

`host_npc_envir_root_enables_check_name_list`：临时 `Envir/TestList.txt` 写 `"  Hero  "` ⇒
`CHECKNAMELIST` 对 `HERO` 命中（Trim + 忽略大小写），`Nobody` 走 `#ELSEACT`，**未注入根 ⇒ 恒假**。

牙：删 `HostNpc::envir_root` 覆写 ⇒ 首段实测 `left: ["miss"] right: ["hit"]`（红）；还原后
`npc.rs` sha256 `afd23cd9…` 与编辑前一致。

## 门禁

fmt / clippy `-D warnings` / `cargo test --workspace` / msgcodegen / anchor 508 / registry /
T 预约表（见卡面结果）。

## 残余 / 窗口项（R，未跑）

- 商人路 `envir_root` 未注入（`MerchantScriptBook` 无字段）⇒ 商人脚本的同源条件仍恒假（登记，另卡）。
- `CHECKACCOUNTLIST` 正例依赖子卡 `openmir2-script-user-account-host`（`user_account` 缺省空串）；
  本卡只负责文件/路径面。
- 真机窗口：抽样 1 个部署 `CHECKNAMELIST` 场景两侧一致；`ADDNAMELIST`/`DELNAMELIST` 落盘逐字节；
  `DELACCOUNTLIST` 名册核对。窗口未跑前不 closed。
- 未起栈、未占端口、未连库。

## 残留物

- `E:/tmp/w24-er.sha`（牙基线）。
