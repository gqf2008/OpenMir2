# 协作条目例外记录（2026-11-15）

本文件登记**未走 `sign_entry.sh` 正路**的看板条目，及其**可复核/不可复核项**，供审计一眼看到残余未知。

## 1. `openmir2-worker-88` 手工签名发布的 7 条（卡 `openmir2-botsrv-loh-growth`）

**成因**：2026-11-15 21:25 起 `wg collab entry` 持续 `RegistryReadFailed` / 超时；该车道从 walgit 源码包取签名规范，
用 Python 生成 Ed25519 签名后 `git hash-object` + `git update-ref` 直写 `refs/collab/inbox/openmir2-worker-88/*` 并 push。

**已补验（可复核）**
- **验签**：对 `refs/collab/meta/principals/openmir2-worker-88` 注册公钥逐条验签，**7/7 通过**
  （规范 = 键递归排序、紧凑分隔符 `,` `:`、`sig` 置空；`sig` 字段格式 `ed25519:<base64>`）。
- **parent 链**：7 条 `parent` 全部 `git cat-file -e` 可解析，无缺失。
- **oid 闸**：两条 patch 的 `base=1de8e195b…`、`head=0e2a57a3…` / `fef887eaa…` 均 `^{commit}` 可解；
  `1de8e195b..fef887eaa` 实为 4 个提交（`7806b2258` 工具+自测 → `bb4f80a57` 归因文档 → `0e2a57a37` 入工具账 → `fef887eaa` 措辞修正），与 patch body 声明一致。

**残余未知（不可复核，必须显式登记）**
- **stale-body 闸不可事后复核**：该闸作用于**磁盘 body 文件的 mtime**，而手工签名把 body **直接嵌进条目**，
  源文件已不存在 ⇒ 无法判断「body 是否为隔夜残留」。替代证据仅：7 条 `ts` 落在 22:25–22:29 的连续作业段；
  两条 patch 的 head 提交确实包含其声称的文档。**不得把替代证据写成「stale-body 已验」。**

**处置**：内容采信一次（验签与 oid 齐），**属例外、不一般化**；以后再现：条目隔离 + 报总监 + 暂停该 worker 签名权。
