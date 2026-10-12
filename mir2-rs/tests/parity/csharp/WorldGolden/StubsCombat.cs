// M2 下半程：战斗 / 掉落 / 经验 —— 逐字拷贝（行号标注在各函数上方）。
// 时钟与随机源复用 `ParityGolden.M2Share.RandomNumber`（同一个种子化实例 ⇒ 与 Rust 侧同一调用顺序）。

using System;
using System.Collections.Generic;

namespace WorldGolden
{
    /// <summary>`UserItem` 的战斗子集（`ClientUserItem.cs:14`）：`PlayObject.StruckDamage` 的耐久循环只读这两个字段。</summary>
    public sealed class UseItemStub
    {
        public ushort Index;
        public ushort Dura;
    }

    /// <summary>`BaseObject` 的 WAbil 子集（战斗数值）。</summary>
    public class AbilityStub
    {
        public ushort HP;
        public ushort MaxHP;
        public int DC; // 低字节 = 下限、高字节 = 上限（与 C# MakeWord 一致）
        public int AC;
        public byte Level = 1;
        public int Exp;
        public int MaxExp;
    }

    /// <summary>战斗配置子集（默认值 = GameSvrConf 初值；现网 Server.conf 未改时即为默认）。</summary>
    public static class CombatConfigStub
    {
        public const int WarrMon = 10;
        public const int WizardMon = 10;
        public const int TaosMon = 10;
        public const int MonHum = 10;
        /// <summary>`Config.HighLevelKillMonFixExp`（现网 Exps.conf = 1；场景可覆写以钉住调用口径）。</summary>
        public static bool HighLevelKillMonFixExp = true;
        public const int KillMonExpMultiple = 1;
        public const int MnKillMonExpMultiple = 1;
        public const int KillMonExpRate = 100;

        public static int Round(double r) => (int)Math.Round(r, MidpointRounding.AwayFromZero);
    }

    public partial class ActorStub
    {
        public AbilityStub WAbil = new AbilityStub();
        public byte HitPoint;
        public byte SpeedPoint;
        public byte Job;
        public byte LifeAttrib;  // 1 = LA_UNDEAD
        public int UndeadPower;  // AddAbil.UndeadPower
        public ActorStub LastHiter;
        public ActorStub ExpHitter;                   // 经验归属者（BaseObject.cs:296）
        public int FightExp;                          // AnimalObject.FightExp = 怪物经验（WorldServer.MonGen.cs:893）
        public int Gold;                              // mon.Gold（掉落预算）
        public List<ParityGolden.UserItem> ItemList;  // mon.ItemList（掉落预算）
        public bool DeathSettled;                     // 本骨架内部：死亡结算只做一次
        public int MapId;

        // ---------- 玩家攻方取数（`PlayObject.GetAttackPower` override）与 `PlayObject.StruckDamage` 子集 ----------
        // `PlayObject.cs:62` / `PlayObject.cs:724` / `PlayObject.cs:199` / `PlayObject.cs:223`。
        public int Luck;                              // RecalcAbilitys：无装备 ⇒ 0（`PlayObject.cs:2011-2013`）
        public int PowerRate = 100;                   // `PlayObject.cs:943`
        public bool BoPowerItem;                      // `PlayObject.cs:223`
        public byte PowerItem;                        // `PlayObject.cs:199`
        // `CharacterObject.UseItems`（`CharacterObject.cs:40`，`CharacterObject.cs:89` 初始化为 13 格）。
        public UseItemStub[] UseItems = new UseItemStub[13];

        /// <summary>src/OpenMir2/HUtil32.cs: LoByte / HiByte。</summary>
        public static byte LoByte(int w) => (byte)w;

        public static byte HiByte(int w) => (byte)(w >> 8);

        // ---------- src/M2Server/Actor/BaseObject.cs:1753 逐字拷贝（时间戳两行不在本子集） ----------
        public void SetLastHiter(ActorStub baseObject)
        {
            LastHiter = baseObject;
            if (ExpHitter == null)
            {
                ExpHitter = baseObject;
            }
        }

        // ---------- src/M2Server/Player/PlayObject.cs:2909 逐字拷贝（玩家 override） ----------
        // `PlayObject.GetAttackPower`：`Luck <= 0` 时比基版**多一次** `Random(10)`；再乘 `PowerRate`/`PowerItem`。
        public int GetAttackPowerPlayer(int nBasePower, int nPower)
        {
            if (nPower < 0)
            {
                nPower = 0;
            }
            int result;
            if (Luck > 0)
            {
                if (ParityGolden.M2Share.RandomNumber.Random(10 - Math.Min(9, Luck)) == 0)
                {
                    result = nBasePower + nPower;
                }
                else
                {
                    result = nBasePower + ParityGolden.M2Share.RandomNumber.Random(nPower + 1);
                }
            }
            else
            {
                result = nBasePower + ParityGolden.M2Share.RandomNumber.Random(nPower + 1);
                if (Luck <= 0)
                {
                    if (ParityGolden.M2Share.RandomNumber.Random(10 - Math.Max(0, -Luck)) == 0)
                    {
                        result = nBasePower;
                    }
                }
            }
            result = CombatConfigStub.Round(result * (PowerRate / 100.0));
            if (BoPowerItem)
            {
                result = CombatConfigStub.Round(PowerItem * (double)result);
            }
            return result;
        }

        // ---------- src/M2Server/Actor/BaseObject.Base.cs:886 逐字拷贝 + 虚分派 ----------
        // oracle 的 `GetBaseAttackPoewr` 调**虚函数** `GetAttackPower`：`PlayObject` 走 override
        //（`PlayObject.cs:2909`），`MonsterObject` 无 override ⇒ 走基版。本桩用 `Race` 分派
        //（`ActorRace.Play = 0`；与 `IsProperTarget`/AOI 的同一判据）。
        public virtual int GetAttackPower(int basePower, int power)
        {
            if (Race == 0)
            {
                return GetAttackPowerPlayer(basePower, power);
            }
            if (power < 0)
            {
                power = 0;
            }
            int result = basePower + ParityGolden.M2Share.RandomNumber.Random(power + 1);
            return result;
        }

        // ---------- src/M2Server/Actor/BaseObject.Attack.cs:25 逐字拷贝 ----------
        internal int GetBaseAttackPoewr()
        {
            return GetAttackPower(LoByte(WAbil.DC), (sbyte)(HiByte(WAbil.DC) - LoByte(WAbil.DC)));
        }

        // ---------- src/M2Server/Actor/BaseObject.Base.cs:789 逐字拷贝 ----------
        public virtual ushort GetHitStruckDamage(ActorStub target, int nDamage)
        {
            int nArmor;
            int nRnd = LoByte(WAbil.AC) + ParityGolden.M2Share.RandomNumber.Random(Math.Abs(HiByte(WAbil.AC) - LoByte(WAbil.AC)) + 1);
            if (nRnd > 0)
            {
                nArmor = LoByte(WAbil.AC) + ParityGolden.M2Share.RandomNumber.Random(nRnd);
            }
            else
            {
                nArmor = LoByte(WAbil.AC);
            }
            nDamage = Math.Max(0, nDamage - nArmor);
            if (nDamage > 0)
            {
                if ((LifeAttrib == 1) && (target != null))
                {
                    nDamage += target.UndeadPower;
                }
            }
            return (ushort)nDamage;
        }

        // ---------- src/M2Server/Actor/BaseObject.Base.cs:827 逐字拷贝（毒/护盾分支不在本子集） ----------
        // 注：`PlayObject.StruckDamage`（`PlayObject.cs:2954`）在 `base.StruckDamage` 之后**无条件**
        // 跑耐久逻辑（即使基版因 `nDamage <= 0` 早退）——故这里把 override 部分放在早退之外。
        public virtual void StruckDamage(int nDamage)
        {
            // 基版（`BaseObject.StruckDamage`，`BaseObject.Base.cs:827`）。
            if (nDamage > 0)
            {
                if ((Race >= 50) && (LastHiter != null) && (LastHiter.Race == 0)) // 人攻击怪物
                {
                    switch (LastHiter.Job)
                    {
                        case 0:
                            nDamage = (ushort)(nDamage * CombatConfigStub.WarrMon / 10);
                            break;
                        case 1:
                            nDamage = (ushort)(nDamage * CombatConfigStub.WizardMon / 10);
                            break;
                        case 2:
                            nDamage = (ushort)(nDamage * CombatConfigStub.TaosMon / 10);
                            break;
                    }
                }
                if ((Race == 0) && (LastHiter != null) && (LastHiter.Master != null)) // 人物下属怪物攻击人
                {
                    nDamage = (ushort)(nDamage * CombatConfigStub.MonHum / 10);
                }
                DamageHealth(nDamage);
            }

            // 玩家 override（`PlayObject.StruckDamage`，`PlayObject.cs:2954-3039`）部分。
            if (Race == 0)
            {
                ushort nDam = (ushort)(ParityGolden.M2Share.RandomNumber.Random(10) + 5);
                // `StatusTimeArr[PoisonState.DAMAGEARMOR] > 0` 的 1.2 倍分支：本骨架无 buff 状态。
                // 衣服位（`UseItems[ItemLocation.Dress = 0]`）特判（`PlayObject.cs:2960-2996`）**不抽数**，只扣耐久。
                for (int i = 0; i < UseItems.Length; i++)
                {
                    if ((UseItems[i] != null) && (UseItems[i].Index > 0)
                        && (ParityGolden.M2Share.RandomNumber.Random(8) == 0))
                    {
                        ushort nDura = UseItems[i].Dura;
                        nDura -= nDam;
                        UseItems[i].Dura = nDura; // 破坏分支（`SendDelItems`/`FeatureChanged`）不在本子集
                    }
                }
            }
        }

        // ---------- src/M2Server/Actor/BaseObject.cs:775 逐字拷贝（护盾分支需 MP，本子集未启用） ----------
        public void DamageHealth(int nDamage)
        {
            if (nDamage > 0)
            {
                if ((WAbil.HP - nDamage) > 0)
                {
                    WAbil.HP = (ushort)(WAbil.HP - nDamage);
                }
                else
                {
                    WAbil.HP = 0;
                }
            }
            else
            {
                if ((WAbil.HP - nDamage) < WAbil.MaxHP)
                {
                    WAbil.HP = (ushort)(WAbil.HP - nDamage);
                }
                else
                {
                    WAbil.HP = WAbil.MaxHP;
                }
            }
        }

        // ---------- src/M2Server/Actor/BaseObject.Attack.cs:30 逐字拷贝（IsProperTarget 取子集） ----------
        internal bool _Attack(int nPower, ActorStub targetObject)
        {
            if (targetObject == null)
            {
                return false;
            }
            bool result = false;
            if (IsProperTarget(targetObject))
            {
                if (targetObject.HitPoint > 0)
                {
                    if (HitPoint < ParityGolden.M2Share.RandomNumber.RandomByte(targetObject.SpeedPoint))
                    {
                        nPower = 0;
                    }
                }
            }
            else
            {
                nPower = 0;
            }
            if (nPower > 0)
            {
                nPower = targetObject.GetHitStruckDamage(this, nPower);
                if (nPower > 0)
                {
                    targetObject.StruckDamage(nPower);
                    result = true;
                }
            }
            // 注意：`_Attack` 里**没有**命中者赋值（C# 原文如此）。命中者的登记走
            // `SendMsg(targetObject, RM_STRUCK, …)`（`BaseObject.Attack.cs:61-64`，条件是
            // `targetObject.Race > ActorRace.Play`），由目标下一次 `Run()` 里
            // `AnimalObject.Operate`（`AnimalObject.cs:118-124`）调用 `SetLastHiter(攻击方)`。
            // 驱动侧在 `_Attack` 返回后按同一条件补这一步（见 Program.cs）。
            return result;
        }

        /// <summary>攻击合法性子集（与 Rust `is_attack_target` 同口径）。</summary>
        public bool IsProperTarget(ActorStub target)
        {
            if (target == null || ReferenceEquals(target, this))
            {
                return false;
            }
            if (target.Death || target.Ghost || target.Invisible)
            {
                return false;
            }
            if (target.Master == this)
            {
                return false;
            }
            if (Race == 0 && target.Race != 0)
            {
                return true;
            }
            if (Race != 0 && target.Race == 0)
            {
                return true;
            }
            return false;
        }

        // ---------- src/M2Server/Actor/BaseObject.cs:712 逐字拷贝 ----------
        internal int CalcGetExp(int nLevel, int nExp)
        {
            int result;
            if (CombatConfigStub.HighLevelKillMonFixExp || (WAbil.Level < (nLevel + 10)))
            {
                result = nExp;
            }
            else
            {
                result = nExp - CombatConfigStub.Round(nExp / 15.0 * (WAbil.Level - (nLevel + 10.0)));
            }
            if (result <= 0)
            {
                result = 1;
            }
            return result;
        }

        // ---------- src/M2Server/Player/PlayObject.Base.cs:129 WinExp + :149 GetExp（单步升级） ----------
        public int GainExp(int dwExp, int[] needExps)
        {
            if (dwExp > 0)
            {
                dwExp = CombatConfigStub.KillMonExpMultiple * dwExp;
                dwExp = CombatConfigStub.MnKillMonExpMultiple * dwExp;
                dwExp = CombatConfigStub.Round(CombatConfigStub.KillMonExpRate / 100.0 * dwExp);
                WAbil.Exp += dwExp;
                if (WAbil.Exp >= WAbil.MaxExp)
                {
                    WAbil.Exp -= WAbil.MaxExp;
                    if (WAbil.Level < 255)
                    {
                        WAbil.Level++;
                    }
                    WAbil.MaxExp = needExps[WAbil.Level];
                }
            }
            return dwExp; // 实际入账的经验（乘完倍率），供对拍逐行输出
        }

        /// <summary>src/M2Server/Actor/BaseObject.cs:470 `GetDropPosition` 的扫描顺序子集。</summary>
        public static bool GetDropPosition(EnvirnomentStub envir, int nOrgX, int nOrgY, int nRange, out int pX, out int pY)
        {
            pX = 0;
            pY = 0;
            for (int i = 0; i < nRange; i++)
            {
                for (int ii = -i; ii <= i; ii++)
                {
                    for (int iii = -i; iii <= i; iii++)
                    {
                        int x = nOrgX + iii + 1;
                        int y = nOrgY + ii + 1;
                        MapCellInfo cell = envir.GetCellInfo(x, y, out bool ok);
                        if (ok && cell.Count == 0)
                        {
                            pX = x;
                            pY = y;
                            return true;
                        }
                    }
                }
            }
            return false;
        }
    }
}
