# 掷骰 NPC「开-始-下-注」链接点击不发 `CM_MERCHANTDLGSELECT(1011)` —— 根因诊断（r7）

- 日期：2026-10-22
- 基线：`7eb281fac`；分支 `dice-link-diagnosis`（只读诊断，不改客户端/服务器，只出本文档）
- 证据根：`E:/tmp/window/r7/**`（第六轮真机窗口）；客户端源码 = 冻结仓库
  `E:/Users/gxh/Documents/GitHub/MirClient/Source/**`
- 证据哈希（sha256，2026-10-22 复核）：

| 证据 | 路径 | sha256 |
| --- | --- | --- |
| 抓包 | `E:/tmp/window/r7/s2c/capture.jsonl`（26690 B） | `169b4f37f6487ed738381eeff610187181fb1d7ed90736dca75900279e044545` |
| 截图 | `E:/tmp/window/r7/shot-dice5.png`（573429 B，1030×797） | `931945e46cc7aac039c1dfb41c47c44bc4e00e42827b45336b5c6d0a2ccd1903` |
| 脚本 | `E:/MirServer/M2GameSvr/Envir/Market_Def/赌博NPC/赌场-B013.txt`（8865 B） | `3831568d6e4d58307b04368eed6c6bd2b2b65cfdacb3417acf21babd11f83e4f` |
| 服务端日志 | `E:/tmp/window/r7/logs/game.out.log` | `51aa673fd326ddaaabc26efbd75346328f96f9160f324a24c508efb0773d60ef` |

---

## 0. 结论（TL;DR）

| 项 | 判定 |
| --- | --- |
| **根因层** | **fixture（点击坐标落在链接命中矩形外）** |
| 服务器 say 字节 | 与脚本 `[@main]` 段**逐字节相同**（226 B = `〖庄〗/` 7 B + 正文 219 B），5 个 `<...>` 标记全部完整 |
| 客户端链接识别 | 正常；命中矩形 = 尖括号内文本的像素框，`lx=30`、`ly=20+16k`、高 14 px |
| 「开-始-下-注」矩形 | client 坐标 **x 43..107、y 68..82** |
| 实际两次点击 | `(105,97)`：x 在矩形内，**y 低 15 px**；`(150,100)`：**x 落在两链接之间的空隙**（107..169），y 低 18 px |
| 直接后果 | 两点都仍在对话框面板内（约 434×174）⇒ 被对话框吞掉，**一个 c2s 帧都不发**（与抓包完全一致） |
| 「随后点击被判 `CM_TURN`/`CM_PICKUP`」 | **与本次抓包不符**：r7 c2s 最后一帧就是 `1010 CM_CLICKNPC`，`643` 说词在其后 112 ms 下发，此后直到断线 0 条 c2s；5 条 `3010 CM_TURN` 全在 `CLICKNPC` **之前** |

一句话：**链接没坏，服务器字节没坏，是点的位置不在链接上**（差一个客户区原点 `(3,26)`）。

---

## 1. 服务器说词字节（服务器侧不是根因）

### 1.1 帧定位与解码

`capture.jsonl` 第 54 帧：

```
dir=s2c  seq=54  ts_ms=1792662876562  raw_len=320
recog=119  ident=643(SM_MERCHANTSAY)  param=0  tag=0  series=1  body_len=226
```

帧结构（本次实测 + `mir2-rs/crates/protocol/src/frame.rs` 的 `decode_server_payload`）：
`#` + EDCode(12 B 头 = 16 字符) + EDCode(body) + `!`。

> ⚠ 结尾 `!`（0x21）是**终结符**，不属于 body。把 `!` 一起解码会多出 1 B，并把末字节解成
> `0x3A 0xA8` —— 这是本轮最容易踩的**假不匹配**。`body_len` 字段 = 226 与「去掉 `!` 后正好 226 B」
> 一致，可自证。

解码结果（GBK）：

```
〖庄〗/欢迎光临-天时地利人和-赌场-系统坐庄-童叟无欺\ \ \※<开-始-下-注/@kaiz>※         ※<筹-码-兑-现/@chouma>※\ \ \注：下注前请先在<商铺>兑换[筹码]！\ \您当前的元宝为:<$GAMEGOLD>个                           <返回土城/@huit>
```

### 1.2 与脚本原文逐字节比对：完全相同

脚本 `赌场-B013.txt` 的 `[@main]` 段（第 2–5 行，CRLF，去掉 CRLF 后拼接 = **219 B**）：

| 段 | 字节 | 说明 |
| --- | --- | --- |
| `〖庄〗/` | 7（`a1bc d7af a1bd 2f`） | NPC 名 + `/`；oracle `ChrName + '/' + msg` |
| `[@main]` 正文 | 219 | 与文件同名段逐字节相同 |
| **合计** | **226** | = 线上 `body_len`；`body == 期望字节` 判定 **True** |

五个链接标记逐一核对（GBK 原始字节 + body 内偏移）：

| 标记 | 偏移 | GBK hex | 完整 |
| --- | --- | --- | --- |
| `<开-始-下-注/@kaiz>` | 58 | `3c bfaa 2d cabc 2d cfc2 2d d7a2 2f 40 6b61697a 3e` | ✅ |
| `<筹-码-兑-现/@chouma>` | 90 | `3c b3ef 2d c2eb 2d b6d2 2d cfd6 2f 40 63686f756d61 3e` | ✅ |
| `<商铺>` | 134 | `3c c9cc c6cc 3e` | ✅ |
| `<$GAMEGOLD>` | 170 | `3c 2447414d45474f4c44 3e` | ✅ |
| `<返回土城/@huit>` | 210 | `3c b7b5bbd8cdc1b3c7 2f 40 68756974 3e` | ✅ |

反斜杠字节位置：`[51, 53, 55, 113, 115, 117, 152, 154]` —— 共 8 个 `\` ⇒ 9 个显示行
（`\ \ \` 中间是**空格**，所以每个 `\` 都切出一个非空段，空行也各占 16 px；见 §2/§3）。

### 1.3 唯一真实差异：`$GAMEGOLD` 未替换（另一张卡，与链接命中无关）

| 项 | server | oracle |
| --- | --- | --- |
| `$GAMEGOLD` | 原样输出 `<$GAMEGOLD>`（`3c2447414d45474f4c443e`） | `GetLineVariableText` 替换成玩家元宝数（数字） |

该差异只影响第 9 行文字，**不影响** `@kaiz` 链接（第 4 行、偏移 58）。

### 1.4 C# oracle 对照

- `E:/tmp/window/**` 下共 13 份 capture（含 2 份 C#：`20261021-r3/csharp` 49 帧、
  `cliente2e/csharp` 116 帧），逐帧解码后**没有任何一份含 `643`**（两份 C# 都是登录/入图场景）。
  ⇒ 没有 C# 侧的商人说词真机抓包可直接 diff。
- 退而用源码路径做字节判据：`merchant_say_text` = `ChrName + "/" + msg`
  （`mir2-rs/bins/game-svr/src/npc.rs:51-68`，对应 oracle `ScriptEngine.cs:991/996` +
  `PlayObject.Message.cs:1724`），实测与脚本逐字节相符。
- **结论：服务器 say 字节与 oracle 一致，本条不需要改服务端代码。**

---

## 2. 客户端链接识别与命中规则（冻结客户端）

| # | 规则 | 位置 |
| --- | --- | --- |
| C1 | `SM_MERCHANTSAY(643)` → `ClientGetMerchantSay(msg.Recog, msg.param, DecodeString(body))`；`Recog` = 商人 ActorId（本帧 119），`param` = face | `ClMain.pas:10989-10990` |
| C2 | 说词按**第一个** `/` 切成 `npcname` + 正文；`npcname` 之后不再使用 | `ClMain.pas:13349-13363`（`GetValidStr3` 语义：`HUtil32.pas:1110-1175`） |
| C3 | `g_nCurMerchant := Recog`（119）；链接点击回发用的就是它 | `ClMain.pas:13356-13358` |
| C4 | `ShowMDlg`：`Left=Top=0`；`atlen(s,'\') < 15` ⇒ 小面板（本说词 8 个 `\`）；`SetImgIndex(g_WMainImages, 384)` 会把 **Width/Height 设成图片尺寸**（所以控件矩形 ≈ 面板矩形）；`RequireAddPoints := True` | `FState.pas:11827-11861`、`DWinCtl.pas:1310-1324` |
| C5 | 排版原点 `lx := 30; ly := 20`，每切一行 `ly := ly + 16`（空段也加） | `FState.pas:12015-12016`、`:12123` |
| C6 | 逐行找链接：`while (data<>'') and (Pos('<')>0) and (Pos('>')>0)`；`ArrestStringEx(data,'<','>',cmdstr)` 取尖括号内文本，**必须有 `>`** | `FState.pas:12023-12034`、`HUtil32.pas:1548-1590` |
| C7 | `cmdparam := GetValidStr3(cmdstr, cmdstr, ['/'])`：显示文本 = 第一个 `/` **之前**，回发文本 = **之后**（含 `@`）⇒ 显示 `开-始-下-注`，回发 `@kaiz` | `FState.pas:12065` |
| C8 | 命中矩形 = `Rect(lx+sX, ly, lx+sX+TextWidth(cmdstr), ly+14)`，**只覆盖尖括号内的可见文本**；只在首次绘制（`RequireAddPoints`）时登记 | `FState.pas:12097-12115` |
| C9 | 点击判定：client 坐标 vs `SurfaceX/Y(Left+rc)`，含边界（`>=`/`<=`）；命中 → `SendMerchantDlgSelect(g_nCurMerchant, p.rstr)` | `FState.pas:12551-12570` |
| C10 | 回发帧 = `CM_MERCHANTDLGSELECT(1011)` + `EncodeString(rstr)`；`rstr` 原样（`@kaiz` 不在客户端特判表里） | `ClMain.pas:7132-7253`（`:7251-7252`） |
| C11 | 点击冷却 `NPC_CILCK_INVTIME = 500` ms（命中一次后 500 ms 内不再响应） | `MShare.pas:80`、`FState.pas:12551/12566` |
| C12 | 只有落在对话框**控件矩形**内才被吞：`X∈[Left,Left+Width)`、`Y∈[Top,Top+Height)` + `OnInRealArea` 像素检查；否则落到世界 → `CM_TURN`/`CM_PICKUP` | `DWinCtl.pas:984-1024`、`:1270-1305`、`FState.pas:12131-12142` |
| C13 | 点击坐标来源是**真实光标**：`FormClick` 用 `GetCursorPos`+`ScreenToClient`（并按 `SCREENWIDTH/ClientWidth` 缩放，本客户端 `ClientWidth := SCREENWIDTH` ⇒ 系数 1），再用 `g_DWinMan.Click` | `ClMain.pas:5765-5778`、`:1051` |
| C14 | 客户端上行帧格式 `#1<EDCode>!`（`SendSocket`），所以 c2s 解码要跳过 `#1`、去掉 `!` | `ClMain.pas:6614-6620` |

**客户端怪癖（登记，不改客户端）**：没有 `/` 的 `<...>`（本说词的 `<商铺>`、`<$GAMEGOLD>`）
也会被登记成命中矩形，`rstr=''`；且因为 `SelectMenuStr` 初值为 `''`，它们按 `SelectMenuStr = cmdparam`
判成**红色**（截图里 `商铺`/`$GAMEGOLD` 正是红色下划线，而 `@kaiz`/`@chouma`/`@huit` 是黄色）。
点击它们会发一条 **body 为空** 的 `1011`。

---

## 3. 链接命中矩形（截图实测）

坐标换算：窗口截图 1030×797 = 客户区 1024×768 + 左边框 3 + 右边框 3 + 标题栏 26 + 底边框 3。
对话框图片画在 `Surface(0,0)`，其左上角在截图上位于 **(3,26)** ⇒

```
client(x,y) = window(x,y) - (3,26)
```

实测方法：链接文本带下划线，`kaiz/chouma/huit` 为黄色、`商铺/$GAMEGOLD` 为红色；
下划线像素行的横向跨度即 `TextWidth(cmdstr)`，等于矩形宽。实测下划线行：y=105、153、185。

| 链接 | rstr | 截图中下划线 x | client 矩形 x | 所在行 k / ly | client 矩形 y | window 矩形 |
| --- | --- | --- | --- | --- | --- | --- |
| `开-始-下-注` | `@kaiz` | 46..110 | **43..107** | k=3 / 68 | **68..82** | 46..110 × 94..108 |
| `筹-码-兑-现` | `@chouma` | 172..236 | **169..233** | k=3 / 68 | **68..82** | 172..236 × 94..108 |
| `商铺` | `''` | 129..151 | 126..~154 | k=6 / 116 | 116..130 | 129..154 × 142..156 |
| `$GAMEGOLD` | `''` | 123..176 | 120..173 | k=8 / 148 | 148..162 | 123..176 × 174..188 |
| `返回土城` | `@huit` | 334..380 | **331..377** | k=8 / 148 | **148..162** | 334..380 × 174..188 |

9 个显示行（`k` / `ly` / 内容）：

```
k=0  ly=20   欢迎光临-天时地利人和-赌场-系统坐庄-童叟无欺
k=1  ly=36   (空格)
k=2  ly=52   (空格)
k=3  ly=68   ※<开-始-下-注/@kaiz>※         ※<筹-码-兑-现/@chouma>※
k=4  ly=84   (空格)
k=5  ly=100  (空格)
k=6  ly=116  注：下注前请先在<商铺>兑换[筹码]！
k=7  ly=132  (空格)
k=8  ly=148  您当前的元宝为:<$GAMEGOLD>个                           <返回土城/@huit>
```

> 夹具作者注意：`\ \ \` 会切出**两个只含空格的显示行**，所以链接不在视觉第 2 行对应的 `ly=36`，
> 而在 `ly=68`。按「打印行号 × 16」猜坐标会**系统性偏上 32 px**。

面板（图片）在截图上的范围约 x 3..437、y 26..200 ⇒ client 约 **434×174**。

---

## 4. 根因：两次点击都落在矩形外（fixture）

| 点击（录到的坐标） | 按 client 解释 | 与 `@kaiz` 矩形 (43..107 × 68..82) 的关系 | 结果 |
| --- | --- | --- | --- |
| `(105,97)` | x=105 ∈ 43..107 ✅；y=97 | **比下沿 82 低 15 px**，落在 k=4（ly=84）的空行 | 无链接 → 不发包 |
| `(150,100)` | x=150 | **落在两链接之间**（107 < 150 < 169）❌；y 低 18 px（k=5） | 无链接 → 不发包 |

两点都仍在面板内（434×174）⇒ 被对话框消费（C12），既没有 `1011`，也**没有** `CM_TURN`。

**为什么差在 y**：这两个数字像是直接从窗口截图读出来的（截图里 `开-始-下-注` 的黄字就在
y≈94..105）。如果它们被当作**客户区坐标**喂给点击工具（`mir_flow` 的 `Send-Click` 正是
客户区语义：`ClientToScreen` + `PostMessage` 客户区 lParam，`tools/client/mir_flow.ps1:1413-1429`），
就会整体下移 26 px（x 右移 3 px）⇒ 正好落到空的 k=4/k=5 行。

反过来验算：若按键面坐标解释，`(105,97)` 的真实 client 点是 `(102,71)`，**落在 `@kaiz` 矩形内**，
应当发出 `1011`。实机没有发 ⇒ **坐标被当成 client 坐标消费了**（或窗口不在 (0,0)、点击落到别处），
两种情形都归 **fixture**。

---

## 5. 与卡片现象描述的偏差：`CM_TURN`/`CM_PICKUP` 不在本次会话

r7 c2s 全量时间线（`capture.jsonl`，`#1<EDCode>!` 解码，ident 已用服务端日志交叉验证）：

| 相对时刻 | 方向 | ident | 含义 |
| --- | --- | --- | --- |
| +9.652 / +10.513 / +14.042 / +15.422 / +27.192 s | c2s | 3010 | `CM_TURN` ×5（进图/公告阶段） |
| +18.128 s | c2s | 1001 | 其它指令 |
| **+37.817 s** | c2s | **1010** | `CM_CLICKNPC`（Recog=119，与服务端日志一致） |
| **+37.929 s** | s2c | **643** | 本说词下发（对话框打开） |
| 之后 → 断线（+37.9 … +76 s） | — | — | **0 条 c2s** |

- 5 条 `3010` 全部在 `CLICKNPC` **之前**；对话框打开后没有任何 c2s 帧。
- 因此「点链接后被判成 `CM_TURN`/`CM_PICKUP`」**不是 r7 这次会话的事实**（若后续轮次复现，
  说明点击落到了对话框控件矩形之外/窗口未在前台，另案记录）。
- 对话框打开后的两次点击被**吞掉**（面板内、无链接）⇒ 0 帧，与 §4 的结论闭合。

服务端日志侧同证：`09:54:36.562 [INF] CM_CLICKNPC：到达 Click 点 npc=119`，其后直到
`09:55:13` 用户关闭，无 `Ident=1011`，也无新的 `3010`。

---

## 6. 重试坐标（可直接用于下一轮）

**首选：客户区坐标**（`mir_flow` 的 `Send-Click` 语义；工具内部会 `ClientToScreen`）：

| 目标 | 客户区坐标 | 依据 |
| --- | --- | --- |
| **开始-下注 `@kaiz`** | **(75, 75)** | 矩形 43..107 × 68..82 的中心 |
| 筹-码-兑-现 `@chouma` | (201, 75) | 矩形 169..233 × 68..82 |
| 返回土城 `@huit` | (354, 155) | 矩形 331..377 × 148..162 |

备用（若工具只吃屏幕/窗口坐标且窗口在 (0,0)，即截图坐标 = 客户区 + (3,26)）：
**(78, 101)** / (204, 101) / (357, 181)。

注意事项：

1. 点击前用 `SetCursorPos` 把**真实光标**移到目标点——`FormClick` 读的是 `GetCursorPos`
   （C13），只 `PostMessage` 不移动光标会点到光标当前所在处。
2. 命中一次后 500 ms 内客户端不响应（C11），连点请间隔 ≥ 0.6 s。
3. 不要用 `商铺` / `$GAMEGOLD` 做判据（rstr 为空，是客户端怪癖，见 §2 末）。
4. 通过判据：c2s capture 出现 `ident=1011` 且 body = `@kaiz`（`EncodeString` 编码后），
   服务端随后下发 `@kaiz` 段的 `SM_MERCHANTSAY(643)`。

---

## 7. 修复建议（不改代码，仅列出改动点）

1. **夹具（必改）**：截图坐标 → 客户区坐标必须减客户区原点 `(3,26)`；或干脆用
   `GetWindowRect`/`ClientToScreen` 反算，别把窗口截图坐标直接喂给客户区语义的点击函数。
   本轮 `(105,97)` / `(150,100)` 就是把截图坐标当客户区坐标用的典型。
2. **更稳的取点**：从下划线像素（黄/红）自动求矩形中心，而不是靠肉眼估；
   或在抓包侧加「点击后 800 ms 内没有 `1011` ⇒ FAIL 并附截图」的断言，避免"以为点了"。
3. **可复现性**：把客户端窗口位置与客户区原点写进 `flow/manifest.json`（r7 没记录，
   导致只能靠截图反推坐标空间）。
4. **服务端**：说词路径无需改（字节正确）；`$GAMEGOLD` 替换按原卡处理，
   替换后**必须保留 `<...>` 标记原形**（元宝值是数字，不会引入 `<>`）。
5. **登记客户端怪癖**：无 `/` 的 `<...>` 会变成 `rstr=''` 的可点矩形并显示红色；
   建议补一条构造级用例锁「服务端收到空 body 的 `1011` 时的行为」（不改客户端，只锁行为）。
6. 若下一轮仍无 `1011`：先看点击是否被对话框吞（0 帧 = 吞掉/落在面板内），
   再看是否落到世界（有 `3010`/`1004` = 点在面板外）。两者指向不同的夹具修正。

---

## 8. 复现命令

解码器（EDCode 的 Python 直译，与 `mir2-rs/crates/protocol/src/edcode.rs` 一致）：

```python
SEED, BASE = 0xAC, 0x3C
def decode(src):
    n, dst = len(src), []
    for i in range(n // 4):
        b, rem = i * 4, (src[i * 4 + 3] - BASE) & 0xFF
        t = (src[b] - BASE) & 0xFF
        dst.append((((t << 2) & 0xF0) | (rem & 0x0C) | (t & 0x3)) ^ SEED)
        t = (src[b + 1] - BASE) & 0xFF
        dst.append((((t << 2) & 0xF0) | ((rem << 2) & 0x0C) | (t & 0x3)) ^ SEED)
        t = (src[b + 2] - BASE) & 0xFF
        dst.append((t | ((rem << 2) & 0xC0)) ^ SEED)
    if n % 4 == 2:
        rem, t = (src[-1] - BASE) & 0xFF, (src[-2] - BASE) & 0xFF
        dst.append((((t << 2) & 0xF0) | ((rem << 2) & 0x0C) | (t & 0x3)) ^ SEED)
    elif n % 4 == 3:
        rem, t = (src[-1] - BASE) & 0xFF, (src[-3] - BASE) & 0xFF
        dst.append((((t << 2) & 0xF0) | (rem & 0x0C) | (t & 0x3)) ^ SEED)
        t = (src[-2] - BASE) & 0xFF
        dst.append((((t << 2) & 0xF0) | ((rem << 2) & 0x0C) | (t & 0x3)) ^ SEED)
    return bytes(dst)

# s2c 帧：raw = bytes.fromhex(rec["raw_hex"]); payload = raw[1:-1]（去 '#' 与 '!'）
#         头 = decode(payload[:16])；体 = decode(payload[16:])
# c2s 帧：raw[0]=='#'、raw[1]=='1'；payload = raw[2:-1]
```

复现步骤：

```sh
# 1) 取 643 帧并比对脚本（脚本见 §1.2）
#    capture: E:/tmp/window/r7/s2c/capture.jsonl  seq=54
# 2) 量下划线：shot-dice5.png 中黄/红下划线行 y=105/153/185，跨度即矩形宽
# 3) 客户区原点：面板左上角 (3,26)；client = window - (3,26)
```

---

## 9. 不确定点

1. **点击工具的坐标空间没有留痕**：r7 的点击是手工/外部工具完成，未记录窗口位置。本文用
   「没有 `1011` + 面板内点击被吞（0 帧）」反推为**客户区坐标**；若当时用屏幕坐标且窗口不在
   (0,0)，结论仍是 fixture，只是偏移量不同。
2. 截图量到的客户区原点 `(3,26)` 比 `tools/client/mir_flow.ps1:1412` 注释里的 `origin(3,29)`
   少 3 px（该注释无实测出处）。不影响结论：y 差 15 px 远大于 3 px 歧义。
3. 对话框图片精确尺寸是量图得到的 ≈434×174，不是图库常量；只用于「点击仍在面板内」的判断。
4. 客户端源码与实机 exe 的对应未钉 commit（`manifest.json` 只有 exe sha256 `c3cb8f73…`）；
   但红/黄两种链接颜色与源码 `SelectMenuStr = cmdparam` 分支完全吻合，行为口径可用。
5. 没有 C# 侧 `643` 真机抓包，oracle 字节判据来自源码路径 + 脚本字节，不是真机 C# 对照。

---

## 10. 残留物清单

- 本文档：`docs/dice-link-diagnosis-20261022.md`（本分支）。
- worktree：`E:/tmp/wt-dicelink`（分支 `dice-link-diagnosis`，未 merge、未 push）。
- 临时脚本/裁图：`E:/tmp/dicework/`（`eddec.py` 解码器、`crop-npc.png`、`dlg-zoom.png`、
  `t1.txt`/`t2.txt` 转义探针）——保留供复核，可随时删除。
- 未起任何服务端/客户端/代理，未占端口，未写数据库，未 push。
