# openmir2-gate-utf8-body-length

> 状态：**已登记，未建卡**（本仓库 remote 无 `refs/collab/*`，walgit 看板不可用；按 AGENTS.md §7 过渡）。
> 发现于卡 `openmir2-db-write-stall` 的 44 处 `MemoryCopy.BlockCopy` 逐条审计（C 类）。

## 现象
C# `GameGate` 的 `SendServerMsg`：正文按**字符数**而不是**字节数**处理——
```
byte[] strBuff = HUtil32.GetBytes(data);                       // 编码后的字节
byte[] tempBuff = new byte[ServerMessage.PacketSize + data.Length];  // 按字符数开缓冲
MemoryCopy.BlockCopy(strBuff, 0, tempBuff, sendBuffer.Length, data.Length);  // 拷贝 data.Length 字节
```

## 影响
`HUtil32.GetBytes` 为非 ASCII 内容产出多于字符数的字节（中文典型 2 字节/字符）⇒
**正文被截断**（拷贝量只有实际字节数的一部分）。这是功能缺陷，不是性能问题。

## 双重疑点（同卡一并核）
同一函数里 `ServerMessage.PackLength = nLen` 由**调用方**传入；需要核 `nLen` 与
`tempBuff` 实际长度、与 `GetBytes(data).Length` 三者是否一致。若不一致，即便拷贝量修好，
包头声明的长度仍会与实际正文不符，客户端侧会再次出问题。

## 建议改动（两侧同卡，R 级 + 窗口对拍）
1. 先用**中文样本**把现状固定成红/绿记录（C# 现有行为 + Rust 现状），避免"改完不知道原来是什么"。
2. C#：缓冲与拷贝长度一律取 `strBuff.Length`；`PackLength` 与实际正文长度对齐。
3. Rust `game-gate`：C# 是 oracle。两侧同改，或登记 `D-*` 偏差（只改 C# 会让对拍红）。
4. 窗口对拍：中文角色名/中文聊天样本的逐字节对拍（R 级证据）。

## 关联
- 根因卡：`openmir2-db-write-stall`
- 同批审计：`openmir2-gate-packlen-validation`
