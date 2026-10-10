# 门禁红检记录（2026-10-10）

每条红检都给出「故意改坏什么 → 观察到什么红 → 如何恢复」。恒绿的门禁等于没有门禁（§6.0-2）。

## 红检 1：码表漂移门禁（`cargo test -p mir2-script-tool --test codegen_drift`）

```
$ printf '\n// tampered\n' >> mir2-rs/crates/script/src/codes.rs
$ cargo test -p mir2-script-tool --test codegen_drift
test codes_rs_matches_csharp_source ... FAILED
codes.rs 与 C# 参照源码不一致；重跑: cargo run -p mir2-script-tool -- gen-codes
test result: FAILED. 1 passed; 1 failed
$ cp 备份恢复 → test result: ok. 2 passed; 0 failed
```

## 红检 2：未知命令必须显式报错（双侧同红）

夹具 `target/redcheck/Envir/Npc_def/bad.txt` 写入 `TAMPEREDCMD 1 2`：

```
Rust: 解析错误: 1  →  [ScriptError] Npc_def/bad.txt 行4: TAMPEREDCMD 1 2
C#  : 解析错误: 1 加载失败: 0 文件未找到: 0 其他错误: 0
两侧其余计数相同（脚本 2 标签 2 过程 2 条件 2 动作 1）
```

## 红检 3：对拍工具本身能变红

篡改 `parse-stats-rust.json` 的 `stats.conditions` +1：

```
DIFF stats.conditions: rust=2138 cs=2137
== 1 项差异 ==
```

## 红检 4：结构摘要门禁对「计数不变、内容变」敏感

夹具 `target/redcheck2/Envir/Npc_def/a.txt`，把动作 `give 金币 5` 改成 `give 金币 6`：

```
基线  : {'hash': '552f35d7456ac7a2', 's': 1, 'r': 1, 'p': 1, 'c': 1, 'a': 1, 'e': 0}
篡改后: {'hash': 'c790f9f298851c2a', 's': 1, 'r': 1, 'p': 1, 'c': 1, 'a': 1, 'e': 0}
计数相同: True
摘要变化: True
```

旧门禁（只比计数 + 错误行文本）对该篡改**不敏感**，新结构摘要门禁变红。

## 红检 5：结构摘要门禁能看见命令码位移（whitelist B-8）

临时把 `parser.rs` 两处 `n_cmd_code = def.field_index - 1;` 改成 `= def.field_index;`（即“修正”位移）：

```
$ node mir2-rs/tools/diff-parity.js target/redcheck-shift.json target/parse-stats-cs.json
DIFF 结构摘要不一致: 115 个文件
  MapQuest_def/Q001.txt: rust={"hash":"da96bea7990da3ed","s":1,"r":1,"p":1,"c":0,"a":2,"e":0}
                         cs=  {"hash":"c8c14e0a8b116e18","s":1,"r":1,"p":1,"c":0,"a":2,"e":0}
  …（115 个文件，计数全部相同，仅指纹不同）
== 1 项差异 ==
```

恢复备份后重跑：`== 全部一致 ==`。这条红检同时证明位移是**普遍存在**的（115/639 文件的可观测命令码受影响）。

## 红检 6：结构摘要门禁上线即抓到真实内容差异

对拍升级后第一次全量跑，原文摘要门禁报出 `GuildRankNameFilter.txt` 两侧不同：
该文件含 GBK `A8 BF`，.NET cp936 映射为 U+E7C8，encoding_rs(WHATWG GBK) 解成 U+FFFD。
该文件内容不进入脚本结构（不含 `[`/`#` 行），旧的计数门禁与结构摘要都看不见它。
按 .NET 探测表生成 cp936 覆盖表后：`== 全部一致 ==`（639/639 原文指纹相同）。

## 红检 7：GB2312 解码穷举门禁（32256 个字节对）

```
$ cargo test -p mir2-script --test gbk_decode
test all_cp936_pairs_match_dotnet ... ok      # 0x81..=0xFE × 0x00..=0xFF 全覆盖（排除 FE FF BOM）
test single_byte_edges_match_dotnet ... ok    # 0x80→U+20AC、0xFF→U+F8F5、孤立引导字节→'?'
```
改动 `gbk_overrides.rs` 任一映射或 `textfile.rs` 的状态机 → 必红。

## 红检 8：`{Quest` 段落后的内容归属（对拍实测差异 → 修复）

夹具 `{Quest 2}` 之后紧跟动作行：C# 把该行写入**上一个记录**（SayingRecord 与 Script 相互独立），
Rust 初版丢弃 ⇒ `动作 2 vs 1`。修正为"记录坐标 (script, record) 与当前脚本解耦"后：
`脚本 2 / 标签 2 / 过程 2 / 条件 1 / 动作 2` 两侧一致，并加回归用例
`quest_block_keeps_writing_into_previous_record`。

## 红检 9：BOM 模型（实测推翻直觉）

用真实 `StringList.LoadFromFile` 实测：`FF FE FF 41` 得到 `U+41FF` ——
`StreamReader` 的 BOM 检测**优先于** `GetEncoding` 的结果，`FF FE` 一律按 UTF-16LE
（`FF FE 00 00` 亦然，实测首字符 U+0000），`00 00 FE FF` 才走 UTF-32BE。
Rust 侧据此重写（此前按 `byte3 != 0xFF` 守卫理解，是错的），并加 `bom_detection_matches_dotnet` 用例。

## 红检 10：回退字符按分支区分（F-R5）

用真实 `StringList.LoadFromFile` 实测（探针 `tools/gbk-probe bom` 可复现）：

```
EF BB BF FF 41        → U+FFFD U+0041      （UTF-8 分支保留 U+FFFD）
EF BB BF ED A0 80     → U+FFFD ×3
FF FE 00 D8 41 00     → U+FFFD U+0041      （UTF-16LE 孤立高代理）
81 20 41（gb2312）     → U+003F U+0041      （cp936 成对消耗 → '?'）
```

初版把 `U+FFFD → '?'` 做成全局替换，会在这三个 BOM 分支给出 '?'（与 C# 不符）。
已把替换下沉到 `decode_gb2312` 内部，并由 `invalid_sequence_fallback_per_branch_matches_dotnet` 钉住。
