# `wg` 故障时的合规逃生路径（低 ref 克隆 + 规范密钥路径）

- 适用：`wg collab entry` 报 `RegistryReadFailed` / `GitFailed` / 挂起 >120s，且主检出 `refs/collab/*` 达 1.5 万量级
  （成因见 `docs/walgit-cli-ref-scale-20261115.md`：≥200 条 collab ref 时 wg 直接挂死（199 通过 0.27s、200 挂住），断崖精确在 200；「每 ref 7.7ms」是超时截尾的伪值，已作废。机制疑为 `cat-file --batch` 双向互锁（8 KiB 管道容量），属强证据、待直接观测。）
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

**签完立刻做 actor 自检**（2026-11-16 加）：回读该条目，断源码 `actor` **等于本车道 principal**；
不等即判本次签卡失败并立刻上报。理由：整套协作的可信度建立在「谁签的」上，条目落到别人的身份下
比签不出来严重得多。命令示例：`git cat-file -p <entry_ref> | python -I -B -c "import json,sys;d=json.load(sys.stdin);print(d['actor'])"`。


**克隆目录必须按 principal 区分**（2026-11-16 加）：例如 `E:/UserTemp/w<principal后缀>sign`。
绝不能多人共用同一个克隆——克隆内的 `.git/walgit/identity` 决定签名身份，共用会在别人的 principal 下落条目（冒名）。
发现有人指向你的克隆，先停、再让对方自建。


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

## wg 挂死后的系统层清理（2026-11-16 加）

wg 挂死会在系统层面留下**第二个痕迹**：它启动的 `git -C . cat-file --batch` 子进程。
**停掉 wg 之后必须确认这些子进程也退出**，否则会跨天累积（现场观察到 2026-11-13 起的多个实例）。

```powershell
Get-CimInstance Win32_Process | Where-Object { $_.Name -eq 'git.exe' -and $_.CommandLine -like '*cat-file*' } |
  ForEach-Object { "{0}|parent={1}|created={2}" -f $_.ProcessId,$_.ParentProcessId,$_.CreationDate }
```

**归属判据三条缺一不可**（缺任一条不要动，先上报）：① 命令行确为 `cat-file --batch`；
② **父进程已不存在**；③ 创建时间与某次 wg 挂死时刻吻合。三条齐才可终止并记账。
**注意**：若父进程是长期存活的 `wg mcp`，那是 MCP 的常驻 helper，不是僵尸，**不要杀**。

## 已知限制

- 这只解决「签名」；**读**（`collab_read.py`）直接走 git 读本地 refs，不受影响。
- 克隆里的 `wg collab thread/board` 在 ref 很少时可用（实测 7/7、10/10 `verified=true`），但**不要**在主检出上跑它们。
- 根治仍需客户端改为批量/增量 fetch（见 `docs/walgit-issue-ref-scale-20261115.md`）。

## 向上游 walgit 仓库报障的专用身份（2026-11-15 建立）

- **何时用**：需要向 walgit 仓库（`http://127.0.0.1:8081/gqf2008/walgit.git`）提交 issue/comment 时。
  该仓库的 GitHub issues 是**关闭**的，`CONTRIBUTING.md` 明确 issue/PR/评审都走 `refs/collab/*` 签名条目
  ⇒ **只能**用他们自己的账本报障，而报障必须有身份。
- **身份**：principal `openmir2-orchestrator`（在 walgit 仓库的 `refs/collab/meta/principals/` 单独注册，
  与 OpenMir2 仓库里同名 principal **使用不同密钥**，两套注册表互不影响）。
- **密钥**：`~/.walgit/keys/walgit-d1-reporter.ed25519`（2026-11-15 创建；由 `wg collab join` 在临时克隆内生成后
  **移动**到规范目录，不是复制）。**它不是通用身份**——**不要**拿它签 OpenMir2 自己的卡。
- **用法**：低 ref 克隆 walgit 仓库 → `.git/walgit/identity` 写
  `{"key":"C:/Users/gxh/.walgit/keys/walgit-d1-reporter.ed25519","principal":"openmir2-orchestrator"}` →
  拷该仓库自己的 `.walgit/board.toml` → 只 fetch 该仓库 `refs/collab/meta/*` → `sign_entry.sh` 签 → 单条 push。
- **已提交**：thread `walgit-cli-ref-scale-o1`（`wg` 在 ≥200 条 collab ref 时挂死；旧「线性劣化」结论已作废并已上游更正）。
- **硬规避**：任何 wg 操作必须在 **<200 条** collab ref 的仓库根跑。**清到 500/1000 条无效**——逃生路径之所以有效，正是因为它落在 200 以下。
- **注意**：同名 principal 两把钥匙并存是刻意的；**任何签名前先确认当前仓库用的是哪把**
  （看 `git rev-parse --git-path walgit/identity`）。密钥迁移/轮换一律先报后做。
