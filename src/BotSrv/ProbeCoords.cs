using OpenMir2;

namespace BotSrv
{
    /// <summary>
    /// 压测动作探针的**坐标口径**（卡 `openmir2-botsrv-action-probe-turn-coords`）。
    ///
    /// 服务端 `CM_TURN` 只在 `MakeLong(X,Y)` 等于它自己的 `CurrX/CurrY` 时转身并回 `#+GD/...!`
    /// （oracle `PlayObject.Operate.cs:310-348` 的 `if (nX == CurrX && nY == CurrY)`；Rust
    /// `movewire::turn_is_at_target` 同义）。探针原先只用 `SM_LOGON` 的落点，进世界后服务端**自己**
    /// 把假人挪走（重叠推挤 `CharPushed`、空间移动 `SpaceMove`）时坐标就永远对不上 ⇒ 每次探测都被拒。
    ///
    /// 本类给出「这一帧要不要改本假人的探针坐标」的纯判定：**只认发给本假人自己的**帧
    /// （帧头 `Recog == 自身 ActorId`），且只认两类服务端驱动位移：
    ///   · `SM_BACKSTEP(9)` = `RM_PUSH` ⇒ `Param/Tag` = 推进后的 `CurrX/CurrY`
    ///     （oracle `BaseObject.cs:871` 每推进一格 `SendRefMsg(RM_PUSH, nBackDir, CurrX, CurrY, 0, "")`；
    ///     `PlayObject.Message.cs:1528` 对 `RM_PUSH` 不做 `ActorId != ActorId` 排除 ⇒ 被推者自己也收到）；
    ///   · `SM_SPACEMOVE_SHOW(801)` / `SM_SPACEMOVE_SHOW2(807)` = `RM_SPACEMOVE_SHOW*`
    ///     ⇒ `Param/Tag` = 落点坐标（`PlayObject.Message.cs:1971/1975`）。
    ///
    /// **不**在这里处理：`SM_LOGON`（`MarkInWorld` 直接写落点）、假人自己的 `CM_WALK/RUN`
    /// （客户端预测，由 `MShare.MySelf` 承担；200/1000 档共享态下不产生移动，见卡面证据）。
    /// </summary>
    internal static class ProbeCoords
    {
        /// <summary>该帧是否把本假人的服务端坐标改掉；true ⇒ `x/y` 是新坐标。</summary>
        public static bool TrySelfPosition(int ident, int recog, int param, int tag, int myActorId, out int x, out int y)
        {
            x = 0;
            y = 0;
            if (myActorId == 0 || recog != myActorId)
            {
                return false;
            }
            switch (ident)
            {
                case Messages.SM_BACKSTEP:
                case Messages.SM_SPACEMOVE_SHOW:
                case Messages.SM_SPACEMOVE_SHOW2:
                    x = param;
                    y = tag;
                    return true;
                default:
                    return false;
            }
        }
    }
}
