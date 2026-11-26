# DEV-29 — `mir2-mcp` 的 `conf_lock_e2e` 并发用例在重负载下红（负载 flake，非产品缺陷）

> 登记日：2026-11-26。来源：`card/clippy-dbsrv-fix` 的 `--workspace` 门首跑。
> 登记号按 opc-27 裁决：**缺陷 #29**。**不是产品缺陷，不按产品缺陷修。**

## 现象

`cargo test --workspace` 在机器上有 14 个 `cargo/rustc` 并发时，`mir2-mcp --test
conf_lock_e2e` 的 `concurrent_stale_takeover_leaves_only_target_file` 失败：

```
panicked at bins\mir2-mcp\tests\conf_lock_e2e.rs:134
```

整轮随后被 Claude Code 的内存收割器中止（低内存保护），不是测试进程自身退出。

## 复跑与判据

- **单二进制复跑**（机器 9.2 GB free、cargo/rustc=3）：
  `cargo test -p mir2-mcp --test conf_lock_e2e` → **rc=0，全部 ok**（并发文件锁一族用例全绿）。
- ⇒ 判为**负载敏感的文件锁争用时序 flake**；与 `db-srv` 的 clippy 修复无关（该 crate 本笔未改动）。

## 处置

- 不修改产品代码、不放宽断言；
- `--workspace` 门遇到该红时：记录当时的 `free` 与 `cargo/rustc` 计数，单跑该二进制确认，若单跑绿则视为环境 flake 并留痕；
- 若将来单跑也红，再按真实缺陷升级处理。
