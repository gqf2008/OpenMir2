using BotSrv.Data;
using BotSrv.Maps;
using BotSrv.Objects;
using BotSrv.Player;
using OpenMir2;
using OpenMir2.Enums;
using OpenMir2.Packets.ServerPackets;
using System;
using System.Collections;
using System.Collections.Generic;
using System.Drawing;

namespace BotSrv.Scenes.Scene
{
    public class PlayScene : SceneBase
    {
        private int m_dwMoveTime = 0;
        private readonly int _mNDefXx = 0;
        private readonly int _mNDefYy = 0;
        public IList<Actor> ActorList = null;
        public ProcMagic ProcMagic = null;
        /// <summary>仅自检注入：在「Count 检查通过、下标读取之前」执行，用于确定性模拟收包线程缩表
        /// （卡 `openmir2-botsrv-autoplay-nre` F1）。生产为 null = 零行为变化。</summary>
        internal static Action<PlayScene> SceneIndexRaceHook;
        /// <summary>仅自检注入：在「删除点身份核对通过、RemoveAt 之前」执行，用于确定性模拟收包线程缩表
        /// （卡 `openmir2-botsrv-autoplay-nre` F1b）。生产为 null = 零行为变化。</summary>
        internal static Action<PlayScene> SceneRemoveRaceHook;
        /// <summary>仅自检注入：在「身份核对已通过、RemoveAt 之前」执行（F1b 的 RemoveAt 窗口）。</summary>
        internal static Action<PlayScene> SceneRemoveAtRaceHook;
        /// <summary>仅自检注入：在 ActorDied 删除 actor 之前执行（卡 `openmir2-botsrv-autoplay-nre` F1b
        /// 「顺带核」）。生产为 null = 零行为变化。</summary>
        internal static Action<PlayScene> SceneActorDiedRaceHook;

        /// <summary>每假人场景的细粒度锁（卡 `openmir2-botsrv-shared-state-locks`）：
        /// 收包线程（SendMsg/NewActor/DeleteActor/DelActor/ClearActors）与 runThread（BeginScene）
        /// 对 `ActorList` 的读写都经它串行；**只保护列表结构**，不包住 actor 逻辑（保持并行度）。</summary>
        private readonly object _actorSync = new object();

        /// <summary>拷贝当前 ActorList 用于无锁迭代（读快照；不持有锁跑 actor 逻辑）。</summary>
        public Actor[] SnapshotActors()
        {
            lock (_actorSync)
            {
                Actor[] copy = new Actor[ActorList.Count];
                ActorList.CopyTo(copy, 0);
                return copy;
            }
        }

        /// <summary>原子 `Count` 检查 + 下标读取（收包线程可能在两次调用之间缩表）。</summary>
        private bool TryGetActorAt(int index, out Actor actor)
        {
            lock (_actorSync)
            {
                if (index >= 0 && index < ActorList.Count)
                {
                    actor = ActorList[index];
                    return true;
                }
                actor = null;
                return false;
            }
        }

        private int ActorCountLocked()
        {
            lock (_actorSync)
            {
                return ActorList.Count;
            }
        }

        // 卡 `openmir2-botsrv-actor-pool-leak`：原 `AddFreeActor`（把 actor 登记进跨 bot 自由池）已移除。
        // 冻结客户端 `PlayScn.pas:3255-3266` 的 `NewActor` 从不从 `g_FreeActorList` 取用（恒 Create）
        // ⇒ 池不是复用池，只是「延迟 60s Free」的缓冲；托管运行时立即解除引用即可，无需保留结构。
        // AOI 驱逐 / DeleteActor / DelActor 三个出口现在都只做 `ActorList.Remove` + 计数。

        public PlayScene(RobotPlayer robotClient) : base(SceneType.PlayGame, robotClient)
        {
            ProcMagic = new ProcMagic();
            ProcMagic.NTargetX = -1;
            ActorList = new List<Actor>();
            m_dwMoveTime = HUtil32.GetTickCount();
        }

        public override void OpenScene()
        {
            RobotClient.SocketEvents();
        }

        public override void CloseScene()
        {

        }

        public override void OpeningScene()
        {

        }

        public void CleanObjects()
        {
            //int i;
            //for (i = MActorList.Count - 1; i >= 0; i--)
            //{
            //    // (TActor(m_ActorList[i]) <> g_MySelf.m_SlaveObject)
            //    if ((((TActor)(MActorList[i])) != MShare.g_MySelf) && (((TActor)(MActorList[i])) != MShare.g_MySelf.m_HeroObject) && !PlayScn.IsMySlaveObject(((TActor)(MActorList[i]))))
            //    {
            //        // BLUE
            //        ((TActor)(MActorList[i])).Free;
            //        MActorList.RemoveAt(i);
            //    }
            //}
            //MShare.g_TargetCret = null;
            //MShare.g_FocusCret = null;
            //MShare.g_MagicTarget = null;
            //for (i = 0; i < MEffectList.Count; i++)
            //{
            //    ((TMagicEff)(MEffectList[i])).Free;
            //}
            //MEffectList.Clear();
        }

        private void ClearDropItemA()
        {
            TDropItem dropItem;
            for (int i = MShare.g_DropedItemList.Count - 1; i >= 0; i--)
            {
                dropItem = MShare.g_DropedItemList[i];
                if (dropItem == null)
                {
                    MShare.g_DropedItemList.RemoveAt(i);
                    // Continue;
                    break;
                }
                if ((Math.Abs(dropItem.X - MShare.MySelf.CurrX) > 20) || (Math.Abs(dropItem.Y - MShare.MySelf.CurrY) > 20))
                {
                    MShare.g_DropedItemList.RemoveAt(i);
                    break;
                }
            }
        }

        public void BeginScene()
        {
            // 轮首快照"自己"（卡 `openmir2-botsrv-autoplay-nre`）：真实 Delphi 客户端是单线程
            // （收包与场景循环同一消息泵），`g_MySelf` 只可能在两轮之间变化；这里按同一语义取一次
            // 快照，避免 BotSrv 的收包线程（SM_LOGON 重绑 / ClearActors）在循环中途把共享
            // `MShare.MySelf` 置 null ⇒ line 161 的 NRE。快照只作本轮比较/坐标基准。
            THumActor self = MShare.MySelf;
            if (self == null)
            {
                return;
            }
            bool movetick = false;
            int tick = HUtil32.GetTickCount();
            if (MShare.SpeedRate)
            {
                if (tick - m_dwMoveTime >= (95 - MShare.g_MoveSpeedRate / 2))
                {
                    m_dwMoveTime = tick;
                    movetick = true;
                }
            }
            else
            {
                if (tick - m_dwMoveTime >= 95)
                {
                    m_dwMoveTime = tick;
                    movetick = true;
                }
            }
            // 卡 openmir2-botsrv-recv-thread-nre：null-skip 上界的分母（每轮 BeginScene 一次；
            // MySelf==null 的早退在上方已 return，不计）。只计数，不改变行为。
            LoadMetrics.SceneRoundStarted();
            int i = 0;
            while (true)
            {
                if (i >= ActorCountLocked())
                {
                    break;
                }
                SceneIndexRaceHook?.Invoke(this);
                Actor actor;
                if (!TryGetActorAt(i, out actor))
                {
                    // `Count` 检查与下标读取之间列表被缩短（生产由 `_actorSync` 挡住；自检 hook
                    // 可有确定性注入）。本轮直接结束并计数，不把 AOORE 抛给 SafeRun。
                    LoadMetrics.SceneIndexRaceSkipped();
                    break;
                }
                if (actor == null)
                {
                    // 夹具健壮性（卡 `openmir2-botsrv-autoplay-nre` F3）：`ActorList` 会被收包线程
                    // （SM_HIDE/SM_NEWMAP 删除、AOI 增删）与本 run 线程并发增删，槽位可能瞬时为 null。
                    // 归因精度：隔离注入 null 槽位在离线复现为 NRE@121（actor.Death），ActorList=null
                    // 为 NRE@116；归档里的 :120 是**并发下索引读取竞争**的签名（见上面 try/catch）。
                    // 真实客户端 `m_ActorList` 只在 UI 线程读写（不会出现 null），且对「帧引用不存在
                    // 的 actor」是显式跳过（`PlayScn.pas:3670-3690`：创建不出 ⇒ `if Actor = nil then Exit`）。
                    // 这里照抄同义：跳过该槽位、计数、不抛 NRE；不自创重试/重建策略。
                    LoadMetrics.SceneNullActorSkipped();
                    i++;
                    continue;
                }
                if (actor.Death && MShare.g_gcGeneral[8] && !actor.m_boItemExplore && (actor.Race != 0) && actor.IsIdle())
                {
                    i++;
                    continue;
                }
                if (movetick)
                {
                    actor.m_boLockEndFrame = false;
                }
                if (!actor.m_boLockEndFrame)
                {
                    actor.ProcMsg();   //处理角色的消息
                    if (movetick)
                    {
                        if (actor.Move())
                        {
                            i++;
                            continue;
                        }
                    }
                    actor.Run();
                    if (actor != self)
                    {
                        actor.ProcHurryMsg();
                    }
                }
                if (actor == self)
                {
                    actor.ProcHurryMsg();
                }
                if (actor.m_nWaitForRecogId != 0)
                {
                    if (actor.IsIdle())
                    {
                        BotHelper.DelChangeFace(actor.m_nWaitForRecogId);
                        NewActor(actor.m_nWaitForRecogId, actor.CurrX, actor.CurrY, actor.m_btDir, actor.m_nWaitForFeature, actor.m_nWaitForStatus);
                        actor.m_nWaitForRecogId = 0;
                        actor.DelActor = true;
                    }
                }
                // 对齐冻结客户端 PlayScn.pas:1219：AOI 驱逐要求「距上次取消息 >2000ms」**且**
                // 「与自身坐标差 >16 格」；`DelActor` 仍无条件。端口此前缺空闲守卫 ⇒ 任何越界立即驱逐
                // （并把 actor 永久留池），既是行为偏差也是泄漏的驱动源。
                bool aoiIdleExpired = MShare.GetTickCount() - actor.m_dwLastGetMessageTime > 2000;
                if (actor.DelActor || (aoiIdleExpired && ((Math.Abs(self.CurrX - actor.CurrX) > 16) || (Math.Abs(self.CurrY - actor.CurrY) > 16))))
                {
                    // 并发一致性（卡 `openmir2-botsrv-autoplay-nre` F1b/F1c/F2）：本槽位可能已被收包线程
                    // （SM_HIDE/SM_NEWMAP/AOI 增删）或 actor.Run() 的同线程重入改掉。删除路径的整体——
                    // **包括身份核对的条件读取 `ActorList[i]`**——都必须在同一个 try 内（F1c：核对
                    // 读取本身也会 AOORE@209）；顺序 = 先 RemoveAt 成功、再计数（无池可登记）；
                    // AOORE ⇒ 跳过 + 计数 + 下标前移，不重复计数、不清理引用。
                    bool removed = false;
                    lock (_actorSync)
                    {
                        try
                        {
                            if (i < ActorList.Count)
                            {
                                SceneRemoveRaceHook?.Invoke(this);   // F1c：条件读取前缩表（自检）
                                if (ActorList[i] == actor)
                                {
                                    SceneRemoveAtRaceHook?.Invoke(this); // F1b：RemoveAt 前缩表（自检）
                                    ActorList.RemoveAt(i);
                                    removed = true;
                                }
                                else
                                {
                                    LoadMetrics.SceneListRaceSkipped();
                                    i++;
                                }
                            }
                            else
                            {
                                LoadMetrics.SceneListRaceSkipped();
                                i++;
                            }
                        }
                        catch (ArgumentOutOfRangeException)
                        {
                            LoadMetrics.SceneListRaceSkipped();
                            i++;
                            removed = false;
                        }
                    }
                    if (removed)
                    {
                        // 立即解除引用（无自由池，见类顶部说明）：actor 在此之后不可达，由 GC 回收。
                        LoadMetrics.SceneActorEjected();
                        if (MShare.TargetCret == actor)
                        {
                            MShare.TargetCret = null;
                        }
                        if (MShare.FocusCret == actor)
                        {
                            MShare.FocusCret = null;
                        }
                        if (MShare.MagicLockActor == actor)
                        {
                            MShare.MagicLockActor = null;
                        }
                        if (MShare.MagicTarget == actor)
                        {
                            MShare.MagicTarget = null;
                        }
                    }
                }
                else
                {
                    i++;
                }
            }
            RobotClient.Map.UpdateMapPos(self.m_nRx, self.m_nRy);
        }

        public void PlaySurface(object sender)
        {

        }

        public void MagicSurface(object sender)
        {

        }

        public void NewMagic(Actor aowner, int magid, int magnumb, int cx, int cy, int tx, int ty, int targetCode, MagicType mtype, bool recusion, int anitime, ref bool boFly, int maglv, int poison)
        {

        }

        public void DelMagic(int magid)
        {
            //for (var i = 0; i < MEffectList.Count; i++)
            //{
            //    if (((TMagicEff)MEffectList[i]).ServerMagicId == magid)
            //    {
            //        ((TMagicEff)MEffectList[i]).Free;
            //        MEffectList.RemoveAt(i);
            //        break;
            //    }
            //}
        }

        public Actor GetCharacter(int x, int y, int wantsel, ref int nowsel, bool liveonly)
        {
            Actor[] __actors = SnapshotActors();
            int ccy = 0;
            Actor result = null;
            nowsel = -1;
            for (int k = ccy + 8; k >= ccy - 1; k--)
            {
                for (int i = __actors.Length - 1; i >= 0; i--)
                {
                    if (__actors[i] != MShare.MySelf)
                    {
                        Actor a = __actors[i];
                        if ((!liveonly || !a.Death) && a.m_boHoldPlace && a.Visible)
                        {
                            if (a.CurrY == k)
                            {
                                int dx = (a.m_nRx - RobotClient.Map.m_ClientRect.Left) * BotConst.UNITX + _mNDefXx + a.m_nPx + a.m_nShiftX;
                                int dy = (a.m_nRy - RobotClient.Map.m_ClientRect.Top - 1) * BotConst.UNITY + _mNDefYy + a.m_nPy + a.m_nShiftY;
                                if (a.CheckSelect(x - dx, y - dy))
                                {
                                    result = a;
                                    nowsel++;
                                    if (nowsel >= wantsel)
                                    {
                                        return result;
                                    }
                                }
                            }
                        }
                    }
                }
            }
            return result;
        }

        // 取得鼠标所指坐标的角色
        public Actor GetAttackFocusCharacter(int x, int y, int wantsel, ref int nowsel, bool liveonly)
        {
            Actor[] __actors = SnapshotActors();
            int ccy = 0;
            Actor a;
            Actor result = GetCharacter(x, y, wantsel, ref nowsel, liveonly);
            if (result == null)
            {
                nowsel = -1;
                for (int k = ccy + 8; k >= ccy - 1; k--)
                {
                    for (int i = __actors.Length - 1; i >= 0; i--)
                    {
                        if (__actors[i] != MShare.MySelf)
                        {
                            a = __actors[i];
                            if ((!liveonly || !a.Death) && a.m_boHoldPlace && a.Visible)
                            {
                                if (a.CurrY == k)
                                {
                                    int dx = (a.m_nRx - RobotClient.Map.m_ClientRect.Left) * BotConst.UNITX + _mNDefXx + a.m_nPx + a.m_nShiftX;
                                    int dy = (a.m_nRy - RobotClient.Map.m_ClientRect.Top - 1) * BotConst.UNITY + _mNDefYy + a.m_nPy + a.m_nShiftY;
                                    int centx;
                                    if (a.CharWidth() > 40)
                                    {
                                        centx = (a.CharWidth() - 40) / 2;
                                    }
                                    else
                                    {
                                        centx = 0;
                                    }

                                    int centy;
                                    if (a.CharHeight() > 70)
                                    {
                                        centy = (a.CharHeight() - 70) / 2;
                                    }
                                    else
                                    {
                                        centy = 0;
                                    }
                                    if ((x - dx >= centx) && (x - dx <= a.CharWidth() - centx) && (y - dy >= centy) && (y - dy <= a.CharHeight() - centy))
                                    {
                                        result = a;
                                        nowsel++;
                                        if (nowsel >= wantsel)
                                        {
                                            return result;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            return result;
        }

        public bool IsSelectMyself(int x, int y)
        {
            int ccy = 0;
            bool result = false;
            for (int k = ccy + 2; k >= ccy - 1; k--)
            {
                if (MShare.MySelf.CurrY == k)
                {
                    int dx = (MShare.MySelf.m_nRx - RobotClient.Map.m_ClientRect.Left) * BotConst.UNITX + _mNDefXx + MShare.MySelf.m_nPx + MShare.MySelf.m_nShiftX;
                    int dy = (MShare.MySelf.m_nRy - RobotClient.Map.m_ClientRect.Top - 1) * BotConst.UNITY + _mNDefYy + MShare.MySelf.m_nPy + MShare.MySelf.m_nShiftY;
                    if (MShare.MySelf.CheckSelect(x - dx, y - dy))
                    {
                        result = true;
                        return result;
                    }
                }
            }
            return result;
        }

        public TDropItem GetDropItems(int x, int y, ref string inames)
        {
            int ccx = 0;
            int ccy = 0;
            TDropItem result = null;
            inames = "";
            for (int i = 0; i < MShare.g_DropedItemList.Count; i++)
            {
                TDropItem dropItem = MShare.g_DropedItemList[i];
                if ((dropItem.X == ccx) && (dropItem.Y == ccy))
                {
                    if (result == null)
                    {
                        result = dropItem;
                    }
                    inames = inames + dropItem.Name + "\\";
                }
            }
            return result;
        }

        public void GetXyDropItemsList(int nX, int nY, ref ArrayList itemList)
        {
            for (int i = 0; i < MShare.g_DropedItemList.Count; i++)
            {
                TDropItem dropItem = MShare.g_DropedItemList[i];
                if ((dropItem.X == nX) && (dropItem.Y == nY))
                {
                    itemList.Add(dropItem);
                }
            }
        }

        public TDropItem GetXyDropItems(int nX, int nY)
        {
            TDropItem result = null;
            for (int i = 0; i < MShare.g_DropedItemList.Count; i++)
            {
                TDropItem dropItem = MShare.g_DropedItemList[i];
                if ((dropItem.X == nX) && (dropItem.Y == nY))
                {
                    result = dropItem;
                    if (MShare.PickUpAll || dropItem.boPickUp)
                    {
                        break;
                    }
                }
            }
            return result;
        }

        public bool CanRun(short sX, short sY, int ex, int ey)
        {
            bool result;
            byte ndir = BotHelper.GetNextDirection(sX, sY, ex, ey);
            short rx = sX;
            short ry = sY;
            BotHelper.GetNextPosXy(ndir, ref rx, ref ry);
            if (CanWalkEx(rx, ry) && CanWalkEx(ex, ey))
            {
                result = true;
            }
            else
            {
                result = false;
            }
            return result;
        }

        public bool CanWalkEx(int mx, int my)
        {
            bool result = false;
            if (RobotClient.Map.CanMove(mx, my))
            {
                result = !CrashManEx(mx, my);
            }
            return result;
        }

        private bool CrashManEx(int mx, int my)
        {
            Actor[] __actors = SnapshotActors();
            bool result = false;
            for (int i = 0; i < __actors.Length; i++)
            {
                Actor actor = __actors[i];
                if (actor == MShare.MySelf)
                {
                    continue;
                }
                if (actor.Visible && actor.m_boHoldPlace && (!actor.Death) && (actor.CurrX == mx) && (actor.CurrY == my))
                {
                    if ((MShare.MySelf.m_nTagX == 0) && (MShare.MySelf.m_nTagY == 0))
                    {
                        if ((actor.Race == ActorRace.Play) && (MShare.g_boCanRunHuman || MShare.g_boCanRunSafeZone))
                        {
                            continue;
                        }
                        if ((actor.Race == ActorRace.Merchant) && MShare.g_boCanRunNpc)
                        {
                            continue;
                        }
                        if ((actor.Race > ActorRace.Play) && (actor.Race != ActorRace.Merchant) && (MShare.g_boCanRunMon || MShare.g_boCanRunSafeZone))
                        {
                            continue;
                        }
                    }
                    result = true;
                    break;
                }
            }
            return result;
        }

        public bool CanWalk(int mx, int my)
        {
            bool result = false;
            if (RobotClient.Map.CanMove(mx, my))
            {
                result = !CrashMan(mx, my);
            }
            return result;
        }

        public bool CrashMan(int mx, int my)
        {
            Actor[] __actors = SnapshotActors();
            bool result = false;
            for (int i = 0; i < __actors.Length; i++)
            {
                Actor actor = __actors[i];
                if (actor == null || actor == MShare.MySelf)
                {
                    continue;
                }
                if (actor.Visible && actor.m_boHoldPlace && (!actor.Death) && (actor.CurrX == mx) && (actor.CurrY == my))
                {
                    result = true;
                    break;
                }
            }
            return result;
        }

        public bool CanFly(int mx, int my)
        {
            return RobotClient.Map.CanFly(mx, my);
        }

        public Actor FindActor(int id)
        {
            Actor[] __actors = SnapshotActors();
            Actor result = null;
            if (id == 0)
            {
                return result;
            }
            for (int i = 0; i < __actors.Length; i++)
            {
                if (__actors[i].RecogId == id)
                {
                    result = __actors[i];
                    break;
                }
            }
            return result;
        }

        public Actor FindActor(string sName)
        {
            Actor[] __actors = SnapshotActors();
            Actor result = null;
            for (int i = 0; i < __actors.Length; i++)
            {
                Actor actor = __actors[i];
                if (string.Compare(actor.UserName, sName, StringComparison.OrdinalIgnoreCase) == 0)
                {
                    result = actor;
                    break;
                }
            }
            return result;
        }

        public Actor FindActorXY(int x, int y)
        {
            Actor[] __actors = SnapshotActors();
            Actor result = null;
            for (int i = 0; i < __actors.Length; i++)
            {
                Actor a = __actors[i];
                if ((a.CurrX == x) && (a.CurrY == y))
                {
                    result = a;
                    if (!result.Death && result.Visible && result.m_boHoldPlace)
                    {
                        break;
                    }
                }
            }
            return result;
        }

        public bool IsValidActor(Actor actor)
        {
            Actor[] __actors = SnapshotActors();
            bool result = false;
            for (int i = 0; i < __actors.Length; i++)
            {
                if (__actors[i] == actor)
                {
                    result = true;
                    break;
                }
            }
            return result;
        }

        public byte RACEfeature(int cfeature)
        {
            return (byte)cfeature;
        }

        public Actor NewActor(int chrid, int cx, int cy, ushort cdir, int cfeature, int cstate)
        {
            lock (_actorSync)
            {
            Actor actor;
            Actor result = null;
            for (int i = 0; i < ActorList.Count; i++)
            {
                if (ActorList[i].RecogId == chrid)
                {
                    result = ActorList[i];
                    return result;
                }
            }
            if (BotHelper.IsChangingFace(chrid))
            {
                return result;
            }
            switch (RACEfeature(cfeature))
            {
                case 0:
                    actor = new THumActor(RobotClient);
                    break;
                case 9:
                    actor = new TSoccerBall(RobotClient);
                    break;
                case 13:
                    actor = new TKillingHerb(RobotClient);
                    break;
                case 14:
                    actor = new TSkeletonOma(RobotClient);
                    break;
                case 15:
                    actor = new TDualAxeOma(RobotClient);
                    break;
                case 16:
                    actor = new TGasKuDeGi(RobotClient);
                    break;
                case 17:
                    actor = new TCatMon(RobotClient);
                    break;
                case 18:
                    actor = new THuSuABi(RobotClient);
                    break;
                case 19:
                    actor = new TCatMon(RobotClient);
                    break;
                case 20:
                    actor = new TFireCowFaceMon(RobotClient);
                    break;
                case 21:
                    actor = new TCowFaceKing(RobotClient);
                    break;
                case 22:
                    actor = new TDualAxeOma(RobotClient);
                    break;
                case 23:
                    actor = new TWhiteSkeleton(RobotClient);
                    break;
                case 24:
                    actor = new TSuperiorGuard(RobotClient);
                    break;
                case 25:
                    actor = new TKingOfSculpureKingMon(RobotClient);
                    break;
                case 26:
                    actor = new TKingOfSculpureKingMon(RobotClient);
                    break;
                case 27:
                    actor = new TSnowMon(RobotClient);
                    break;
                case 28:
                    actor = new TSnowMon(RobotClient);
                    break;
                case 29:
                    actor = new TSnowMon(RobotClient);
                    break;
                case 30:
                    actor = new TCatMon(RobotClient);
                    break;
                case 31:
                    actor = new TCatMon(RobotClient);
                    break;
                case 32:
                    actor = new TScorpionMon(RobotClient);
                    break;
                case 33:
                    actor = new TCentipedeKingMon(RobotClient);
                    break;
                case 34:
                    actor = new TBigHeartMon(RobotClient);
                    break;
                case 35:
                    actor = new TSpiderHouseMon(RobotClient);
                    break;
                case 36:
                    actor = new TExplosionSpider(RobotClient);
                    break;
                case 37:
                    actor = new TFlyingSpider(RobotClient);
                    break;
                case 38:
                    actor = new TSnowMon(RobotClient);
                    break;
                case 39:
                    actor = new TSnowMon(RobotClient);
                    break;
                case 40:
                    actor = new TZombiLighting(RobotClient);
                    break;
                case 41:
                    actor = new TZombiDigOut(RobotClient);
                    break;
                case 42:
                    actor = new TZombiZilkin(RobotClient);
                    break;
                case 43:
                    actor = new TBeeQueen(RobotClient);
                    break;
                case 44:
                    actor = new TSnowMon(RobotClient);
                    break;
                case 45:
                    actor = new TArcherMon(RobotClient);
                    break;
                case 46:
                    actor = new TSnowMon(RobotClient);
                    break;
                case 47:
                    actor = new TSculptureMon(RobotClient);
                    break;
                case 48:
                    actor = new TSculptureMon(RobotClient);
                    break;
                case 49:
                    actor = new TSculptureKingMon(RobotClient);
                    break;
                case 50:
                    actor = new TNpcActor(RobotClient);
                    break;
                case 51:
                    actor = new TSnowMon(RobotClient);
                    break;
                case 52:
                    actor = new TGasKuDeGi(RobotClient);
                    break;
                case 53:
                    actor = new TGasKuDeGi(RobotClient);
                    break;
                case 54:
                    actor = new TSmallElfMonster(RobotClient);
                    break;
                case 55:
                    actor = new TWarriorElfMonster(RobotClient);
                    break;
                case 56:
                    actor = new TAngel(RobotClient);
                    break;
                case 57:
                    actor = new TDualAxeOma(RobotClient);
                    break;
                case 58:
                    actor = new TDualAxeOma(RobotClient);
                    break;
                case 60:
                    actor = new TElectronicScolpionMon(RobotClient);
                    break;
                case 61:
                    actor = new TBossPigMon(RobotClient);
                    break;
                case 62:
                    actor = new TKingOfSculpureKingMon(RobotClient);
                    break;
                case 63:
                    actor = new TSkeletonKingMon(RobotClient);
                    break;
                case 64:
                    actor = new TGasKuDeGi(RobotClient);
                    break;
                case 65:
                    actor = new TSamuraiMon(RobotClient);
                    break;
                case 66:
                    actor = new TSkeletonSoldierMon(RobotClient);
                    break;
                case 67:
                    actor = new TSkeletonSoldierMon(RobotClient);
                    break;
                case 68:
                    actor = new TSkeletonSoldierMon(RobotClient);
                    break;
                case 69:
                    actor = new TSkeletonArcherMon(RobotClient);
                    break;
                case 70:
                    actor = new TBanyaGuardMon(RobotClient);
                    break;
                case 71:
                    actor = new TBanyaGuardMon(RobotClient);
                    break;
                case 72:
                    actor = new TBanyaGuardMon(RobotClient);
                    break;
                case 73:
                    actor = new TPBOMA1Mon(RobotClient);
                    break;
                case 74:
                    actor = new TCatMon(RobotClient);
                    break;
                case 75:
                    actor = new TStoneMonster(RobotClient);
                    break;
                case 76:
                    actor = new TSuperiorGuard(RobotClient);
                    break;
                case 77:
                    actor = new TStoneMonster(RobotClient);
                    break;
                case 78:
                    actor = new TBanyaGuardMon(RobotClient);
                    break;
                case 79:
                    actor = new TPBOMA6Mon(RobotClient);
                    break;
                case 80:
                    actor = new TMineMon(RobotClient);
                    break;
                case 81:
                    actor = new TAngel(RobotClient);
                    break;
                case 83:
                    actor = new TFireDragon(RobotClient);
                    break;
                case 84:
                    actor = new TDragonStatue(RobotClient);
                    break;
                case 87:
                    actor = new TDragonStatue(RobotClient);
                    break;
                case 90:
                    actor = new TDragonBody(RobotClient);
                    break;
                case 91:
                    actor = new TWhiteSkeleton(RobotClient);
                    break;
                case 92:
                    actor = new TWhiteSkeleton(RobotClient);
                    break;
                case 93:
                    actor = new TWhiteSkeleton(RobotClient);
                    break;
                case 94:
                    actor = new TWarriorElfMonster(RobotClient);
                    break;
                case 95:
                    actor = new TWarriorElfMonster(RobotClient);
                    break;
                case 98:
                    actor = new TWallStructure(RobotClient);
                    break;
                case 99:
                    actor = new TCastleDoor(RobotClient);
                    break;
                case 101:
                    actor = new TBanyaGuardMon(RobotClient);
                    break;
                case 102:
                    actor = new TKhazardMon(RobotClient);
                    break;
                case 103:
                    actor = new TFrostTiger(RobotClient);
                    break;
                case 104:
                    actor = new TRedThunderZuma(RobotClient);
                    break;
                case 105:
                    actor = new TCrystalSpider(RobotClient);
                    break;
                case 106:
                    actor = new TYimoogi(RobotClient);
                    break;
                case 109:
                    actor = new TBlackFox(RobotClient);
                    break;
                case 110:
                    actor = new TGreenCrystalSpider(RobotClient);
                    break;
                case 111:
                    actor = new TBanyaGuardMon(RobotClient);
                    break;
                case 113:
                    actor = new TBanyaGuardMon(RobotClient);
                    break;
                case 114:
                    actor = new TBanyaGuardMon(RobotClient);
                    break;
                case 115:
                    actor = new TBanyaGuardMon(RobotClient);
                    break;
                case 117:
                case 118:
                case 119:
                    actor = new TBanyaGuardMon(RobotClient);
                    break;
                case 120:
                    actor = new TFireDragon(RobotClient);
                    break;
                case 121:
                    actor = new TTiger(RobotClient);
                    break;
                case 122:
                    actor = new TDragon(RobotClient);
                    break;
                case 123:
                    actor = new TGhostShipMonster(RobotClient);
                    break;
                default:
                    actor = new Actor(RobotClient);
                    break;
            }
            actor.RecogId = chrid;
            actor.CurrX = (short)(ushort)(short)cx;
            actor.CurrY = (short)(ushort)(short)cy;
            actor.m_nRx = actor.CurrX;
            actor.m_nRy = actor.CurrY;
            actor.m_btDir = (byte)cdir;
            actor.m_nFeature = cfeature;
            if (MShare.OpenAutoPlay && MShare.g_gcAss[6])
            {
                // actor.m_btAFilter = MShare.g_APMobList.IndexOf(actor.m_sUserName) >= 0;
            }
            actor.Race = RACEfeature(cfeature);
            actor.m_Action = null;
            actor.m_nState = cstate;
            ActorList.Add(actor);
            result = actor;
            return result;

            }}

        public void ActorDied(Actor actor)
        {
            lock (_actorSync)
            {
            // F1b「顺带核」：ActorDied 在 BeginScene 的 ProcMsg 里由 run 线程调用，收包线程仍可能
            // 并发增删 ActorList ⇒ 读下标/RemoveAt/Insert 都可能 AOORE。按删除路径同一模式：
            // 读写各自自护、失败即计数跳过；插入失败回落 Add（不抛给 SafeRun）。
            for (int i = 0; i < ActorList.Count; i++)
            {
                Actor cur;
                try
                {
                    cur = ActorList[i];
                }
                catch (ArgumentOutOfRangeException)
                {
                    LoadMetrics.SceneListRaceSkipped();
                    break;
                }
                if (cur == actor)
                {
                    SceneActorDiedRaceHook?.Invoke(this);
                    try
                    {
                        ActorList.RemoveAt(i);
                    }
                    catch (ArgumentOutOfRangeException)
                    {
                        LoadMetrics.SceneListRaceSkipped();
                    }
                    break;
                }
            }
            // P3-2：RemoveAt 可能因并发失败而 actor 仍在列表里；先判在列表则不再 Insert/Add，
            // 避免重复插入（夹具层面）。Contains 读并发失败时保守按「仍在列表」处理。
            bool present = false;
            try
            {
                present = ActorList.Contains(actor);
            }
            catch (ArgumentOutOfRangeException)
            {
                present = true;
                LoadMetrics.SceneListRaceSkipped();
            }
            bool flag = false;
            if (!present)
            {
                for (int i = 0; i < ActorList.Count; i++)
                {
                    Actor cur;
                    try
                    {
                        cur = ActorList[i];
                    }
                    catch (ArgumentOutOfRangeException)
                    {
                        LoadMetrics.SceneListRaceSkipped();
                        break;
                    }
                    if (cur == null)
                    {
                        // P3-1：空槽位不能解引用；跳过并计数，继续找插入点。
                        LoadMetrics.SceneNullActorSkipped();
                        continue;
                    }
                    if (!cur.Death)
                    {
                        try
                        {
                            ActorList.Insert(i, actor);
                            flag = true;
                        }
                        catch (ArgumentOutOfRangeException)
                        {
                            LoadMetrics.SceneListRaceSkipped();
                        }
                        break;
                    }
                }
                if (!flag)
                {
                    ActorList.Add(actor);
                }
            }

            }}

        public void SetActorDrawLevel(Actor actor, int level)
        {
            lock (_actorSync)
            {
            if (level == 0)
            {
                for (int i = 0; i < ActorList.Count; i++)
                {
                    if (ActorList[i] == actor)
                    {
                        ActorList.RemoveAt(i);
                        ActorList.Insert(0, actor);
                        break;
                    }
                }
            }

            }}

        // 清除所有角色
        public void ClearActors()
        {
            lock (_actorSync)
            {
            // 冻结客户端还会 Free 并清空 g_FreeActorList（PlayScn.pas:3457-3459）；本端口没有自由池
            // （已移除，见类顶部说明），清空 ActorList 即等价：所有 actor 的最后一个引用归零。
            ActorList.Clear();
            MShare.MySelf = null;
            RobotClient.MyActor = null;
            MShare.TargetCret = null;
            MShare.FocusCret = null;
            MShare.MagicTarget = null;

            }}

        public Actor DeleteActor(int id, bool boDeath)
        {
            lock (_actorSync)
            {
            Actor result = null;
            int i = 0;
            while (true)
            {
                if (i >= ActorList.Count)
                {
                    break;
                }
                if (ActorList[i].RecogId == id)
                {
                    if (MShare.TargetCret == ActorList[i])
                    {
                        MShare.TargetCret = null;
                    }
                    if (MShare.FocusCret == ActorList[i])
                    {
                        MShare.FocusCret = null;
                    }
                    if (MShare.MagicTarget == ActorList[i])
                    {
                        MShare.MagicTarget = null;
                    }
                    if (IsMySlaveObject(ActorList[i]))
                    {
                        if (!boDeath)
                        {
                            break;
                        }
                    }
                    ActorList[i].m_dwDeleteTime = MShare.GetTickCount();
                    ActorList.RemoveAt(i);
                    LoadMetrics.SceneActorEjected();
                }
                else
                {
                    i++;
                }
            }
            return result;

            }}

        public Actor DeleteActor(int id)
        {
            return DeleteActor(id, false);
        }

        public void DelActor(object actor)
        {
            lock (_actorSync)
            {
            for (int i = 0; i < ActorList.Count; i++)
            {
                if (ActorList[i] == actor)
                {
                    ActorList[i].m_dwDeleteTime = MShare.GetTickCount();
                    ActorList.RemoveAt(i);
                    LoadMetrics.SceneActorEjected();
                    break;
                }
            }

            }}

        public Actor ButchAnimal(int x, int y)
        {
            Actor[] __actors = SnapshotActors();
            Actor result = null;
            for (int i = 0; i < __actors.Length; i++)
            {
                Actor a = __actors[i];
                if (a.Death)
                {
                    if ((Math.Abs(a.CurrX - x) <= 1) && (Math.Abs(a.CurrY - y) <= 1))
                    {
                        result = a;
                        break;
                    }
                }
            }
            return result;
        }

        public void SendMsg(int ident, int chrid, ushort x, ushort y, byte cdir, int feature, int state, string str, int ipInfo = 0)
        {
            Actor actor;
            MessageBodyW mbw;
            switch (ident)
            {
                case Messages.SM_CHANGEMAP:
                case Messages.SM_NEWMAP:
                    ProcMagic.NTargetX = -1;
                    //robotClient.EventMan.ClearEvents();
                    RobotClient.GPathBusy = true;
                    try
                    {
                        if (RobotClient.TimerAutoMove != null)
                        {
                            if (RobotClient.TimerAutoMove.Enabled)
                            {
                                RobotClient.TimerAutoMove.Enabled = false;
                                PathMap.g_MapPath = new Point[0];
                                PathMap.g_MapPath = null;
                                ScreenManager.AddChatBoardString("地图跳转，停止自动移动");
                            }
                            if (MShare.OpenAutoPlay && RobotClient.TimerAutoPlay.Enabled)
                            {
                                RobotClient.TimerAutoPlay.Enabled = false;
                                MShare.g_gcAss[0] = false;
                                MShare.MapPath = new Point[0];
                                MShare.g_APMapPath2 = new Point[0];
                                MShare.AutoStep = -1;
                                MShare.AutoLastPoint.X = -1;
                                ScreenManager.AddChatBoardString("[挂机] 地图跳转，停止自动挂机");
                            }
                        }
                        if (MShare.MySelf != null)
                        {
                            MShare.MySelf.m_nTagX = 0;
                            MShare.MySelf.m_nTagY = 0;
                        }
                        if (RobotClient.Map.m_MapBuf != null)
                        {
                            //FreeMem(robotClient.Map.m_MapBuf);
                            RobotClient.Map.m_MapBuf = null;
                        }
                        if (RobotClient.Map.m_MapData != null)
                        {
                            RobotClient.Map.m_MapData = new CellParams[0, 0];
                            RobotClient.Map.m_MapData = null;
                        }
                    }
                    finally
                    {
                        RobotClient.GPathBusy = false;
                    }
                    RobotClient.Map.LoadMap(str, x, y);
                    if ((ident == Messages.SM_NEWMAP) && (MShare.MySelf != null))
                    {
                        MShare.MySelf.CurrX = (short)x;
                        MShare.MySelf.CurrY = (short)y;
                        MShare.MySelf.m_nRx = (short)x;
                        MShare.MySelf.m_nRy = (short)y;
                        DelActor(MShare.MySelf);
                    }
                    break;
                case Messages.SM_LOGON:
                    actor = FindActor(chrid);
                    if (actor == null)
                    {
                        actor = NewActor(chrid, x, y, HUtil32.LoByte(cdir), feature, state);
                        actor.m_nChrLight = HUtil32.HiByte(cdir);
                        cdir = HUtil32.LoByte(cdir);
                        actor.SendMsg(Messages.SM_TURN, x, y, cdir, feature, state, "", 0);
                    }
                    // per-bot 自身 actor（卡 `openmir2-botsrv-shared-state-locks`）：不再写跨 bot
                    // 全局静态；本线程的 `MShare.MySelf` 上下文由 ClientManager per-packet/per-round
                    // 设置，这里同时把权威引用挂到本假人实例上。
                    if (RobotClient.MyActor != null)
                    {
                        RobotClient.MyActor.SlaveObject.Clear();
                    }
                    RobotClient.MyActor = (THumActor)actor;
                    MShare.MySelf = RobotClient.MyActor;
                    break;
                case Messages.SM_HIDE:
                    actor = FindActor(chrid);
                    if (actor != null)
                    {
                        if (actor.m_boDelActionAfterFinished)
                        {
                            return;
                        }
                        if (actor.m_nWaitForRecogId != 0)
                        {
                            return;
                        }
                        if (IsMySlaveObject(actor))
                        {
                            if ((cdir != 0) || actor.Death)
                            {
                                DeleteActor(chrid, true);
                            }
                            return;
                        }
                    }
                    DeleteActor(chrid);
                    break;
                default:
                    actor = FindActor(chrid);
                    if ((ident == Messages.SM_TURN) || (ident == Messages.SM_RUN) || (ident == Messages.SM_HORSERUN) || (ident == Messages.SM_WALK) || (ident == Messages.SM_BACKSTEP) || (ident == Messages.SM_DEATH) || (ident == Messages.SM_SKELETON) || (ident == Messages.SM_DIGUP) || (ident == Messages.SM_ALIVE))
                    {
                        if (actor == null)
                        {
                            actor = NewActor(chrid, x, y, HUtil32.LoByte(cdir), feature, state);
                        }
                        if (actor != null)
                        {
                            if (ipInfo != 0)
                            {
                                actor.m_nIPowerLvl = HUtil32.HiWord(ipInfo);
                            }
                            actor.m_nChrLight = HUtil32.HiByte(cdir);
                            cdir = HUtil32.LoByte(cdir);
                            if (ident == Messages.SM_SKELETON)
                            {
                                actor.Death = true;
                                actor.m_boSkeleton = true;
                            }
                            if (ident == Messages.SM_DEATH)
                            {
                                if (HUtil32.HiByte(cdir) != 0)
                                {
                                    actor.m_boItemExplore = true;
                                }
                            }
                        }
                    }
                    if (actor == null)
                    {
                        return;
                    }
                    switch (ident)
                    {
                        case Messages.SM_FEATURECHANGED:
                            actor.m_nFeature = feature;
                            actor.m_nFeatureEx = state;
                            if (!string.IsNullOrEmpty(str))
                            {
                                mbw = EDCode.DecodeBuffer<MessageBodyW>(str);
                                actor.m_btTitleIndex = (byte)HUtil32.LoWord(mbw.Param1);
                            }
                            else
                            {
                                actor.m_btTitleIndex = 0;
                            }
                            actor.FeatureChanged();
                            break;
                        case Messages.SM_CHARSTATUSCHANGED:
                            actor.m_nState = feature;
                            actor.HitSpeed = (ushort)state;
                            break;
                        default:
                            if (ident == Messages.SM_TURN)
                            {
                                if (!string.IsNullOrEmpty(str))
                                {
                                    actor.UserName = str;
                                }
                            }
                            actor.SendMsg(ident, x, y, cdir, feature, state, "", 0);
                            break;
                    }
                    break;
            }
        }

        public static bool IsMySlaveObject(Actor atc)
        {
            bool result = false;
            if (MShare.MySelf == null)
            {
                return result;
            }
            for (int i = 0; i < MShare.MySelf.SlaveObject.Count; i++)
            {
                if (atc == MShare.MySelf.SlaveObject[i])
                {
                    result = true;
                    break;
                }
            }
            return result;
        }
    }
}
