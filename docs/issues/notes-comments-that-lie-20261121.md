# 注释如何骗人：从 MG-55 的两处产品差看"不跑生成器也能怀疑"的清单

> 2026-11-21，MG-55（46 分身术）实现中撞出两处产品差。两处都有一句**看起来权威、实际错误**的
> 注释/断言把错误行为固定了下来。本文只提炼**可操作的识破方法**，不复述缺陷本身
> （卡面：`docs/issues/magic-mgs-34-36-55-batch-impl.md` §6.7/§6.8）。

## 1. 两句话，两种骗法

**骗法 A：把"这个文件里没有"写成"构造过程里没有"**
`crates/world-bevy/src/slave.rs` 的 `spawn_clone` 文档注释曾写：

> **不抽随机数**（oracle 构造器无 `Random`，RNG 账本不变）。

同文件还有一颗测试把它钉死：

```rust
assert_eq!(w.rng.random(), fresh.random(), "构造克隆体不消费 RNG");
```

事实：oracle `new PlayCloneObject` 走
`PlayObject → CharacterObject → AnimalObject → BaseObject` 的 ctor 链，**每一层基类 ctor 都抽数**：

| 层 | 抽数 | 来源 |
| --- | --- | --- |
| `BaseObject` | `Random(1500)`、`Random(2000)` | `RunTick` / `SearchTime` |
| `AnimalObject` | `Random(3000)` ×2 | `AttackTick` / `WalkTick` |
| `PlayObject` | `Random(999999)` | `RandomNo` |

`PlayCloneObject.cs` 自己的 ctor 确实一行 `Random` 都没有——**注释在它读过的那个文件里是真的**，
在"构造过程"这个范围里是假的。

**骗法 B：把"代码路径存在"写成"可观测效果发生"**
`bins/game-svr/src/lib.rs` 的 46 支曾按 `PlayCloneObject.cs:17` 的
`SendRefMsg(RM_TURN, …)` 直接广播 `turn_pair`。事实：`PlayObject` ctor 把
`FixedHideMode` 置 true（`src/M2Server/Player/PlayObject.cs:993`），克隆体不登录、无
`PlayObject.cs:1442` 的复位，`SendRefMsg` 命中 `BaseObject.Message.cs:390-394` 的
"隐藏态只发自己"分支 ⇒ **旁观者零帧**。路径存在 ≠ 帧存在，中间隔着一个**类级默认状态**。

## 2. 为什么几个月没人发现

三条防线各自失效，且失效方式互补：

1. **golden 其实记录了**：clone 的 `Rng` 是 6 个值（5 个 ctor + 1 个训练尾），不是 0；
2. **驱动没消费、比较器没比**：skeleton/angel 先例里写了"造怪抽数不可复刻 ⇒ `Rng` 不在对拍范围"，
   clone 继承了这句口径；
3. **唯一的单测是 Rust 跟 Rust 比**：`w.rng.random()` vs `fresh.random()`（`fresh` 是构造前克隆）。
   任何"Rust 决定不抽"的实现都能让它绿——它保护的是实现约定，不是 oracle 行为。

⇒ 错误不是"没人测"，是**测的东西对 oracle 没有约束力**，而注释让这个空白看起来是故意的。

## 3. 不跑生成器也能怀疑的清单

遇到"oracle 没有 X / 不消费 X / 无影响"类断言，逐条问：

1. **构造类断言先走基类链**。`grep -n "Random" ` 不能只 grep 当前类：把
   `PlayObject/CharacterObject/AnimalObject/BaseObject`（或对应语言的基类）各 grep 一遍，
   看 ctor 体（不是整个文件）。"本文件无"必须被读成"本文件无"。
2. **要求范围，而不是结论**。可证伪的写法是"`PlayCloneObject` ctor 体（`:5-23`）无 `Random`；
   基类链消耗见 X"；不可证伪的写法是"oracle 构造器无 `Random`"。
3. **自比测试是红旗**。问一句："这条断言能让哪个 oracle 观测变红？"答不上来就不是对拍照，
   是内部一致性测试——可以留，但不许当"已对拍"引用。
4. **"记录但不比较"的列是未爆的 bug**。`Rng` 有 6 个值、文档却说"不入对拍"，这本身就是矛盾：
   值若真是垃圾，为何记录？值若不是垃圾，驱动为何不消费？两问必有一问答错。
5. **"不可复刻"是强断言，需要失败日志**。可复现的定义是"同 seed + 同路径 ⇒ 同序列"。本案例
   用 .NET `Random(1)` 按 `[1500,2000,3000,3000,2000] + Random(3)` 重放，与 golden 逐值一致——
   写"不可复刻"之前，至少做一次这样的重放。
6. **数一数抽数差**。被测行为若只剩训练尾 1 抽，而 golden 是 6，差值 5 恰好等于基类 ctor 的
   抽数（2+2+1）——这类"整差"是构造期遗漏的典型指纹。
7. **"效果"类断言查中间门**。`SendRefMsg`/`SendMsg` 之前通常有 `Ghost` / `ObMode` /
   `FixedHideMode` / 视野范围等门；这些门的状态可能由**类 ctor 的默认值**决定，而不在你看的
   那个函数里。看到"会发帧"就应先查发送函数的前置分支。

## 4. 写注释的最低要求（本案例的教训）

- 断言带**范围**：文件、函数、行段，三者至少给一个；
- 断言带**可观测反例**：哪一列 golden / 哪条测试会在它是错的时候变红；
- 未验证的就写"未验"，不要写成事实；基类/调用链参与时，把链列出来；
- 当你要写"没有/不影响/不变"时，先写下**你搜过什么**——这句话是给下一个人复核用的，不是给自己
  免责用的。

## 5. 一句话

> **最容易骗人的注释，不是写错的常量，而是替一个"空观测"给出合理解释的那句。**
> 它让"没看到"变成"不需要看到"，于是下一个人连怀疑的入口都没有。
