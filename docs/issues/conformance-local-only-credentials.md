# conformance-local-only-credentials

> 状态：**说明件（2026-11-19，opc-07 要求）**。只登记口径，不改任何历史文档里的既有字面值。

## 口径

`CONFORMANCE_PASSWORD`（对拍/窗口脚本使用的测试口令）属于**本地私有测试栈**的凭据：
栈由 `E:/tmp/conformance` 拉起、只监听**回环**（`127.0.0.1` 的 15xxx/17xxx 端口），
配套账号只存在于测试库。

**红灯提示**：**若该私有栈将来对外暴露（改监听地址、端口转发、迁到公网主机），
必须先轮换该口令，并清理仓库文档里出现的字面值。** 在此之前不得把它当作可公开的知识。

## 背景（2026-11-19 凭据盘点）

- `docs/window-run-1-20261024.md` 出现的口令是**测试账号同名口令**，来源
  `docs/conformance-batch1-login-20261023.md` 明写为本地私有栈同账号同口令串跑；
- `mir2-rs/tests/parity/whitelist.md` 中的 `CONFORMANCE_PASSWORD` 是 `<口令>` /
  `PASTE-PASSWORD-HERE` 占位，非真值；
- `mir2-mcp` 的 HTTP e2e 测试常量是**假值**（测试自拉起的回环服务 + `definitely-wrong` 负例）。

以上均判定为**夹具/占位**，不轮换、不改写历史文档。
