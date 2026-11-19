# tools/orchestration/stamp-groups.psd1
# C# 产物清单的**单一来源**（卡 `openmir2-own-binary-without-stamp` R2）。
#
# 消费者（任何一处改动都要跟着本表走，改表即改全部）：
#   * tools/window/start-csharp.ps1               起栈前的影子 provenance 门
#   * tools/hygiene/assert_three_way_aligned.ps1  三处对齐断言的比对清单
#   * E:/UserFleet/m1-stack/start-m1-stack.ps1    起栈门（$StampGroups）
#   * E:/UserFleet/w-freeze-release.ps1           冻结集（$groups）与 PROVENANCE 计数
#
# OwnGlobs 的语义：`stamp-binaries.ps1 -Check -OwnGlobs` 对 BinDir 里匹配 glob 的每个 .dll/.exe
# 要求它已出现在 `-Names`（本表 Groups）里；未出现 ⇒ RED UNLISTED_OWN_BINARY。
# 该检查**独立于 -RequireAll**（R3）——防「自动拷进输出的自家插件没进名单也全绿」。
@{
    Groups = @{
        DBSrv    = @(
            'DBSrv.exe',
            'DBSrv.dll',
            'DBSrv.Storage.dll',
            'DBSrv.Storage.MySQL.dll',
            'DBSrv.Storage.MongoDB.dll',
            'OpenMir2.dll'
        )
        LoginSrv = @(
            'LoginSrv.exe',
            'LoginSrv.dll',
            'OpenMir2.dll'
        )
    }
    OwnGlobs = @{
        DBSrv    = @('DBSrv*.dll', 'OpenMir2.dll')
        LoginSrv = @('LoginSrv*.dll', 'OpenMir2.dll')
    }
}
