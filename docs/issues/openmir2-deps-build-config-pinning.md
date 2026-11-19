# openmir2-deps-build-config-pinning

> 状态：**已登记，待实施（opc-07 2026-11-19 裁决：排在 `openmir2-aoi-incremental-default-on` 之后）**。

## 现象（今天的实例）

`tools/deploy/deps_manifest.py` 生成 manifest 时用 `detect_output_dir()` **按 mtime 取最新的
`bin/<config>`**（`Release` 优先遍历、但最终由 mtime 决定），把结果写进 `output_dirs`；
`check` 再**重新探测一遍**并与 manifest 记录的目录比对，不等即 `BUILD_OUTPUT_MISMATCH`。

于是：

- `m1-stack/deps/*.deps.json` 两份最初是在 **Debug** 口径下生成的；
- 部署走 Release（`start-m1-stack.ps1` 从 `src\DBSrv\bin\Release` 刷新影子），只要主仓库出现
  一次 Release 构建（今天 M7 的门禁构建就是），探测结果翻成 Release；
- `check` 立刻判红：

```
DEPS_RED BUILD_OUTPUT_MISMATCH ...DBSrv.csproj manifest=...\bin\Debug now=...\bin\Release
DEPS_RED_TOTAL 1                                       (rc=1)
```

而 `start-m1-stack.ps1` 的 3c 段 fail-closed ⇒ **整栈起不来**，与被检内容是否真有问题无关。

## 要求（opc-07）

1. `check` 增加**显式** `--build-config <Debug|Release>`（或从 manifest 读 `build_config` 并**强制**
   用它解析输出目录），不再让"最新输出目录"这种隐式行为成为判据；
2. **含糊时 fail-closed**：`--build-config` 与 manifest 不一致、或两者都缺/无法确定 ⇒ 红（rc≠0），
   不许静默挑一个；
3. 生成侧同步：`generate --build-config` 写死口径；manifest 里 `build_config` 与 `output_dirs`
   必须自洽；
4. 卡面写清今天的实例（Debug 构建落地 ⇒ 旧 manifest 立刻 `BUILD_OUTPUT_MISMATCH`、整栈起不来）。

## 边界

- 只动 `tools/deploy/deps_manifest.py`（+ 自测/牙）与 `start-m1-stack.ps1` 的传参（botfleet 侧）；
- 不改 manifest 的包字段口径、不重下包、不动戳门禁。

## 验收（实施时）

- 牙：① Debug 目录 mtime 更新但传 `--build-config Release` ⇒ 判据不变、不红；
  ② `--build-config Debug` 对 Release 口径 manifest ⇒ 红；
  ③ 缺参且两目录并存 ⇒ fail-closed；
- `deps_manifest selftest` 扩充并进工具门（TOTAL 变化按口径记）；
- 真实两份 manifest（DBSrv 46 包 / LoginSrv 48 包）复跑 `DEPS_GREEN`。

## 背景实例（M7，2026-11-19）

- 已在 M7 上用 Release 口径重生成两份 manifest 并双双 `DEPS_GREEN`（DBSrv 46/1030、LoginSrv 48/1063）；
- 该处置只是把口径拨正，**脆弱点仍在**：下一次 Debug 构建又会翻回去。
