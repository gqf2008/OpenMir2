# `wg collab board` 的 ref 规模曲线：阶跃在 200 条 inbox（2026-11-16 实测）

> 只读测量。所有 `wg` 调用都在 `git clone --shared` 的临时克隆里执行，
> **主检出未运行任何 `wg` 读命令**（否则会全量 fetch 回 `refs/collab/*`，毁灭变量）。
> 背景：`docs/walgit-cli-ref-scale-20261115.md`；机制先例：`docs/collab-wg-deadlock-20261107.md`。

## 0. 结论

1. **不是线性劣化，是阶跃**：
   - `N ≤ 199`：**0.23–0.36 s**。`N=0` 也要 0.23–0.25 s，说明这段基本是进程启动成本；
     从 0 到 199 的斜率约 **0.12 ms/ref**，在噪声量级，不构成可测的线性项。
   - `N ≥ 200`：在 120 s 上限内**不返回**（`rc=124`）。
2. **断崖精确落在 200 条**：199 条通过，200 条挂住。
   每条约 **41 B**（40 hex oid + LF）：`199 × 41 = 8159 B` 通过，`200 × 41 = 8200 B` 挂住。
   `8192 B = 8 KiB` 与本机管道容量吻合（由边界反推，见 §6.5）⇒ 写入侧刚好越过管道缓冲。
3. **机制是既有的 `git cat-file --batch` 双向互锁**（`docs/collab-wg-deadlock-20261107.md` §3）：
   `wg` 把全部 oid 一次写入子进程 stdin 后才开始读 stdout；8 KiB 写满后，
   `git` 的 stdout 也早已写满并停止读 stdin ⇒ 双向互锁。
   本次的进程观测与之一致：挂住期间 `wg` 只有 `git -C . cat-file --batch` 一个子进程，
   二者 CPU 合计 <0.1 s（阻塞，不是在算）。
4. 因此 2026-11-15 文档里的 **“15,670 / 120 s ≈ 7.7 ms/ref” 是截尾时间除以 N**，
   不是逐 ref 工作量：真实路径在第 200 个 oid 处就停住了，后面的 1.5 万条根本没有进入管道。
5. **断崖与内容无关**：199/200 条指向同一个 blob 的合成 ref，同样在 200 翻转。
   meta refs 不进入这个批次：`199 inbox + 116 meta`（315 条总 ref）仍 1.16 s 返回。
6. **单个 `patch` 条目不是原因**：`1 patch + 116 meta`（117 条总 ref）1.218 s `rc=0`。

## 1. 方法

- 测量对象：`wg 0.1.0`（zig 客户端），Git 2.55.0.windows.2，Windows 10 19045。
- 临时克隆：`git clone --shared --no-checkout`，对象库经 `objects/info/alternates`
  共享主检出，**不复制任何对象**。
- 受控 upstream：一个裸仓库，先按**显式 refspec、按名**从主检出 fetch 恰好 N 条
  `refs/collab/inbox/*`（不用通配 fetch），再把 `refs/heads/master` 指到主检出的
  `master`（让 `wg` 能读到 `.walgit/board.toml`）。
  测量克隆的 `origin` 指向该裸仓库，因此即使 `wg` 的读路径发起
  `fetch refs/collab/*`，也只能拿回同样的 N 条，**变量稳定**。
- 每次 `wg` 调用前后都记录 `git for-each-ref refs/collab | wc -l` 的实际值
  （表 1 的“实测 ref 数”列）；全部点数的测前/测后值相等。
- 墙钟：Python `time.monotonic()`；stdout/stderr 重定向到文件（不用管道，
  避免“孙进程持有 stdout 导致 `communicate` 永不返回”的伪超时）。
- 超时：表 1 用 **120 s**，超时由父进程 `kill` 并记 `rc=124`；断崖定位（表 2）
  用 **30 s** 上限（下界已由表 1 的 120 s 给出）。
- 主检出只运行 `git for-each-ref` / `git cat-file` / `git fetch <name>` 等只读命令。

## 2. 表 1：指定的 N 扫描（120 s 上限，每点 2 次）

测量时刻主检出：`refs/collab/inbox` = 15,584–15,586 条，`refs/collab/meta` = 115–116 条
（测量期间有其他会话新增条目，逐次测前/测后计数见列）。
采样集为字典序前 N 条 inbox ref（该前缀全部属于 `openmir2-coordinator`）。

| N | 实测 ref 数（测前/测后） | 第 1 次耗时 | 第 2 次耗时 | rc |
| ---: | --- | ---: | ---: | --- |
| 0 | 0 / 0 | 0.250 s | 0.234 s | 0 |
| 3 | 3 / 3 | 0.360 s | 0.297 s | 0 |
| 100 | 100 / 100 | 0.281 s | 0.281 s | 0 |
| 500 | 500 / 500 | 120.016 s | 120.016 s | 124 |
| 1000 | 1000 / 1000 | 120.016 s | 120.016 s | 124 |
| 2000 | 2000 / 2000 | 120.016 s | 120.016 s | 124 |
| 5000 | 5000 / 5000 | 120.016 s | 120.031 s | 124 |

> `rc=124` 是**父进程超时**，不是 `wg` 自己返回的退出码。按任务约定不在 120 s 后继续等待。
> 既有记录（2026-11-07，14,529 条规模）显示 `wg` 最终会在约 2 分钟后报 `GitFailed`；
> 本次未等到该退出。

表 1 已经给出“100 快、500 挂”的粗断崖；下面把它夹到一条 oid 的宽度。

## 3. 表 2：断崖定位（30 s 上限，单次；199 与 200 为决定性点）

| N | 实测 ref 数 | oid 请求字节（N×41） | 耗时 | rc |
| ---: | ---: | ---: | ---: | --- |
| 150 | 150 | 6150 | 0.281 s | 0 |
| 190 | 190 | 7790 | 0.297 s | 0 |
| 195 | 195 | 7995 | 0.281 s | 0 |
| 198 | 198 | 8118 | 0.281 s | 0 |
| **199** | **199** | **8159** | **0.266 s** | **0** |
| **200** | **200** | **8200** | **30.016 s** | **124** |
| 201 | 201 | 8241 | 30.016 s | 124 |
| 250 | 250 | 10250 | 30.015 s | 124 |
| 300 | 300 | 12300 | 30.015 s | 124 |
| 400 | 400 | 16400 | 30.015 s | 124 |

- 199 与 200 只差 1 条 ref / 41 B，行为从 `rc=0, 0.27 s` 直接翻到挂住。
  N=199 复测两次：0.297 s / 0.297 s，`rc=0`。
- `199 × 41 = 8159 ≤ 8192`，`200 × 41 = 8200 > 8192`：断崖位置与 8 KiB 管道容量吻合。
- 前缀中第 329 条才是第一个 `patch`；N=200/201 的挂住与该条目无关（见 §4 的负例）。

## 4. 对照实验（排除“内容 / meta / patch”解释）

| 对照 | 构造 | 总 ref 数 | 耗时 | rc |
| --- | --- | ---: | ---: | --- |
| 合成 ref，199 条 | 199 条 ref 全部指向**同一个** blob（约 80 B） | 199 | 0.297 s | 0 |
| 合成 ref，200 条 | 同上 | 200 | 30.000 s | 124 |
| 真实 ref + meta，199 inbox | 199 inbox + 116 meta | 315 | 1.156 s | 0 |
| 真实 ref + meta，200 inbox | 200 inbox + 116 meta | 316 | 30.016 s | 124 |
| 单 patch + meta | 1 条 `patch`（第 329 条）+ 116 meta | 117 | 1.218 s | 0 |

- **内容无关**：同一 blob 的 199/200 条合成 ref 同样在 200 翻转，说明不是条目大小、
  不是签名验证、不是具体卡面。
- **meta 无关**：116 条 meta ref 不进入该 oid 批次（199 inbox + 116 meta 仍通过；
  若一起批量写，`315 × 41 = 12915 > 8192` 早就该挂）。
- **patch 无关**：单独一个 `patch` 条目在 117 条总 ref 下 1.2 s 返回。

进程侧的直接证据（在表 1 的 N=2000 挂住期间采样）：

- `wg collab board`（pid 41652，01:02:11 启动）的唯一子进程是
  `git -C . cat-file --batch`（pid 23844，同一秒启动）。
- 数秒后两者累计 CPU 分别约 **0.05 s / 0.02 s**：既不是解析也不是验证，
  而是在管道上**互相等待**。这与 `collab-wg-deadlock-20261107.md` §3 的互锁描述一致。

## 5. 拟合与断崖判断

- **N ≤ 199**：`t(N) ≈ 0.24 s + 0.00012 × N`（即约 0.12 ms/ref）。
  `N=0` 的 0.23–0.25 s 与 `N=199` 的 0.266 s 同量级，这条“斜率”主要是输出与进程启动噪声；
  可以认为**没有可观测的线性项**。
- **N ≥ 200**：不是“更慢的线性段”，而是**不完成**（在 120 s / 30 s 上限内）。
  它不是曲线上的点，而是另一个状态（互锁）。
- **断崖点**：**200 条 inbox ref**（本机、本版本、8 KiB 管道）。
  上限形态是 `floor(8192 / 41) = 199` 条通过。
- 这同时修正了 2026-11-07 文档 §3 的估算：那里假设 64 KiB 管道 ⇒ 阈值约 1,600；
  本机实测边界是 200，说明该环境的管道是 8 KiB。**模型（互锁 + 管道容量/41B）对，
  常数取决于环境。**
- 原文档的 7.7 ms/ref 应改写为：**“第 200 条 oid 处触发管道互锁；120 s 是观测上限，
  不是真实计算时间”**。

## 6. 没有测到的点与原因

1. **N ≥ 200 的真实完成时间**：`wg` 在 120 s 内不返回；按任务只记 `rc=124`，
   没有继续等它的内部超时。既有文档记为约 2 分钟后 `GitFailed`，本次未复验该退出点。
2. **15,586 条（当前主检出全量）**：不在任务给定点表内；且主检出禁止跑 `wg` 读命令。
   200 条已经跨越断崖，全量点只会是同一个挂住状态。
3. **真实 checkout 的完整 meta 集合（115–116 条）下的大 N**：只做了 199/200 的 meta 对照，
   没有做 `500/1000/2000/5000 + meta` 的完整扫描（meta 对照已证明边界不移动）。
4. **其他 `wg` 读命令**（`ls` / `thread` / `report` / `pr`）：本卡只测 `board`。
   既有文档显示它们共用同一聚合读路径，但本次没有逐条复验。
5. **管道容量 8192 B 是推断值**：由“8159 通过 / 8200 挂住”与 41 B/oid 反推，
   没有用 OS 层 API 直接读取管道缓冲大小。
6. **N ≥ 200 的运行间方差**：无法测量（全部被截尾），只能报 `rc=124`。
7. **N=5000 的第 2 次 / rc**：已测，120.031 s，`rc=124`（与第 1 次 120.016 s 同为截尾态，
   运行间方差不可测）。

## 7. 临时物清理记账

- 临时根：`E:/UserTemp/wg-ref-bisect-20261116/`（`$TEMP` 本机指向 `E:\UserTemp`）。
- 脚本运行期按点创建的 upstream 裸仓库与测量克隆（`upstream.git`、`measure-*`、
  `upb-*.git`、`mb-*`、`upm-*.git`、`mm-*`）在每个点测完后由脚本删除；
  失败的手工探针目录 `measure/` 与 `synth/` 也已删除。
- 删除前检查：`find E:/UserTemp/wg-ref-bisect-20261116 -name '*.ed25519' -o -name '*.key' -o -name 'id_*'` ⇒ **0 个**
  （未复制任何密钥；本任务未签名、未 push）。
- 最终清理记录（2026-11-16）：
  - 删除前清点：**652 个文件 / 117 个目录 / 3.1 MiB**（脚本、日志、`results*.json`、
    残留的 `measure/` 手工探针克隆、`upstream.git`、`synth`、`synth2`）；
    **密钥文件 0 个**。
  - 删除时间：**01:11:00**；删除后 `[ -e ]` 复核 ⇒ gone；
    `ls -d /e/UserTemp/wg-ref-bisect*` ⇒ No such file or directory。
  - 同时删除了为 key 检查临时写出的 `/e/UserTemp/wg-keycheck.txt`（0 字节）。
- 主检出与任何身份 worktree 均未被本任务写 ref、写密钥或 push（只读 `git for-each-ref` /
  `git cat-file` / 按名 `git fetch` 到临时裸仓库）。

## 8. 复现脚本

下面是本次实际使用的扫描脚本（去掉逐点日志的裁剪版；完整版在测量机临时目录，
已随 §7 删除）。所有 `git` 均为只读或本地 fetch，`wg` 只在克隆内运行。

```python
import subprocess, time, os, shutil, json

MAIN = "E:/Users/gxh/Documents/GitHub/OpenMir2"
WORK = "E:/UserTemp/wg-ref-bisect-20261116"
MAIN_OBJECTS = MAIN + "/.git/objects"
N_LIST = [0, 3, 100, 500, 1000, 2000, 5000]   # 表 1
CAP = 120.0                                    # 表 2 用 30.0

def sh(cmd):
    return subprocess.run(cmd, capture_output=True, text=True)

def git(repo, *args):
    p = sh(["git", "-C", repo, *args])
    if p.returncode != 0:
        raise RuntimeError(p.stderr[:400])
    return p

def count(repo):
    p = git(repo, "for-each-ref", "--format=%(refname)", "refs/collab")
    return len([l for l in p.stdout.splitlines() if l])

def measure(N):
    up = os.path.join(WORK, "upstream.git")
    clone = os.path.join(WORK, f"measure-{N}")
    for p in (up, clone):
        if os.path.exists(p):
            shutil.rmtree(p, ignore_errors=True)
    sh(["git", "init", "--bare", "-q", up])
    # 对象库共享主检出；不复制对象
    with open(os.path.join(up, "objects", "info", "alternates"), "wb") as f:
        f.write((MAIN_OBJECTS + "\n").encode())
    git(up, "update-ref", "refs/heads/master", git(MAIN, "rev-parse", "master").stdout.strip())
    # 按名取恰好 N 条 inbox ref（显式 refspec，不用通配）
    names = [l.split()[1] for l in git(
        MAIN, "for-each-ref", "--format=%(objectname) %(refname)", "refs/collab/inbox"
    ).stdout.splitlines() if l.strip()][:N]
    for i in range(0, len(names), 200):
        git(up, "fetch", "-q", "--no-tags", MAIN, *[f"+{r}:{r}" for r in names[i:i + 200]])
    sh(["git", "clone", "-q", "--shared", "--no-checkout", up, clone])
    git(clone, "fetch", "-q", "--no-tags", "origin",
        "+refs/collab/inbox/*:refs/collab/inbox/*")
    out = []
    for _ in range(2):
        pre = count(clone)
        of = os.path.join(WORK, "run.out")
        ef = os.path.join(WORK, "run.err")
        with open(of, "wb") as o, open(ef, "wb") as e:
            t0 = time.monotonic()
            p = subprocess.Popen(["wg", "collab", "board"], cwd=clone,
                                 stdout=o, stderr=e,
                                 env=dict(os.environ, WALGIT_LOCAL_COMMANDS="1"))
            try:
                rc = p.wait(timeout=CAP)
            except subprocess.TimeoutExpired:
                p.kill()
                rc = 124
            wall = time.monotonic() - t0
        out.append({"pre": pre, "post": count(clone), "rc": rc, "wall": round(wall, 3)})
    shutil.rmtree(up, ignore_errors=True)
    shutil.rmtree(clone, ignore_errors=True)
    return out

for N in N_LIST:
    print(N, measure(N), flush=True)
```

合成对照（§4）的最小复现：

```sh
mkdir r200 && cd r200 && git init -q .
printf '{"version":1,"kind":"comment","id":"x","actor":"a","ts":1,"sig":"e"}' > e.json
OID=$(git hash-object -w e.json)
mkdir -p .walgit && printf 'version = 1\n[[column]]\nname = "全部"\n' > .walgit/board.toml
git add -A && git -c user.email=a@b -c user.name=a commit -qm init
for i in $(seq 1 200); do echo "create refs/collab/inbox/a/$i $OID"; done | git update-ref --stdin
timeout 30 wg collab board   # 期望挂住；把 200 改成 199 则约 0.3 s 返回
```

## 9. 给上游的最小结论

`wg` 的批量对象读应在写入端达到管道容量前**分块写入并交替读 stdout**
（或改成一次一个 oid 的 `cat-file --batch` 复用 + 流式读）。
任何 N ≥ 200 的本地镜像都会触发互锁，因此“清理本地 collab ref”与“等待更久”
都不是有效处置；用 `docs/collab-wg-deadlock-20261107.md` 的 `collab_read.py`
读路径是正确的临时处置。
