# oracle 部署件 ↔ 源码构建输出 对账

- 生成时间：2026-10-10 16:41:58
- ServerRoot：`E:\MirServer`；RepoRoot：`E:\Users\gxh\Documents\GitHub\OpenMir2`
- 命令：`powershell -ExecutionPolicy Bypass -File tools/oracle/reconcile_deployment.ps1`（红检 `-SelfTestRed`）

| 组件 | 结论 | 依据 |
| --- | --- | --- |
| DBSrv | **SELF-HOSTED** | 按 start-all.ps1 直接从 E:\Users\gxh\Documents\GitHub\OpenMir2\src\DBSrv\bin\Release 启动，无独立部署副本 |
| LoginSrv | **SELF-HOSTED** | 按 start-all.ps1 直接从 E:\Users\gxh\Documents\GitHub\OpenMir2\src\LoginSrv\bin\Release 启动，无独立部署副本 |
| GameSvr | **MIXED** | 有产物与源码构建输出不一致（不是同一份构建）: CommandSystem.dll(DRIFT), GameSrv.exe(DRIFT), OpenMir2.dll(DRIFT), PlanesSystem.dll(DRIFT), ScriptSystem.dll(DRIFT), SystemModule.dll(DRIFT) |
| LoginGate | **SINGLE-BUILD** | 受管产物全部与源码构建输出逐字节一致 |
| SelGate | **SINGLE-BUILD** | 受管产物全部与源码构建输出逐字节一致 |
| RunGate | **SINGLE-BUILD** | 受管产物全部与源码构建输出逐字节一致 |

## DBSrv — SELF-HOSTED

| 产物 | 判定 | 宿主 | 部署件 sha256[0:16] | 源码 sha256[0:16] | 部署时间 | 源码构建时间 |
| --- | --- | --- | --- | --- | --- | --- |

## LoginSrv — SELF-HOSTED

| 产物 | 判定 | 宿主 | 部署件 sha256[0:16] | 源码 sha256[0:16] | 部署时间 | 源码构建时间 |
| --- | --- | --- | --- | --- | --- | --- |

## GameSvr — MIXED

| 产物 | 判定 | 宿主 | 部署件 sha256[0:16] | 源码 sha256[0:16] | 部署时间 | 源码构建时间 |
| --- | --- | --- | --- | --- | --- | --- |
| Collections.Pooled.dll | MATCH | PE | DEFD808D64E1D920 | DEFD808D64E1D920 | 2019-11-28 12:26:48 | 2019-11-28 12:26:48 |
| CommandSystem.dll | DRIFT | PE | 044051CEBFBFD099 | 70347B38655D4F3C | 2026-10-09 01:18:07 | 2026-10-10 00:10:35 |
| GameSrv.dll | MATCH | PE | 0FF74469DF089F70 | 0FF74469DF089F70 | 2026-10-10 00:10:36 | 2026-10-10 00:10:36 |
| GameSrv.exe | DRIFT | PE | 27B415C9E9F5EBF5 | 5B34FE6713B106DE | 2026-10-09 07:08:29 | 2026-10-10 00:10:36 |
| IPLocal.dll | EXTRA | PE | DA61A728A96293D8 |  | 2026-10-09 01:32:29 |  |
| M2Server.dll | MATCH | PE | EE7A6055F5559BF6 | EE7A6055F5559BF6 | 2026-10-10 00:10:36 | 2026-10-10 00:10:36 |
| McMaster.Extensions.CommandLineUtils.dll | MATCH | PE | E2EDE6013683588C | E2EDE6013683588C | 2023-08-26 13:10:18 | 2023-08-26 13:10:18 |
| MediatR.Contracts.dll | MATCH | PE | C2601304AA9D15C5 | C2601304AA9D15C5 | 2023-02-23 02:48:20 | 2023-02-23 02:48:20 |
| MediatR.dll | MATCH | PE | EDF305947E2D0F89 | EDF305947E2D0F89 | 2023-11-18 00:23:20 | 2023-11-18 00:23:20 |
| MemoryPack.Core.dll | MATCH | PE | 101F0CFBF844D027 | 101F0CFBF844D027 | 2023-11-09 15:19:38 | 2023-11-09 15:19:38 |
| Microsoft.Extensions.Configuration.Abstractions.dll | MATCH | PE | FEFCDB267A73099C | FEFCDB267A73099C | 2023-10-31 22:59:08 | 2023-10-31 22:59:08 |
| Microsoft.Extensions.Configuration.Binder.dll | MATCH | PE | 5A47750F5C8EB91A | 5A47750F5C8EB91A | 2023-10-31 22:59:26 | 2023-10-31 22:59:26 |
| Microsoft.Extensions.Configuration.CommandLine.dll | MATCH | PE | 688EF791D4BB2E84 | 688EF791D4BB2E84 | 2023-10-31 22:59:28 | 2023-10-31 22:59:28 |
| Microsoft.Extensions.Configuration.dll | MATCH | PE | B5CD9DA2C3364A5B | B5CD9DA2C3364A5B | 2023-10-31 22:59:20 | 2023-10-31 22:59:20 |
| Microsoft.Extensions.Configuration.EnvironmentVariables.dll | MATCH | PE | 797B3034FD173CAF | 797B3034FD173CAF | 2023-10-31 22:59:28 | 2023-10-31 22:59:28 |
| Microsoft.Extensions.Configuration.FileExtensions.dll | MATCH | PE | D90FD867B38C1346 | D90FD867B38C1346 | 2023-10-31 22:59:28 | 2023-10-31 22:59:28 |
| Microsoft.Extensions.Configuration.Json.dll | MATCH | PE | 96920CAA680F7583 | 96920CAA680F7583 | 2023-10-31 22:59:36 | 2023-10-31 22:59:36 |
| Microsoft.Extensions.Configuration.UserSecrets.dll | MATCH | PE | A415ECB7EBDF4F65 | A415ECB7EBDF4F65 | 2023-10-31 22:59:42 | 2023-10-31 22:59:42 |
| Microsoft.Extensions.DependencyInjection.Abstractions.dll | MATCH | PE | A0106A638FACAE62 | A0106A638FACAE62 | 2023-10-31 22:58:56 | 2023-10-31 22:58:56 |
| Microsoft.Extensions.DependencyInjection.dll | MATCH | PE | 8D6E7B64CCE554F0 | 8D6E7B64CCE554F0 | 2023-10-31 22:59:00 | 2023-10-31 22:59:00 |
| Microsoft.Extensions.DependencyModel.dll | MATCH | PE | 73E69DA2A4462DCD | 73E69DA2A4462DCD | 2023-10-31 22:59:24 | 2023-10-31 22:59:24 |
| Microsoft.Extensions.Diagnostics.Abstractions.dll | MATCH | PE | BEA3CF225599B05A | BEA3CF225599B05A | 2023-10-31 22:59:22 | 2023-10-31 22:59:22 |
| Microsoft.Extensions.Diagnostics.dll | MATCH | PE | 5C83650F1C0A15C8 | 5C83650F1C0A15C8 | 2023-10-31 22:59:42 | 2023-10-31 22:59:42 |
| Microsoft.Extensions.FileProviders.Abstractions.dll | MATCH | PE | F08B1F597ABB3647 | F08B1F597ABB3647 | 2023-10-31 22:59:04 | 2023-10-31 22:59:04 |
| Microsoft.Extensions.FileProviders.Physical.dll | MATCH | PE | 9F67076C79A953F5 | 9F67076C79A953F5 | 2023-10-31 22:59:20 | 2023-10-31 22:59:20 |
| Microsoft.Extensions.FileSystemGlobbing.dll | MATCH | PE | ED0855B522F09B5A | ED0855B522F09B5A | 2023-10-31 22:58:56 | 2023-10-31 22:58:56 |
| Microsoft.Extensions.Hosting.Abstractions.dll | MATCH | PE | 1765C6147ED43B7C | 1765C6147ED43B7C | 2023-10-31 22:59:34 | 2023-10-31 22:59:34 |
| Microsoft.Extensions.Hosting.dll | MATCH | PE | 746B030485BCF78D | 746B030485BCF78D | 2023-10-31 22:59:44 | 2023-10-31 22:59:44 |
| Microsoft.Extensions.Logging.Abstractions.dll | MATCH | PE | 29E6BFAF5CE079AD | 29E6BFAF5CE079AD | 2023-10-31 22:59:12 | 2023-10-31 22:59:12 |
| Microsoft.Extensions.Logging.Configuration.dll | MATCH | PE | 4BD1245656BEF4EA | 4BD1245656BEF4EA | 2023-10-31 22:59:42 | 2023-10-31 22:59:42 |
| Microsoft.Extensions.Logging.Console.dll | MATCH | PE | B6836602E2139D6E | B6836602E2139D6E | 2023-10-31 22:59:44 | 2023-10-31 22:59:44 |
| Microsoft.Extensions.Logging.Debug.dll | MATCH | PE | 798B0714E3DFE8E8 | 798B0714E3DFE8E8 | 2023-10-31 22:59:30 | 2023-10-31 22:59:30 |
| Microsoft.Extensions.Logging.dll | MATCH | PE | 4330A5EFE9D110AF | 4330A5EFE9D110AF | 2023-10-31 22:59:24 | 2023-10-31 22:59:24 |
| Microsoft.Extensions.Logging.EventLog.dll | MATCH | PE | 1182928ECAA6AB23 | 1182928ECAA6AB23 | 2023-10-31 22:59:30 | 2023-10-31 22:59:30 |
| Microsoft.Extensions.Logging.EventSource.dll | MATCH | PE | E8CDE7AD083DC062 | E8CDE7AD083DC062 | 2023-10-31 22:59:30 | 2023-10-31 22:59:30 |
| Microsoft.Extensions.Options.ConfigurationExtensions.dll | MATCH | PE | B9EA7BB70BFECF5C | B9EA7BB70BFECF5C | 2023-10-31 22:59:38 | 2023-10-31 22:59:38 |
| Microsoft.Extensions.Options.dll | MATCH | PE | 5F9B0E589F1CE9CA | 5F9B0E589F1CE9CA | 2023-10-31 22:59:06 | 2023-10-31 22:59:06 |
| Microsoft.Extensions.Primitives.dll | MATCH | PE | 446FF16E903E7479 | 446FF16E903E7479 | 2023-10-31 22:58:52 | 2023-10-31 22:58:52 |
| MySqlConnector.dll | MATCH | PE | 84D297E722DB70C7 | 84D297E722DB70C7 | 2024-01-20 20:54:02 | 2024-01-20 20:54:02 |
| Newtonsoft.Json.dll | MATCH | PE | 22C649F75FCE5BE7 | 22C649F75FCE5BE7 | 2023-03-08 15:09:54 | 2023-03-08 15:09:54 |
| NLog.dll | MATCH | PE | F8FDD998B9846C4A | F8FDD998B9846C4A | 2023-12-30 02:17:32 | 2023-12-30 02:17:32 |
| NLog.Extensions.Logging.dll | MATCH | PE | 77E8675AD8E053F8 | 77E8675AD8E053F8 | 2023-12-30 04:01:22 | 2023-12-30 04:01:22 |
| OpenMir2.dll | DRIFT | PE | 27D7B4631D6540A0 | C068F4E0C0CA7200 | 2026-10-09 21:53:24 | 2026-10-10 00:10:34 |
| PlanesSystem.dll | DRIFT | PE | 0F8189E0DE0A891C | AC6F772439D18090 | 2026-10-09 01:18:07 | 2026-10-10 00:10:35 |
| ScriptSystem.dll | DRIFT | PE | CC4AEBD8F277706A | 7733576C79615C6E | 2026-10-09 01:18:07 | 2026-10-10 00:10:35 |
| Serilog.dll | MATCH | PE | 6B67FB3F10451303 | 6B67FB3F10451303 | 2023-11-10 20:41:54 | 2023-11-10 20:41:54 |
| Serilog.Extensions.Logging.dll | MATCH | PE | A0B6C860FA5BDB3F | A0B6C860FA5BDB3F | 2023-11-15 06:47:46 | 2023-11-15 06:47:46 |
| Serilog.Settings.Configuration.dll | MATCH | PE | 1159398C7EA29D8D | 1159398C7EA29D8D | 2023-11-15 09:17:44 | 2023-11-15 09:17:44 |
| Serilog.Sinks.Console.dll | MATCH | PE | 30B79981EF4EBD97 | 30B79981EF4EBD97 | 2023-11-29 04:32:02 | 2023-11-29 04:32:02 |
| Spectre.Console.dll | MATCH | PE | 6D52BF7256858F97 | 6D52BF7256858F97 | 2024-02-01 00:48:20 | 2024-02-01 00:48:20 |
| System.Configuration.ConfigurationManager.dll | MATCH | PE | 89D52B4A10B13BF9 | 89D52B4A10B13BF9 | 2023-10-31 22:59:08 | 2023-10-31 22:59:08 |
| System.Diagnostics.EventLog.dll | MATCH | PE | 61C0B16A1D6091B4 | 61C0B16A1D6091B4 | 2023-10-31 22:59:02 | 2023-10-31 22:59:02 |
| System.Diagnostics.PerformanceCounter.dll | MATCH | PE | C596C35317C7BD5A | C596C35317C7BD5A | 2023-10-31 23:05:38 | 2023-10-31 23:05:38 |
| System.Security.Cryptography.ProtectedData.dll | MATCH | PE | 5E04D6CFF3B6FE97 | 5E04D6CFF3B6FE97 | 2023-10-31 22:59:04 | 2023-10-31 22:59:04 |
| SystemModule.dll | DRIFT | PE | 0B93BFF54F0829D3 | E942BA3DAB51BBEA | 2026-10-09 01:18:06 | 2026-10-10 00:10:34 |
| TouchSocket.Core.dll | MATCH | PE | C5886A30644ECD6A | C5886A30644ECD6A | 2024-02-05 13:10:36 | 2024-02-05 13:10:36 |
| TouchSocket.dll | MATCH | PE | C37180F134F588C2 | C37180F134F588C2 | 2024-02-05 13:10:40 | 2024-02-05 13:10:40 |

## LoginGate — SINGLE-BUILD

| 产物 | 判定 | 宿主 | 部署件 sha256[0:16] | 源码 sha256[0:16] | 部署时间 | 源码构建时间 |
| --- | --- | --- | --- | --- | --- | --- |
| LoginGate | EXTRA | MACHO | E8A8B4B40C7E5A55 |  | 2026-10-09 01:32:29 |  |
| LoginGate.dll | MATCH | PE | B83DB6B8553F9D04 | B83DB6B8553F9D04 | 2026-10-09 01:18:05 | 2026-10-09 01:18:05 |
| LoginGate.exe | MATCH | PE | 8A261CAE8DC650A0 | 8A261CAE8DC650A0 | 2026-10-09 01:18:05 | 2026-10-09 01:18:05 |
| MemoryPack.Core.dll | MATCH | PE | 101F0CFBF844D027 | 101F0CFBF844D027 | 2023-11-09 15:19:38 | 2023-11-09 15:19:38 |
| Microsoft.Extensions.Configuration.Abstractions.dll | MATCH | PE | FEFCDB267A73099C | FEFCDB267A73099C | 2023-10-31 22:59:08 | 2023-10-31 22:59:08 |
| Microsoft.Extensions.Configuration.Binder.dll | MATCH | PE | 5A47750F5C8EB91A | 5A47750F5C8EB91A | 2023-10-31 22:59:26 | 2023-10-31 22:59:26 |
| Microsoft.Extensions.Configuration.CommandLine.dll | MATCH | PE | 688EF791D4BB2E84 | 688EF791D4BB2E84 | 2023-10-31 22:59:28 | 2023-10-31 22:59:28 |
| Microsoft.Extensions.Configuration.dll | MATCH | PE | B5CD9DA2C3364A5B | B5CD9DA2C3364A5B | 2023-10-31 22:59:20 | 2023-10-31 22:59:20 |
| Microsoft.Extensions.Configuration.EnvironmentVariables.dll | MATCH | PE | 797B3034FD173CAF | 797B3034FD173CAF | 2023-10-31 22:59:28 | 2023-10-31 22:59:28 |
| Microsoft.Extensions.Configuration.FileExtensions.dll | MATCH | PE | D90FD867B38C1346 | D90FD867B38C1346 | 2023-10-31 22:59:28 | 2023-10-31 22:59:28 |
| Microsoft.Extensions.Configuration.Json.dll | MATCH | PE | 96920CAA680F7583 | 96920CAA680F7583 | 2023-10-31 22:59:36 | 2023-10-31 22:59:36 |
| Microsoft.Extensions.Configuration.UserSecrets.dll | MATCH | PE | A415ECB7EBDF4F65 | A415ECB7EBDF4F65 | 2023-10-31 22:59:42 | 2023-10-31 22:59:42 |
| Microsoft.Extensions.DependencyInjection.Abstractions.dll | MATCH | PE | A0106A638FACAE62 | A0106A638FACAE62 | 2023-10-31 22:58:56 | 2023-10-31 22:58:56 |
| Microsoft.Extensions.DependencyInjection.dll | MATCH | PE | 8D6E7B64CCE554F0 | 8D6E7B64CCE554F0 | 2023-10-31 22:59:00 | 2023-10-31 22:59:00 |
| Microsoft.Extensions.DependencyModel.dll | MATCH | PE | 73E69DA2A4462DCD | 73E69DA2A4462DCD | 2023-10-31 22:59:24 | 2023-10-31 22:59:24 |
| Microsoft.Extensions.Diagnostics.Abstractions.dll | MATCH | PE | BEA3CF225599B05A | BEA3CF225599B05A | 2023-10-31 22:59:22 | 2023-10-31 22:59:22 |
| Microsoft.Extensions.Diagnostics.dll | MATCH | PE | 5C83650F1C0A15C8 | 5C83650F1C0A15C8 | 2023-10-31 22:59:42 | 2023-10-31 22:59:42 |
| Microsoft.Extensions.FileProviders.Abstractions.dll | MATCH | PE | F08B1F597ABB3647 | F08B1F597ABB3647 | 2023-10-31 22:59:04 | 2023-10-31 22:59:04 |
| Microsoft.Extensions.FileProviders.Physical.dll | MATCH | PE | 9F67076C79A953F5 | 9F67076C79A953F5 | 2023-10-31 22:59:20 | 2023-10-31 22:59:20 |
| Microsoft.Extensions.FileSystemGlobbing.dll | MATCH | PE | ED0855B522F09B5A | ED0855B522F09B5A | 2023-10-31 22:58:56 | 2023-10-31 22:58:56 |
| Microsoft.Extensions.Hosting.Abstractions.dll | MATCH | PE | 1765C6147ED43B7C | 1765C6147ED43B7C | 2023-10-31 22:59:34 | 2023-10-31 22:59:34 |
| Microsoft.Extensions.Hosting.dll | MATCH | PE | 746B030485BCF78D | 746B030485BCF78D | 2023-10-31 22:59:44 | 2023-10-31 22:59:44 |
| Microsoft.Extensions.Logging.Abstractions.dll | MATCH | PE | 29E6BFAF5CE079AD | 29E6BFAF5CE079AD | 2023-10-31 22:59:12 | 2023-10-31 22:59:12 |
| Microsoft.Extensions.Logging.Configuration.dll | MATCH | PE | 4BD1245656BEF4EA | 4BD1245656BEF4EA | 2023-10-31 22:59:42 | 2023-10-31 22:59:42 |
| Microsoft.Extensions.Logging.Console.dll | MATCH | PE | B6836602E2139D6E | B6836602E2139D6E | 2023-10-31 22:59:44 | 2023-10-31 22:59:44 |
| Microsoft.Extensions.Logging.Debug.dll | MATCH | PE | 798B0714E3DFE8E8 | 798B0714E3DFE8E8 | 2023-10-31 22:59:30 | 2023-10-31 22:59:30 |
| Microsoft.Extensions.Logging.dll | MATCH | PE | 4330A5EFE9D110AF | 4330A5EFE9D110AF | 2023-10-31 22:59:24 | 2023-10-31 22:59:24 |
| Microsoft.Extensions.Logging.EventLog.dll | MATCH | PE | 1182928ECAA6AB23 | 1182928ECAA6AB23 | 2023-10-31 22:59:30 | 2023-10-31 22:59:30 |
| Microsoft.Extensions.Logging.EventSource.dll | MATCH | PE | E8CDE7AD083DC062 | E8CDE7AD083DC062 | 2023-10-31 22:59:30 | 2023-10-31 22:59:30 |
| Microsoft.Extensions.Options.ConfigurationExtensions.dll | MATCH | PE | B9EA7BB70BFECF5C | B9EA7BB70BFECF5C | 2023-10-31 22:59:38 | 2023-10-31 22:59:38 |
| Microsoft.Extensions.Options.dll | MATCH | PE | 5F9B0E589F1CE9CA | 5F9B0E589F1CE9CA | 2023-10-31 22:59:06 | 2023-10-31 22:59:06 |
| Microsoft.Extensions.Primitives.dll | MATCH | PE | 446FF16E903E7479 | 446FF16E903E7479 | 2023-10-31 22:58:52 | 2023-10-31 22:58:52 |
| Newtonsoft.Json.dll | MATCH | PE | 22C649F75FCE5BE7 | 22C649F75FCE5BE7 | 2023-03-08 15:09:54 | 2023-03-08 15:09:54 |
| NLog.dll | EXTRA | PE | 47C46DD1A18CB70A |  | 2026-10-09 01:32:29 |  |
| NLog.Extensions.Logging.dll | EXTRA | PE | 1EA14A337B666E55 |  | 2026-10-09 01:32:29 |  |
| OpenMir2.dll | MATCH | PE | 4CB2B14E6A594F18 | 4CB2B14E6A594F18 | 2026-10-09 01:18:05 | 2026-10-09 01:18:05 |
| protobuf-net.Core.dll | EXTRA | PE | 9F8779051AC65327 |  | 2026-10-09 01:32:29 |  |
| protobuf-net.dll | EXTRA | PE | 202ED016258DE40E |  | 2026-10-09 01:32:29 |  |
| Serilog.dll | MATCH | PE | 6B67FB3F10451303 | 6B67FB3F10451303 | 2023-11-10 20:41:54 | 2023-11-10 20:41:54 |
| Serilog.Extensions.Logging.dll | MATCH | PE | A0B6C860FA5BDB3F | A0B6C860FA5BDB3F | 2023-11-15 06:47:46 | 2023-11-15 06:47:46 |
| Serilog.Settings.Configuration.dll | MATCH | PE | 1159398C7EA29D8D | 1159398C7EA29D8D | 2023-11-15 09:17:44 | 2023-11-15 09:17:44 |
| Serilog.Sinks.Console.dll | MATCH | PE | 30B79981EF4EBD97 | 30B79981EF4EBD97 | 2023-11-29 04:32:02 | 2023-11-29 04:32:02 |
| SixLabors.ImageSharp.dll | EXTRA | PE | 8702A4944195D428 |  | 2026-10-09 01:32:29 |  |
| Spectre.Console.dll | MATCH | PE | 6D52BF7256858F97 | 6D52BF7256858F97 | 2024-02-01 00:48:20 | 2024-02-01 00:48:20 |
| Spectre.Console.ImageSharp.dll | EXTRA | PE | 50B4B126187AD6A5 |  | 2026-10-09 01:32:29 |  |
| System.Diagnostics.EventLog.dll | MATCH | PE | 61C0B16A1D6091B4 | 61C0B16A1D6091B4 | 2023-10-31 22:59:02 | 2023-10-31 22:59:02 |
| SystemModule.dll | EXTRA | PE | 9E0A0DDF14D2CFB4 |  | 2026-10-09 01:32:29 |  |
| TouchSocket.Core.dll | MATCH | PE | C5886A30644ECD6A | C5886A30644ECD6A | 2024-02-05 13:10:36 | 2024-02-05 13:10:36 |
| TouchSocket.dll | MATCH | PE | C37180F134F588C2 | C37180F134F588C2 | 2024-02-05 13:10:40 | 2024-02-05 13:10:40 |

## SelGate — SINGLE-BUILD

| 产物 | 判定 | 宿主 | 部署件 sha256[0:16] | 源码 sha256[0:16] | 部署时间 | 源码构建时间 |
| --- | --- | --- | --- | --- | --- | --- |
| MemoryPack.Core.dll | MATCH | PE | 101F0CFBF844D027 | 101F0CFBF844D027 | 2023-11-09 15:19:38 | 2023-11-09 15:19:38 |
| Microsoft.Extensions.Configuration.Abstractions.dll | MATCH | PE | FEFCDB267A73099C | FEFCDB267A73099C | 2023-10-31 22:59:08 | 2023-10-31 22:59:08 |
| Microsoft.Extensions.Configuration.Binder.dll | MATCH | PE | 5A47750F5C8EB91A | 5A47750F5C8EB91A | 2023-10-31 22:59:26 | 2023-10-31 22:59:26 |
| Microsoft.Extensions.Configuration.CommandLine.dll | MATCH | PE | 688EF791D4BB2E84 | 688EF791D4BB2E84 | 2023-10-31 22:59:28 | 2023-10-31 22:59:28 |
| Microsoft.Extensions.Configuration.dll | MATCH | PE | B5CD9DA2C3364A5B | B5CD9DA2C3364A5B | 2023-10-31 22:59:20 | 2023-10-31 22:59:20 |
| Microsoft.Extensions.Configuration.EnvironmentVariables.dll | MATCH | PE | 797B3034FD173CAF | 797B3034FD173CAF | 2023-10-31 22:59:28 | 2023-10-31 22:59:28 |
| Microsoft.Extensions.Configuration.FileExtensions.dll | MATCH | PE | D90FD867B38C1346 | D90FD867B38C1346 | 2023-10-31 22:59:28 | 2023-10-31 22:59:28 |
| Microsoft.Extensions.Configuration.Json.dll | MATCH | PE | 96920CAA680F7583 | 96920CAA680F7583 | 2023-10-31 22:59:36 | 2023-10-31 22:59:36 |
| Microsoft.Extensions.Configuration.UserSecrets.dll | MATCH | PE | A415ECB7EBDF4F65 | A415ECB7EBDF4F65 | 2023-10-31 22:59:42 | 2023-10-31 22:59:42 |
| Microsoft.Extensions.DependencyInjection.Abstractions.dll | MATCH | PE | A0106A638FACAE62 | A0106A638FACAE62 | 2023-10-31 22:58:56 | 2023-10-31 22:58:56 |
| Microsoft.Extensions.DependencyInjection.dll | MATCH | PE | 8D6E7B64CCE554F0 | 8D6E7B64CCE554F0 | 2023-10-31 22:59:00 | 2023-10-31 22:59:00 |
| Microsoft.Extensions.DependencyModel.dll | MATCH | PE | 73E69DA2A4462DCD | 73E69DA2A4462DCD | 2023-10-31 22:59:24 | 2023-10-31 22:59:24 |
| Microsoft.Extensions.Diagnostics.Abstractions.dll | MATCH | PE | BEA3CF225599B05A | BEA3CF225599B05A | 2023-10-31 22:59:22 | 2023-10-31 22:59:22 |
| Microsoft.Extensions.Diagnostics.dll | MATCH | PE | 5C83650F1C0A15C8 | 5C83650F1C0A15C8 | 2023-10-31 22:59:42 | 2023-10-31 22:59:42 |
| Microsoft.Extensions.FileProviders.Abstractions.dll | MATCH | PE | F08B1F597ABB3647 | F08B1F597ABB3647 | 2023-10-31 22:59:04 | 2023-10-31 22:59:04 |
| Microsoft.Extensions.FileProviders.Physical.dll | MATCH | PE | 9F67076C79A953F5 | 9F67076C79A953F5 | 2023-10-31 22:59:20 | 2023-10-31 22:59:20 |
| Microsoft.Extensions.FileSystemGlobbing.dll | MATCH | PE | ED0855B522F09B5A | ED0855B522F09B5A | 2023-10-31 22:58:56 | 2023-10-31 22:58:56 |
| Microsoft.Extensions.Hosting.Abstractions.dll | MATCH | PE | 1765C6147ED43B7C | 1765C6147ED43B7C | 2023-10-31 22:59:34 | 2023-10-31 22:59:34 |
| Microsoft.Extensions.Hosting.dll | MATCH | PE | 746B030485BCF78D | 746B030485BCF78D | 2023-10-31 22:59:44 | 2023-10-31 22:59:44 |
| Microsoft.Extensions.Logging.Abstractions.dll | MATCH | PE | 29E6BFAF5CE079AD | 29E6BFAF5CE079AD | 2023-10-31 22:59:12 | 2023-10-31 22:59:12 |
| Microsoft.Extensions.Logging.Configuration.dll | MATCH | PE | 4BD1245656BEF4EA | 4BD1245656BEF4EA | 2023-10-31 22:59:42 | 2023-10-31 22:59:42 |
| Microsoft.Extensions.Logging.Console.dll | MATCH | PE | B6836602E2139D6E | B6836602E2139D6E | 2023-10-31 22:59:44 | 2023-10-31 22:59:44 |
| Microsoft.Extensions.Logging.Debug.dll | MATCH | PE | 798B0714E3DFE8E8 | 798B0714E3DFE8E8 | 2023-10-31 22:59:30 | 2023-10-31 22:59:30 |
| Microsoft.Extensions.Logging.dll | MATCH | PE | 4330A5EFE9D110AF | 4330A5EFE9D110AF | 2023-10-31 22:59:24 | 2023-10-31 22:59:24 |
| Microsoft.Extensions.Logging.EventLog.dll | MATCH | PE | 1182928ECAA6AB23 | 1182928ECAA6AB23 | 2023-10-31 22:59:30 | 2023-10-31 22:59:30 |
| Microsoft.Extensions.Logging.EventSource.dll | MATCH | PE | E8CDE7AD083DC062 | E8CDE7AD083DC062 | 2023-10-31 22:59:30 | 2023-10-31 22:59:30 |
| Microsoft.Extensions.Options.ConfigurationExtensions.dll | MATCH | PE | B9EA7BB70BFECF5C | B9EA7BB70BFECF5C | 2023-10-31 22:59:38 | 2023-10-31 22:59:38 |
| Microsoft.Extensions.Options.dll | MATCH | PE | 5F9B0E589F1CE9CA | 5F9B0E589F1CE9CA | 2023-10-31 22:59:06 | 2023-10-31 22:59:06 |
| Microsoft.Extensions.Primitives.dll | MATCH | PE | 446FF16E903E7479 | 446FF16E903E7479 | 2023-10-31 22:58:52 | 2023-10-31 22:58:52 |
| Newtonsoft.Json.dll | MATCH | PE | 22C649F75FCE5BE7 | 22C649F75FCE5BE7 | 2023-03-08 15:09:54 | 2023-03-08 15:09:54 |
| NLog.dll | EXTRA | PE | 47C46DD1A18CB70A |  | 2026-10-09 01:32:30 |  |
| NLog.Extensions.Logging.dll | EXTRA | PE | 1EA14A337B666E55 |  | 2026-10-09 01:32:30 |  |
| OpenMir2.dll | MATCH | PE | 4CB2B14E6A594F18 | 4CB2B14E6A594F18 | 2026-10-09 01:18:05 | 2026-10-09 01:18:05 |
| protobuf-net.Core.dll | EXTRA | PE | 9F8779051AC65327 |  | 2026-10-09 01:32:30 |  |
| protobuf-net.dll | EXTRA | PE | 202ED016258DE40E |  | 2026-10-09 01:32:30 |  |
| SelGate | EXTRA | MACHO | DEA3F26C35466BB2 |  | 2026-10-09 01:32:30 |  |
| SelGate.dll | MATCH | PE | 2251120F252A981E | 2251120F252A981E | 2026-10-09 01:18:05 | 2026-10-09 01:18:05 |
| SelGate.exe | MATCH | PE | FA6F57654777D807 | FA6F57654777D807 | 2026-10-09 01:18:05 | 2026-10-09 01:18:05 |
| Serilog.dll | MATCH | PE | 6B67FB3F10451303 | 6B67FB3F10451303 | 2023-11-10 20:41:54 | 2023-11-10 20:41:54 |
| Serilog.Extensions.Logging.dll | MATCH | PE | A0B6C860FA5BDB3F | A0B6C860FA5BDB3F | 2023-11-15 06:47:46 | 2023-11-15 06:47:46 |
| Serilog.Settings.Configuration.dll | MATCH | PE | 1159398C7EA29D8D | 1159398C7EA29D8D | 2023-11-15 09:17:44 | 2023-11-15 09:17:44 |
| Serilog.Sinks.Console.dll | MATCH | PE | 30B79981EF4EBD97 | 30B79981EF4EBD97 | 2023-11-29 04:32:02 | 2023-11-29 04:32:02 |
| SixLabors.ImageSharp.dll | EXTRA | PE | 8702A4944195D428 |  | 2026-10-09 01:32:30 |  |
| Spectre.Console.dll | MATCH | PE | 6D52BF7256858F97 | 6D52BF7256858F97 | 2024-02-01 00:48:20 | 2024-02-01 00:48:20 |
| Spectre.Console.ImageSharp.dll | EXTRA | PE | 50B4B126187AD6A5 |  | 2026-10-09 01:32:30 |  |
| System.Diagnostics.EventLog.dll | MATCH | PE | 61C0B16A1D6091B4 | 61C0B16A1D6091B4 | 2023-10-31 22:59:02 | 2023-10-31 22:59:02 |
| SystemModule.dll | EXTRA | PE | 9E0A0DDF14D2CFB4 |  | 2026-10-09 01:32:30 |  |
| TouchSocket.Core.dll | MATCH | PE | C5886A30644ECD6A | C5886A30644ECD6A | 2024-02-05 13:10:36 | 2024-02-05 13:10:36 |
| TouchSocket.dll | MATCH | PE | C37180F134F588C2 | C37180F134F588C2 | 2024-02-05 13:10:40 | 2024-02-05 13:10:40 |

## RunGate — SINGLE-BUILD

| 产物 | 判定 | 宿主 | 部署件 sha256[0:16] | 源码 sha256[0:16] | 部署时间 | 源码构建时间 |
| --- | --- | --- | --- | --- | --- | --- |
| GameGate | EXTRA | MACHO | 88AD55065D66DE13 |  | 2026-10-09 01:32:30 |  |
| GameGate.dll | MATCH | PE | 5EC04AAEF0646325 | 5EC04AAEF0646325 | 2026-10-09 01:18:06 | 2026-10-09 01:18:06 |
| GameGate.exe | MATCH | PE | 4A4B1CC8A5A398B9 | 4A4B1CC8A5A398B9 | 2026-10-09 01:18:06 | 2026-10-09 01:18:06 |
| MemoryPack.Core.dll | MATCH | PE | 101F0CFBF844D027 | 101F0CFBF844D027 | 2023-11-09 15:19:38 | 2023-11-09 15:19:38 |
| Microsoft.Extensions.Configuration.Abstractions.dll | MATCH | PE | FEFCDB267A73099C | FEFCDB267A73099C | 2023-10-31 22:59:08 | 2023-10-31 22:59:08 |
| Microsoft.Extensions.Configuration.Binder.dll | MATCH | PE | 5A47750F5C8EB91A | 5A47750F5C8EB91A | 2023-10-31 22:59:26 | 2023-10-31 22:59:26 |
| Microsoft.Extensions.Configuration.CommandLine.dll | MATCH | PE | 688EF791D4BB2E84 | 688EF791D4BB2E84 | 2023-10-31 22:59:28 | 2023-10-31 22:59:28 |
| Microsoft.Extensions.Configuration.dll | MATCH | PE | B5CD9DA2C3364A5B | B5CD9DA2C3364A5B | 2023-10-31 22:59:20 | 2023-10-31 22:59:20 |
| Microsoft.Extensions.Configuration.EnvironmentVariables.dll | MATCH | PE | 797B3034FD173CAF | 797B3034FD173CAF | 2023-10-31 22:59:28 | 2023-10-31 22:59:28 |
| Microsoft.Extensions.Configuration.FileExtensions.dll | MATCH | PE | D90FD867B38C1346 | D90FD867B38C1346 | 2023-10-31 22:59:28 | 2023-10-31 22:59:28 |
| Microsoft.Extensions.Configuration.Json.dll | MATCH | PE | 96920CAA680F7583 | 96920CAA680F7583 | 2023-10-31 22:59:36 | 2023-10-31 22:59:36 |
| Microsoft.Extensions.Configuration.UserSecrets.dll | MATCH | PE | A415ECB7EBDF4F65 | A415ECB7EBDF4F65 | 2023-10-31 22:59:42 | 2023-10-31 22:59:42 |
| Microsoft.Extensions.DependencyInjection.Abstractions.dll | MATCH | PE | A0106A638FACAE62 | A0106A638FACAE62 | 2023-10-31 22:58:56 | 2023-10-31 22:58:56 |
| Microsoft.Extensions.DependencyInjection.dll | MATCH | PE | 8D6E7B64CCE554F0 | 8D6E7B64CCE554F0 | 2023-10-31 22:59:00 | 2023-10-31 22:59:00 |
| Microsoft.Extensions.DependencyModel.dll | MATCH | PE | 73E69DA2A4462DCD | 73E69DA2A4462DCD | 2023-10-31 22:59:24 | 2023-10-31 22:59:24 |
| Microsoft.Extensions.Diagnostics.Abstractions.dll | MATCH | PE | BEA3CF225599B05A | BEA3CF225599B05A | 2023-10-31 22:59:22 | 2023-10-31 22:59:22 |
| Microsoft.Extensions.Diagnostics.dll | MATCH | PE | 5C83650F1C0A15C8 | 5C83650F1C0A15C8 | 2023-10-31 22:59:42 | 2023-10-31 22:59:42 |
| Microsoft.Extensions.FileProviders.Abstractions.dll | MATCH | PE | F08B1F597ABB3647 | F08B1F597ABB3647 | 2023-10-31 22:59:04 | 2023-10-31 22:59:04 |
| Microsoft.Extensions.FileProviders.Physical.dll | MATCH | PE | 9F67076C79A953F5 | 9F67076C79A953F5 | 2023-10-31 22:59:20 | 2023-10-31 22:59:20 |
| Microsoft.Extensions.FileSystemGlobbing.dll | MATCH | PE | ED0855B522F09B5A | ED0855B522F09B5A | 2023-10-31 22:58:56 | 2023-10-31 22:58:56 |
| Microsoft.Extensions.Hosting.Abstractions.dll | MATCH | PE | 1765C6147ED43B7C | 1765C6147ED43B7C | 2023-10-31 22:59:34 | 2023-10-31 22:59:34 |
| Microsoft.Extensions.Hosting.dll | MATCH | PE | 746B030485BCF78D | 746B030485BCF78D | 2023-10-31 22:59:44 | 2023-10-31 22:59:44 |
| Microsoft.Extensions.Logging.Abstractions.dll | MATCH | PE | 29E6BFAF5CE079AD | 29E6BFAF5CE079AD | 2023-10-31 22:59:12 | 2023-10-31 22:59:12 |
| Microsoft.Extensions.Logging.Configuration.dll | MATCH | PE | 4BD1245656BEF4EA | 4BD1245656BEF4EA | 2023-10-31 22:59:42 | 2023-10-31 22:59:42 |
| Microsoft.Extensions.Logging.Console.dll | MATCH | PE | B6836602E2139D6E | B6836602E2139D6E | 2023-10-31 22:59:44 | 2023-10-31 22:59:44 |
| Microsoft.Extensions.Logging.Debug.dll | MATCH | PE | 798B0714E3DFE8E8 | 798B0714E3DFE8E8 | 2023-10-31 22:59:30 | 2023-10-31 22:59:30 |
| Microsoft.Extensions.Logging.dll | MATCH | PE | 4330A5EFE9D110AF | 4330A5EFE9D110AF | 2023-10-31 22:59:24 | 2023-10-31 22:59:24 |
| Microsoft.Extensions.Logging.EventLog.dll | MATCH | PE | 1182928ECAA6AB23 | 1182928ECAA6AB23 | 2023-10-31 22:59:30 | 2023-10-31 22:59:30 |
| Microsoft.Extensions.Logging.EventSource.dll | MATCH | PE | E8CDE7AD083DC062 | E8CDE7AD083DC062 | 2023-10-31 22:59:30 | 2023-10-31 22:59:30 |
| Microsoft.Extensions.Options.ConfigurationExtensions.dll | MATCH | PE | B9EA7BB70BFECF5C | B9EA7BB70BFECF5C | 2023-10-31 22:59:38 | 2023-10-31 22:59:38 |
| Microsoft.Extensions.Options.dll | MATCH | PE | 5F9B0E589F1CE9CA | 5F9B0E589F1CE9CA | 2023-10-31 22:59:06 | 2023-10-31 22:59:06 |
| Microsoft.Extensions.Primitives.dll | MATCH | PE | 446FF16E903E7479 | 446FF16E903E7479 | 2023-10-31 22:58:52 | 2023-10-31 22:58:52 |
| Newtonsoft.Json.dll | MATCH | PE | 22C649F75FCE5BE7 | 22C649F75FCE5BE7 | 2023-03-08 15:09:54 | 2023-03-08 15:09:54 |
| NLog.dll | EXTRA | PE | 47C46DD1A18CB70A |  | 2026-10-09 01:32:30 |  |
| NLog.Extensions.Logging.dll | EXTRA | PE | 1EA14A337B666E55 |  | 2026-10-09 01:32:30 |  |
| OpenMir2.dll | MATCH | PE | 4CB2B14E6A594F18 | 4CB2B14E6A594F18 | 2026-10-09 01:18:05 | 2026-10-09 01:18:05 |
| protobuf-net.Core.dll | EXTRA | PE | 9F8779051AC65327 |  | 2026-10-09 01:32:30 |  |
| protobuf-net.dll | EXTRA | PE | 202ED016258DE40E |  | 2026-10-09 01:32:30 |  |
| Serilog.dll | MATCH | PE | 6B67FB3F10451303 | 6B67FB3F10451303 | 2023-11-10 20:41:54 | 2023-11-10 20:41:54 |
| Serilog.Extensions.Logging.dll | MATCH | PE | A0B6C860FA5BDB3F | A0B6C860FA5BDB3F | 2023-11-15 06:47:46 | 2023-11-15 06:47:46 |
| Serilog.Settings.Configuration.dll | MATCH | PE | 1159398C7EA29D8D | 1159398C7EA29D8D | 2023-11-15 09:17:44 | 2023-11-15 09:17:44 |
| Serilog.Sinks.Console.dll | MATCH | PE | 30B79981EF4EBD97 | 30B79981EF4EBD97 | 2023-11-29 04:32:02 | 2023-11-29 04:32:02 |
| SixLabors.ImageSharp.dll | EXTRA | PE | 8702A4944195D428 |  | 2026-10-09 01:32:30 |  |
| Spectre.Console.dll | MATCH | PE | 6D52BF7256858F97 | 6D52BF7256858F97 | 2024-02-01 00:48:20 | 2024-02-01 00:48:20 |
| Spectre.Console.ImageSharp.dll | EXTRA | PE | 50B4B126187AD6A5 |  | 2026-10-09 01:32:30 |  |
| System.Diagnostics.EventLog.dll | MATCH | PE | 61C0B16A1D6091B4 | 61C0B16A1D6091B4 | 2023-10-31 22:59:02 | 2023-10-31 22:59:02 |
| SystemModule.dll | EXTRA | PE | 9E0A0DDF14D2CFB4 |  | 2026-10-09 01:32:30 |  |
| TouchSocket.Core.dll | MATCH | PE | C5886A30644ECD6A | C5886A30644ECD6A | 2024-02-05 13:10:36 | 2024-02-05 13:10:36 |
| TouchSocket.dll | MATCH | PE | C37180F134F588C2 | C37180F134F588C2 | 2024-02-05 13:10:40 | 2024-02-05 13:10:40 |

## 未声明为部署件的目录

| 目录 | apphost | 宿主 | deps runtimeTarget | 说明 |
| --- | --- | --- | --- | --- |
| CloudGate | CloudGate | MACHO | .NETCoreApp,Version=v6.0 | 未声明为 oracle 部署件（start-all.ps1 的 $Components 里没有） |
| DBServer | DBSvr | MACHO | .NETCoreApp,Version=v6.0 | 未声明为 oracle 部署件（start-all.ps1 的 $Components 里没有） |
| LoginSrv | LoginSvr | MACHO | .NETCoreApp,Version=v6.0 | 未声明为 oracle 部署件（start-all.ps1 的 $Components 里没有） |
| MakePlay | MakePlayer | MACHO | .NETCoreApp,Version=v6.0 | 未声明为 oracle 部署件（start-all.ps1 的 $Components 里没有） |
| Mir200 | GameSvr | MACHO | .NETCoreApp,Version=v6.0 | 未声明为 oracle 部署件（start-all.ps1 的 $Components 里没有） |

