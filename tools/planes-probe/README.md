# planes-probe —— G11 跨服（Planes）事实探针

`docs/g11-planes-design-20261108.md` 里 §5.1（槽位语义）与 §5.2（主服是否监听）的结论
来自这里。入仓是为了**可复现**：临时目录会被后续实验覆盖，事实不能只剩口述。

依赖：.NET 9 SDK；`TouchSocket 2.0.0-beta.273`（与 `src/OpenMir2/OpenMir2.csproj` 同版本）。

```sh
dotnet run -c Release -- ids      # SocketClient.Id 的分配语义
dotnet run -c Release -- slots    # 复刻 oracle srvArray 逻辑，看断线重连后数据去哪
dotnet run -c Release -- listen   # 只 Setup vs Setup+Start（oracle 属于前者）
dotnet run -c Release -- cross    # 跨槽：id-1 指向别人的槽时数据被追加
```

## 期望输出（2026-11-08 实测）

`ids`：`id=1..5` 依次递增，断开后**不复用**。

`slots`：
- 首连 `CONNECTED id=1 -> slot0`，`RECV id=1 -> slot0`；
- 断开后 `DISCONNECTED id=1 -> cleared slot0`；
- 后续连接 `CONNECTED id=2 -> slot0`，但 `RECV id=2 THREW NullReferenceException`
  ——`Received` 用 `Id-1` 当槽下标，slot1 是空的。

`cross`：连 A(id1→slot0)、B(id2→slot1)、C(id3→slot2) → 断 A → 连 D(id4→slot0)、E(id5→slot3)
→ D 发送 ⇒ `RECV id=4 -> slot3 owner=5 SOC=(1/2/from4)`——**数据进了别人的缓冲**。

`listen`：
- 只 `Setup` ⇒ `ServerState=None`，客户端 `ConnectAsync` 抛 `TimeoutException`；
- 再 `StartAsync()` ⇒ `ServerState=Running`，客户端连上并触发 `Connected`。

⇒ oracle `PlanesServer.StartPlanesServer()` 只 `Setup`、没有 `Start`，**主服从不监听**。
