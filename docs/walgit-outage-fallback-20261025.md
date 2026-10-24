# walgit 长故障兜底：交付物 bundle + 补推计划 + 恢复判定（2026-10-25）

- 卡：`openmir2-walgit-outage-fallback`（tier **L**；owner `openmir2-reviewer-20`；branch `review/walgit-fallback`）。
- 基线：本地 `master = dc0430eda`（walgit 故障期，本地主源）。
- 写集：**只新增本文档**；bundle 与临时克隆都在 `E:/tmp/openmir2-fallback/`（**不入库**）。
- **红线（本卡已遵守）**：**未 push 任何远端**（walgit 与 origin 都没推）、未把 bundle 提交进仓库、
  未改代码/卡状态、未动 `E:/MirServer`、未起服务端。

## 1. 现状（故障快照 @ `dc0430eda`）

- **故障现象**：`git ls-remote walgit refs/heads/master` 报
  `remote: internal error: wal: object not found: repos/gqf2008/OpenMir2/log/0000000000003000.pb` +
  `error: 500`（walgit 服务端 store 的 R2 WAL 段缺失）⇒ 所有 fetch/push 不可用。
- **最后已知 walgit master**：`4ae3dc810`（2026-10-24 23:51）。
- **本地 master**：`dc0430eda`；`git rev-list --count refs/remotes/walgit/master..master` ⇒ **95**（活量）。
- **GitHub 镜像 origin**：`refs/remotes/origin/master` 落后本地 **25**（`git rev-list --count origin/master..master`，活量）。
- **协作 ref 活量**：`refs/collab/inbox/*` = **9383**、`refs/collab/meta/*` = **99**（本文件写作时点；持续增长）。
- 本地 `refs/heads/card/*` = **7** 条（见 §3 表）。

> 上述数字均为**活量**（agent 持续提交/合并会变）；复算命令见各节。

## 2. 兜底 bundle（自校验）

### 2.1 生成

```sh
mkdir -p E:/tmp/openmir2-fallback
cd <本检出>
git bundle create E:/tmp/openmir2-fallback/openmir2-20261025.bundle --all
```

- **为什么用 `--all` 而不是逐个列 ref**：`git bundle create` 的 ref 参数**不做通配展开**
  （`refs/heads/card/*` 被原样传给 rev-list ⇒ 会被当路径 ⇒ 「master is excluded by the rev-list options」、
  拒建空 bundle）；而 9k+ 条 `refs/collab/inbox/*` 逐个列会超出命令行长度上限。
  `--all` 是**超集**：包含 `refs/heads/*`（master + 7 条 card 分支）、`refs/collab/inbox/*`、`refs/collab/meta/*`，
  另含 `refs/archive-origin/*`、`refs/rescue/*` 等历史归档 ref（一并兜底，不影响可用性）。
- **bundle 的 `HEAD`** = 生成时该 worktree 的 `HEAD`（可能是某个 `review/*` 分支）⇒ **恢复时用
  `git clone -b master <bundle>`** 或克隆后 `git checkout master`。

### 2.2 产物与校验值（本次实测）

| 项 | 值 |
|---|---|
| 路径 | `E:/tmp/openmir2-fallback/openmir2-20261025.bundle` |
| 大小 | `166132533` 字节（≈158.4 MiB） |
| sha256 | `24d39daccb5051c077ac8e8accc83a6b2a8278f6ce272462c91b51d04306e927` |
| ref 数（`git bundle list-heads`） | `11470` |
| 生成时 master | `dc0430eda` |

### 2.3 自校验（命令 + 输出）

```sh
# ① verify（在空仓里验 prerequisites，自包含 bundle 应报 complete history）
git init -q --bare E:/tmp/openmir2-fallback/verify.git
git -C E:/tmp/openmir2-fallback/verify.git bundle verify E:/tmp/openmir2-fallback/openmir2-20261025.bundle
#   ⇒ rc=0；末两行：`The bundle records a complete history.` / `The bundle uses this hash algorithm: sha1`

# ② 从 bundle 克隆 + 抽查 6 个 ref 的 oid 与主仓一致
git clone -q -b master E:/tmp/openmir2-fallback/openmir2-20261025.bundle E:/tmp/openmir2-fallback/clone
git -C E:/tmp/openmir2-fallback/clone rev-parse master      # ⇒ dc0430edacaebc189a00c4749ab5d19d7a9dbfbd
for r in refs/heads/master refs/heads/card/castle-official-actions \
         refs/collab/meta/principals/openmir2-dev-2 <一条 refs/collab/inbox/openmir2-reviewer-20/*> \
         <一条 refs/collab/inbox/openmir2-orchestrator/*> refs/archive-origin/archive/m3bag-equip-slot4; do
  m=$(git rev-parse "$r"); b=$(git bundle list-heads E:/tmp/openmir2-fallback/openmir2-20261025.bundle | awk -v r="$r" '$2==r{print $1}')
  echo "$([ "$m" = "$b" ] && echo OK || echo MISMATCH) $r main=$m bundle=$b"
done
```

本次输出（6/6 OK）：

```text
OK refs/heads/master main=dc0430edacaebc189a00c4749ab5d19d7a9dbfbd bundle=dc0430edacaebc189a00c4749ab5d19d7a9dbfbd
OK refs/heads/card/castle-official-actions main=c65f0b0b9c89c2ad50192674dced87d4e20e9db0 bundle=c65f0b0b9c89c2ad50192674dced87d4e20e9db0
OK refs/collab/meta/principals/openmir2-dev-2 main=001cc06f8862ff95687d8593a1f8b57c03e17cdd bundle=001cc06f8862ff95687d8593a1f8b57c03e17cdd
OK refs/collab/inbox/openmir2-reviewer-20/f11e8af7066400f860fb9655b8912376 main=d822fbcf7a71e8cb29bda1bc5163b61abe3844e2 bundle=d822fbcf7a71e8cb29bda1bc5163b61abe3844e2
OK refs/collab/inbox/openmir2-orchestrator/0026da5c32e3b4e75ad424028dfefaed main=af4d3d109490125a27475194152a62cac8ec82f8 bundle=af4d3d109490125a27475194152a62cac8ec82f8
OK refs/archive-origin/archive/m3bag-equip-slot4 main=0616c2804175d32015c065eea4c60d0d98bb2409 bundle=0616c2804175d32015c065eea4c60d0d98bb2409
```

> **bundle 是时点快照**：生成后新增的提交/协作条目（含本卡的 patch/status 两条）不在里面；
> 需要时按 §2.1 重打包（bundle 覆盖写同一路径），或恢复后按 §3 直接补推。

## 3. 补推清单（walgit 恢复后）

| 项 | 值（快照 @ `dc0430eda`，活量） | 命令 |
|---|---|---|
| 主线 | 本地 master 领先最后已知远端 **95** | `git push walgit master` |
| card 分支（7 条） | `card/castle-official-actions` `c65f0b0b9` / `card/script-handler-msgqueue` `d1dd5dbc4` / `card/window-closure-ledger` `9e88020b5` / `card/window-kit-coverage` `18900a260` / `card/window-kit-p0-unblock` `4953b10ae` / `card/window-run2-castle-list-caliber` `fa24260b0` / `card/window-scenes-clearmapmon` `725bd57be` | `git push walgit <逐个分支>`（已合并的推送仅作审计/身份留存） |
| 协作条目 | `refs/collab/inbox/*` **9383**、`refs/collab/meta/*` **99**（活量，补推前重数） | `git push walgit 'refs/collab/inbox/*:refs/collab/inbox/*'`；`git push walgit 'refs/collab/meta/*:refs/collab/meta/*'` |

复算：`git rev-list --count refs/remotes/walgit/master..master`；`git for-each-ref refs/heads/card`；
`git for-each-ref refs/collab/inbox | wc -l`；`git for-each-ref refs/collab/meta | wc -l`。

## 4. 恢复判定（三条命令，全绿才算恢复）

```sh
# ① 远端读通：应打印 refs/heads/master 的 sha（不再 500）
git ls-remote walgit refs/heads/master

# ② 协作面可读：看板能生成、条目数与本地量级相当
walgit collab board --format json | python -I -c "import json,sys;d=json.load(sys.stdin);print(sum(len(c['cards']) for c in d['columns']))"

# ③ 一次 dry-run push（不发对象，只协商）：rc=0 且不报 500
git push --dry-run walgit master
```

判定：① 返回 sha 且 ② 的卡数 > 0 且 ③ rc=0 ⇒ 恢复；任一条失败 ⇒ 仍按故障期处理（本地主源 + §3 延后补推）。

## 5. 恢复后的步骤顺序

1. **先推主线**：`git push walgit master`（远端先有主线，后续分支/条目才有参照）。
2. **再推 card 分支**（§3 表逐条；推荐 `git push walgit --all` 之外**显式列名**，避免把临时 `review/*` 分支一起推上去）。
3. **最后推协作 ref**：`inbox` → `meta`（条目引用链以 inbox 为主；两者都是活量，**推前重数**）。
4. **恢复核对**：`git ls-remote walgit refs/heads/master` 应等于本地 `git rev-parse master`；
   `git for-each-ref refs/collab/inbox | wc -l` 与推送后远端一致；随后把本报告 §1/§3 的数字按届时重数更新。

## 6. 若 walgit **永不恢复**（需 owner 批准，本卡不执行）

- **bundle 是审计链兜底**：§2 的 bundle 自包含（`complete history` + 6 ref 抽查一致），可在任意机器
  `git clone -b master <bundle>` 复原到 `dc0430eda` 的全部主线/分支/协作 ref。
- **GitHub origin 镜像**：可把 master 与 card 分支推到 origin（当前 origin/master 落后 25）；
  `refs/collab/*` 属自定义命名空间，GitHub 允许推送（`git push origin 'refs/collab/inbox/*:refs/collab/inbox/*'`），
  可作为协作审计链的异地备份——**但会公开可见，是否可接受由 owner 决定**。
- 无论走哪条路，**先保留 bundle 与本地 `.git`**，不要 `git gc --prune` 掉未推送对象。

## 7. 残留与复算

- 产物：`E:/tmp/openmir2-fallback/` = `openmir2-20261025.bundle`（§2.2）+ `verify.git/`（空仓，仅用于 verify）
  + `clone/`（抽查用临时克隆，可删）。**bundle 不入库**；`E:/tmp` 下无其它本线残留。
- 本卡**未推任何远端**；walgit 仍 500（§4 的判定在恢复前必然失败）。
- 本文件是唯一仓库改动；门禁与 `git diff --check <base> <head>`、merge_tree 见 patch body。
