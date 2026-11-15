# `wg` 故障时的合规逃生路径（低 ref 克隆 + 规范密钥路径）

- 适用：`wg collab entry` 报 `RegistryReadFailed` / `GitFailed` / 挂起 >120s，且主检出 `refs/collab/*` 达 1.5 万量级
  （成因见 `docs/walgit-cli-ref-scale-20261115.md`：客户端按 ref 数 O(n)，≈7.7ms/ref）。
- 前提：**不复制任何密钥**、**不手写 refs**、**不绕签名**。签名仍是 `sign_entry.sh` + `wg collab entry`，只是换一个 **ref 很少的仓库根**来跑。

## 为什么要换仓库根

`refs/collab/*` 是**仓库级共享**的，任何 worktree 都看得见同一批 1.5 万条 ref；
`git clone --shared` 通过 alternates 复用主仓的**对象库**，但**不复制 refs**——所以我们能用最小 ref 集跑 `wg`。

## 步骤（每次约 2 分钟）

```bash
MAIN=/e/Users/gxh/Documents/GitHub/OpenMir2
CLONE=/e/UserTemp/wgsign            # 每次独立目录，用完即删
PRINCIPAL=openmir2-orchestrator     # 换成你自己的 principal
KEY=C:/Users/gxh/.walgit/keys/openmir2-orchestrator.ed25519   # 规范密钥路径

# 1) 低 ref 克隆（共享对象库，不复制 refs）
rm -rf "$CLONE" && mkdir -p "$CLONE"
git clone --shared --no-checkout "$MAIN" "$CLONE"
git -C "$CLONE" remote add walgit http://127.0.0.1:8081/gqf2008/OpenMir2.git

# 2) identity 只写「指向规范密钥路径」的 JSON —— 绝不复制密钥文件
mkdir -p "$CLONE/.git/walgit"
printf '{\n  "key": "%s",\n  "principal": "%s"\n}\n' "$KEY" "$PRINCIPAL" > "$CLONE/.git/walgit/identity"

# 3) sign_entry.sh 的 --root / 线程判空需要看板定义
cp "$MAIN/.walgit/board.toml" "$CLONE/.walgit/board.toml" 2>/dev/null || { mkdir -p "$CLONE/.walgit"; cp "$MAIN/.walgit/board.toml" "$CLONE/.walgit/board.toml"; }

# 4) 只拉本 principal 的 meta（1 条 ref 即可；要审别的线程时再加该线程所需 refs）
git -C "$CLONE" fetch walgit "+refs/collab/meta/principals/$PRINCIPAL:refs/collab/meta/principals/$PRINCIPAL"

# 5) 正常签名（走 sign_entry.sh 的全部校验：oid 闸 / parent / stale-body）
cd "$CLONE"
WALGIT_LOCAL_COMMANDS=1 "$MAIN/tools/collab/sign_entry.sh" --repo . \
  --kind comment --id <thread> --parent <parent_oid> --body-file <body.json>

# 6) 单条 ref push（不要 push 整个 inbox）
git push walgit "refs/collab/inbox/$PRINCIPAL/<entry_id>:refs/collab/inbox/$PRINCIPAL/<entry_id>"

# 7) 回主检出单条 fetch 验证（不要用通配 fetch，会把 1.5 万条拉回来）
git -C "$MAIN" fetch walgit "+refs/collab/inbox/$PRINCIPAL/<entry_id>:refs/collab/inbox/$PRINCIPAL/<entry_id>"

# 8) 清理并记账（见下）
```

## 安全规则（硬）

1. **零密钥复制**：identity 是**指针 JSON**，不是密钥副本。删除前必须 `find "$CLONE" -name '*.ed25519'` 为**空**。
2. **不手写 refs**：所有条目必须由 `wg collab entry` 生成（有真签名）。禁止 `git hash-object` + `update-ref` 手工造条目
   （2026-11-15 出现过一次，虽然事后验签通过，但绕过了 oid 闸 / parent / stale-body 三道检查）。
3. **单条 push / 单条 fetch**：任何 `refs/collab/*` 通配 fetch 都会把 1.5 万条 ref 拉回主检出，等于自毁变量。
4. **stale-body**：body 文件超过 1800s 会被拒；确认不是上一张卡的残留后加 `--allow-stale-body`，并在报告里注明。
5. **密钥操作先报后做**（做什么/在哪/怎么清/替代方案）。

## 记账模板（删临时目录前填）

```
CLONE=<路径>
创建=<时间>   删除=<时间>
删前检查：find <CLONE> -name '*.ed25519' → 空；git -C <CLONE> status --short → 空
删除后复核：目录 gone；$TEMP 下无 <名>* 残留
签名条目：<entry ref/oid>  已 push=<yes>  主检出回读=<yes>
```

## 已知限制

- 这只解决「签名」；**读**（`collab_read.py`）直接走 git 读本地 refs，不受影响。
- 克隆里的 `wg collab thread/board` 在 ref 很少时可用（实测 7/7、10/10 `verified=true`），但**不要**在主检出上跑它们。
- 根治仍需客户端改为批量/增量 fetch（见 `docs/walgit-issue-ref-scale-20261115.md`）。
