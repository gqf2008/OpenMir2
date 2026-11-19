# quest-parity 合成夹具

**为什么是合成**：`{Quest` 头在现网语料里**零命中**——`E:/MirServer` 全量 `.txt` 扫描 0 命中
（命令、rc 与扫描数见卡面 `docs/issues/openmir2-quests-parity.md` §10 的 `corpus_quest_headers.txt`）。
所以本夹具按 oracle 语法手写，只用来证明「脚本装载成功，但 `ScriptInfo.IsQuest` 恒 false」。

**出处**：作者（principal `openmir2-ca`）按 `src/Modules/ScriptEngine/ScriptParsers.cs` 的 `{Quest`
解析语法手写，**不来自任何现网脚本**。

**用法**：探针 `quest-parity-probe` 把它们拷进自己的 `bin/Envir/QuestProbe/`，再用**真**
`ScriptParsers.LoadScriptFile` 装载（不注入、不替换解析器）。

| 文件 | 覆盖 |
| --- | --- |
| `quest_probe_a.txt` | `{Quest 1}` + 两个标签（`@main` / `@other`），证「装载成功 + 已知标签命中」 |
| `quest_probe_b.txt` | `{Quest 2}` + `#IF` / `#ACT` 行，证条件/动作行在死分支下仍按普通记录解析 |
| `quest_abort_good_a.txt` | 中止范围（§10.7 ①）前缀：坏脚本**之前**的合法脚本 ⇒ 必须装载 |
| `quest_abort_bad.txt` | 中止范围（§10.7 ①）坏脚本：`SET [5] " "` ⇒ oracle `CaptureString` 越界（`IndexOutOfRangeException`），整批中止 |
| `quest_abort_good_b.txt` | 中止范围（§10.7 ①）后缀：坏脚本**之后**的合法脚本 ⇒ **不得**装载（证「中止」而非「跳过」） |

校验和见 `SHA256SUMS.txt`（仓库字节口径，LF）。
