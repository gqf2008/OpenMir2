//! 消息号表 —— 由 `cargo run -p msgcodegen` 从 C# 源机械生成，**禁止手改**。
//!
//! 真源：`src/OpenMir2/Messages.cs` + `src/OpenMir2/Grobal2.cs`（白名单常量，在 [`grobal2`] 子模块）。
//! 漂移门禁：`cargo run -p msgcodegen -- --check`（与 C# 源不一致时变红）。
//!
//! 已知坑（服务端为准）：`SM_ATTACKMODE == 213`；客户端分支曾把 213 占给
//! `SM_HERODELMAGIC`（已改 546），本表以服务端编号为唯一判据。

// 常量名与 C# 源逐字一致（含 PascalCase/混写），便于双向检索，故豁免命名 lint。
// 文档注释采自 C# `///`（可空），不强制。
#![allow(non_upper_case_globals)]
#![allow(missing_docs)]
// 字面量保持 C# 源写法（不分位），便于人工对照。
#![allow(clippy::unreadable_literal)]
// names_of 是巨型 match（每个值一行），行数即数据量。
#![allow(clippy::too_many_lines)]

// ==== 其他 ====

pub const DefBlockSize: u8 = 16;
pub const UNKNOWMSG: u16 = 199;

// ==== CM_* — 客户端→服务端 ====

pub const CM_SENDSELL: u16 = 9050;
pub const CM_MYSHOPEXIT: u16 = 9053;
pub const CM_MYSHOPHAM: u16 = 9055;
pub const CM_HAMSHOPBUY: u16 = 9057;
pub const CM_OPHAMSHOP: u16 = 9061;
pub const CM_CHACKITEM: u16 = 9063;
pub const CM_QUERYUSERSTATE: u16 = 82;
pub const CM_QUERYUSERNAME: u16 = 80;
pub const CM_QUERYBAGITEMS: u16 = 81;
pub const CM_QUERYCHR: u16 = 100;
pub const CM_NEWCHR: u16 = 101;
pub const CM_DELCHR: u16 = 102;
pub const CM_SELCHR: u16 = 103;
/// 玩家选择服务器
pub const CM_SELECTSERVER: u16 = 104;
/// 开门
pub const CM_OPENDOOR: u16 = 1002;
pub const CM_SOFTCLOSE: u16 = 1009;
pub const CM_DROPITEM: u16 = 1000;
/// 拾取物品
pub const CM_PICKUP: u16 = 1001;
pub const CM_TAKEONITEM: u16 = 1003;
pub const CM_TAKEOFFITEM: u16 = 1004;
pub const CM_1005: u16 = 1005;
/// 使用武物品
pub const CM_EAT: u16 = 1006;
/// 挖物品
pub const CM_BUTCH: u16 = 1007;
pub const CM_MAGICKEYCHANGE: u16 = 1008;
/// 点击NPC
pub const CM_CLICKNPC: u16 = 1010;
pub const CM_MERCHANTDLGSELECT: u16 = 1011;
pub const CM_MERCHANTQUERYSELLPRICE: u16 = 1012;
pub const CM_USERSELLITEM: u16 = 1013;
pub const CM_USERBUYITEM: u16 = 1014;
pub const CM_USERGETDETAILITEM: u16 = 1015;
pub const CM_DROPGOLD: u16 = 1016;
pub const CM_TEST: u16 = 1017;
/// 检测客户是否有下载好的client
pub const CM_LOGINNOTICEOK: u16 = 1018;
pub const CM_GROUPMODE: u16 = 1019;
pub const CM_CREATEGROUP: u16 = 1020;
pub const CM_ADDGROUPMEMBER: u16 = 1021;
pub const CM_DELGROUPMEMBER: u16 = 1022;
pub const CM_USERREPAIRITEM: u16 = 1023;
pub const CM_MERCHANTQUERYREPAIRCOST: u16 = 1024;
/// 发起交易
pub const CM_DEALTRY: u16 = 1025;
pub const CM_DEALADDITEM: u16 = 1026;
pub const CM_DEALDELITEM: u16 = 1027;
pub const CM_DEALCANCEL: u16 = 1028;
pub const CM_DEALCHGGOLD: u16 = 1029;
pub const CM_DEALEND: u16 = 1030;
pub const CM_USERSTORAGEITEM: u16 = 1031;
pub const CM_USERTAKEBACKSTORAGEITEM: u16 = 1032;
pub const CM_WANTMINIMAP: u16 = 1033;
pub const CM_USERMAKEDRUGITEM: u16 = 1034;
pub const CM_OPENGUILDDLG: u16 = 1035;
pub const CM_GUILDHOME: u16 = 1036;
pub const CM_GUILDMEMBERLIST: u16 = 1037;
pub const CM_GUILDADDMEMBER: u16 = 1038;
pub const CM_GUILDDELMEMBER: u16 = 1039;
pub const CM_GUILDUPDATENOTICE: u16 = 1040;
pub const CM_GUILDUPDATERANKINFO: u16 = 1041;
pub const CM_1042: u16 = 1042;
pub const CM_ADJUST_BONUS: u16 = 1043;
pub const CM_GUILDALLY: u16 = 1044;
pub const CM_GUILDBREAKALLY: u16 = 1045;
pub const CM_SPEEDHACKUSER: u16 = 10430;
pub const CM_PROTOCOL: u16 = 2000;
pub const CM_IDPASSWORD: u16 = 2001;
/// 创建账号
pub const CM_ADDNEWUSER: u16 = 2002;
/// 修改密码
pub const CM_CHANGEPASSWORD: u16 = 2003;
/// 更新账号信息
pub const CM_UPDATEUSER: u16 = 2004;
pub const CM_THROW: u16 = 3005;
pub const CM_TURN: u16 = 3010;
/// 走路
pub const CM_WALK: u16 = 3011;
/// 蹲下
pub const CM_SITDOWN: u16 = 3012;
/// 跑步
pub const CM_RUN: u16 = 3013;
/// 攻击
pub const CM_HIT: u16 = 3014;
pub const CM_HEAVYHIT: u16 = 3015;
pub const CM_BIGHIT: u16 = 3016;
pub const CM_SPELL: u16 = 3017;
pub const CM_POWERHIT: u16 = 3018;
pub const CM_LONGHIT: u16 = 3019;
pub const CM_WIDEHIT: u16 = 3024;
pub const CM_FIREHIT: u16 = 3025;
/// 玩家说话
pub const CM_SAY: u16 = 3030;
pub const CM_SPEEDHACKMSG: u16 = 3500;
pub const CM_CHECKTIME: u16 = 15999;
pub const CM_SERVERREGINFO: u16 = 65074;
pub const CM_GETGAMELIST: u16 = 5001;
pub const CM_CHECKCLIENT_RES: u16 = 41905;
pub const CM_SMUGGLE: u16 = 41902;
pub const CM_SMUGGLE_SUCESS: u16 = 41903;
/// 找回密码
pub const CM_GETBACKPASSWORD: u16 = 5003;
pub const CM_42HIT: u16 = 42;
pub const CM_QUERYVAL: u16 = 1065;
pub const CM_PASSWORD: u16 = 2001;
pub const CM_CHGPASSWORD: u16 = 2002;
pub const CM_SETPASSWORD: u16 = 2004;
pub const CM_HORSERUN: u16 = 3035;
pub const CM_CRSHIT: u16 = 3036;
pub const CM_3037: u16 = 3037;
pub const CM_TWINHIT: u16 = 3038;
pub const CM_QUERYUSERSET: u16 = 3040;
pub const CM_SELLOFFADDITEM: u16 = 23002;
pub const CM_SELLOFFDELITEM: u16 = 23007;
pub const CM_SELLOFFCANCEL: u16 = 23012;
pub const CM_SELLOFFEND: u16 = 23015;
pub const CM_CANCELSELLOFFITEMING: u16 = 23024;
pub const CM_SELLOFFBUYCANCEL: u16 = 23025;
pub const CM_SELLOFFBUY: u16 = 23026;

// ==== SM_* — 服务端→客户端 ====

pub const SM_SENDSELL: u16 = 9051;
pub const SM_MYSHOPEXIT: u16 = 9054;
pub const SM_MYSHOPHAM: u16 = 9056;
pub const SM_HAMSHOPBUYA: u16 = 9058;
pub const SM_HAMSHOPBUYB: u16 = 9059;
pub const SM_HAMSHOPTYPE: u16 = 9060;
pub const SM_OPHAMSHOP: u16 = 9062;
pub const SM_CHACKITEM: u16 = 9064;
pub const SM_41: u16 = 4;
pub const SM_THROW: u16 = 5;
pub const SM_RUSH: u16 = 6;
pub const SM_RUSHKUNG: u16 = 7;
/// 烈火
pub const SM_FIREHIT: u16 = 8;
pub const SM_BACKSTEP: u16 = 9;
/// 转向
pub const SM_TURN: u16 = 10;
/// 走
pub const SM_WALK: u16 = 11;
pub const SM_SITDOWN: u16 = 12;
pub const SM_RUN: u16 = 13;
/// 砍
pub const SM_HIT: u16 = 14;
pub const SM_HEAVYHIT: u16 = 15;
pub const SM_BIGHIT: u16 = 16;
/// 使用魔法
pub const SM_SPELL: u16 = 17;
/// 刺杀
pub const SM_POWERHIT: u16 = 18;
pub const SM_LONGHIT: u16 = 19;
pub const SM_DIGUP: u16 = 20;
pub const SM_DIGDOWN: u16 = 21;
pub const SM_FLYAXE: u16 = 22;
pub const SM_LIGHTING: u16 = 23;
pub const SM_WIDEHIT: u16 = 24;
pub const SM_CRSHIT: u16 = 25;
pub const SM_TWINHIT: u16 = 26;
pub const SM_ALIVE: u16 = 27;
pub const SM_MOVEFAIL: u16 = 28;
pub const SM_HIDE: u16 = 29;
pub const SM_DISAPPEAR: u16 = 30;
/// 弯腰
pub const SM_STRUCK: u16 = 31;
pub const SM_DEATH: u16 = 32;
pub const SM_SKELETON: u16 = 33;
pub const SM_NOWDEATH: u16 = 34;
pub const SM_HEAR: u16 = 40;
pub const SM_FEATURECHANGED: u16 = 41;
pub const SM_USERNAME: u16 = 42;
pub const SM_43: u16 = 43;
pub const SM_WINEXP: u16 = 44;
pub const SM_LEVELUP: u16 = 45;
pub const SM_DAYCHANGING: u16 = 46;
pub const SM_LOGON: u16 = 50;
pub const SM_NEWMAP: u16 = 51;
pub const SM_ABILITY: u16 = 52;
pub const SM_HEALTHSPELLCHANGED: u16 = 53;
pub const SM_MAPDESCRIPTION: u16 = 54;
pub const SM_SPELL2: u16 = 117;
pub const SM_HWID: u16 = 113;
pub const SM_SYSMESSAGE: u16 = 100;
pub const SM_GROUPMESSAGE: u16 = 101;
pub const SM_CRY: u16 = 102;
pub const SM_WHISPER: u16 = 103;
pub const SM_GUILDMESSAGE: u16 = 104;
pub const SM_ADDITEM: u16 = 200;
pub const SM_BAGITEMS: u16 = 201;
pub const SM_DELITEM: u16 = 202;
pub const SM_UPDATEITEM: u16 = 203;
pub const SM_ADDMAGIC: u16 = 210;
pub const SM_SENDMYMAGIC: u16 = 211;
pub const SM_DELMAGIC: u16 = 212;
pub const SM_ATTACKMODE: u16 = 213;
pub const SM_QUERYVALUE: u16 = 215;
/// 攻击模式成功
pub const SM_CERTIFICATION_SUCCESS: u16 = 500;
/// 攻击模式失败
pub const SM_CERTIFICATION_FAIL: u16 = 501;
/// 账号不存在
pub const SM_ID_NOTFOUND: u16 = 502;
pub const SM_PASSWD_FAIL: u16 = 503;
pub const SM_NEWID_SUCCESS: u16 = 504;
pub const SM_NEWID_FAIL: u16 = 505;
pub const SM_CHGPASSWD_SUCCESS: u16 = 506;
pub const SM_CHGPASSWD_FAIL: u16 = 507;
/// 查询人物
pub const SM_QUERYCHR: u16 = 520;
/// 创建人物成功
pub const SM_NEWCHR_SUCCESS: u16 = 521;
/// 创建人物失败
pub const SM_NEWCHR_FAIL: u16 = 522;
/// 删除人物成功
pub const SM_DELCHR_SUCCESS: u16 = 523;
/// 删除人物失败
pub const SM_DELCHR_FAIL: u16 = 524;
pub const SM_STARTPLAY: u16 = 525;
pub const SM_STARTFAIL: u16 = 526;
pub const SM_QUERYCHR_FAIL: u16 = 527;
pub const SM_OUTOFCONNECTION: u16 = 528;
pub const SM_PASSOK_SELECTSERVER: u16 = 529;
pub const SM_SELECTSERVER_OK: u16 = 530;
pub const SM_NEEDUPDATE_ACCOUNT: u16 = 531;
pub const SM_UPDATEID_SUCCESS: u16 = 532;
pub const SM_UPDATEID_FAIL: u16 = 533;
pub const SM_DROPITEM_SUCCESS: u16 = 600;
pub const SM_DROPITEM_FAIL: u16 = 601;
pub const SM_ITEMSHOW: u16 = 610;
pub const SM_ITEMHIDE: u16 = 611;
pub const SM_OPENDOOR_OK: u16 = 612;
pub const SM_OPENDOOR_LOCK: u16 = 613;
pub const SM_CLOSEDOOR: u16 = 614;
pub const SM_TAKEON_OK: u16 = 615;
pub const SM_TAKEON_FAIL: u16 = 616;
pub const SM_TAKEOFF_OK: u16 = 619;
pub const SM_TAKEOFF_FAIL: u16 = 620;
pub const SM_SENDUSEITEMS: u16 = 621;
pub const SM_WEIGHTCHANGED: u16 = 622;
pub const SM_QUERYITEMDLG: u16 = 623;
pub const SM_ITEMDLGSELECT: u16 = 624;
/// 清理对象
pub const SM_CLEAROBJECTS: u16 = 633;
pub const SM_CHANGEMAP: u16 = 634;
pub const SM_EAT_OK: u16 = 635;
pub const SM_EAT_FAIL: u16 = 636;
pub const SM_BUTCH: u16 = 637;
pub const SM_MAGICFIRE: u16 = 638;
pub const SM_MAGICFIRE_FAIL: u16 = 639;
pub const SM_MAGIC_LVEXP: u16 = 640;
pub const SM_DURACHANGE: u16 = 642;
pub const SM_MERCHANTSAY: u16 = 643;
pub const SM_MOVEMESSAGE: u16 = 99;
pub const SM_MERCHANTDLGCLOSE: u16 = 644;
pub const SM_SENDGOODSLIST: u16 = 645;
pub const SM_SENDUSERSELL: u16 = 646;
pub const SM_SENDBUYPRICE: u16 = 647;
pub const SM_USERSELLITEM_OK: u16 = 648;
pub const SM_USERSELLITEM_FAIL: u16 = 649;
pub const SM_BUYITEM_SUCCESS: u16 = 650;
pub const SM_BUYITEM_FAIL: u16 = 651;
pub const SM_SENDDETAILGOODSLIST: u16 = 652;
pub const SM_GOLDCHANGED: u16 = 653;
pub const SM_CHANGELIGHT: u16 = 654;
pub const SM_LAMPCHANGEDURA: u16 = 655;
pub const SM_CHANGENAMECOLOR: u16 = 656;
pub const SM_CHARSTATUSCHANGED: u16 = 657;
pub const SM_SENDNOTICE: u16 = 658;
pub const SM_GROUPMODECHANGED: u16 = 659;
pub const SM_CREATEGROUP_OK: u16 = 660;
pub const SM_CREATEGROUP_FAIL: u16 = 661;
pub const SM_GROUPADDMEM_OK: u16 = 662;
pub const SM_GROUPDELMEM_OK: u16 = 663;
pub const SM_GROUPADDMEM_FAIL: u16 = 664;
pub const SM_GROUPDELMEM_FAIL: u16 = 665;
pub const SM_GROUPCANCEL: u16 = 666;
pub const SM_GROUPMEMBERS: u16 = 667;
pub const SM_SENDUSERREPAIR: u16 = 668;
pub const SM_USERREPAIRITEM_OK: u16 = 669;
pub const SM_USERREPAIRITEM_FAIL: u16 = 670;
pub const SM_SENDREPAIRCOST: u16 = 671;
pub const SM_DEALMENU: u16 = 673;
pub const SM_DEALTRY_FAIL: u16 = 674;
pub const SM_DEALADDITEM_OK: u16 = 675;
pub const SM_DEALADDITEM_FAIL: u16 = 676;
pub const SM_DEALDELITEM_OK: u16 = 677;
pub const SM_DEALDELITEM_FAIL: u16 = 678;
pub const SM_DEALCANCEL: u16 = 681;
pub const SM_DEALREMOTEADDITEM: u16 = 682;
pub const SM_DEALREMOTEDELITEM: u16 = 683;
pub const SM_DEALCHGGOLD_OK: u16 = 684;
pub const SM_DEALCHGGOLD_FAIL: u16 = 685;
pub const SM_DEALREMOTECHGGOLD: u16 = 686;
pub const SM_DEALSUCCESS: u16 = 687;
pub const SM_SENDUSERSTORAGEITEM: u16 = 700;
pub const SM_STORAGE_OK: u16 = 701;
pub const SM_STORAGE_FULL: u16 = 702;
pub const SM_STORAGE_FAIL: u16 = 703;
pub const SM_SAVEITEMLIST: u16 = 704;
pub const SM_TAKEBACKSTORAGEITEM_OK: u16 = 705;
pub const SM_TAKEBACKSTORAGEITEM_FAIL: u16 = 706;
pub const SM_TAKEBACKSTORAGEITEM_FULLBAG: u16 = 707;
pub const SM_AREASTATE: u16 = 766;
pub const SM_MYSTATUS: u16 = 708;
pub const SM_DELITEMS: u16 = 709;
pub const SM_READMINIMAP_OK: u16 = 710;
pub const SM_READMINIMAP_FAIL: u16 = 711;
pub const SM_SENDUSERMAKEDRUGITEMLIST: u16 = 712;
pub const SM_MAKEDRUG_SUCCESS: u16 = 713;
pub const SM_MAKEDRUG_FAIL: u16 = 714;
pub const SM_716: u16 = 716;
pub const SM_CHANGEGUILDNAME: u16 = 750;
pub const SM_SENDUSERSTATE: u16 = 751;
pub const SM_SUBABILITY: u16 = 752;
pub const SM_OPENGUILDDLG: u16 = 753;
pub const SM_OPENGUILDDLG_FAIL: u16 = 754;
pub const SM_SENDGUILDMEMBERLIST: u16 = 756;
pub const SM_GUILDADDMEMBER_OK: u16 = 757;
pub const SM_GUILDADDMEMBER_FAIL: u16 = 758;
pub const SM_GUILDDELMEMBER_OK: u16 = 759;
pub const SM_GUILDDELMEMBER_FAIL: u16 = 760;
pub const SM_GUILDRANKUPDATE_FAIL: u16 = 761;
pub const SM_BUILDGUILD_OK: u16 = 762;
pub const SM_BUILDGUILD_FAIL: u16 = 763;
pub const SM_DONATE_OK: u16 = 764;
pub const SM_DONATE_FAIL: u16 = 765;
pub const SM_MENU_OK: u16 = 767;
pub const SM_GUILDMAKEALLY_OK: u16 = 768;
pub const SM_GUILDMAKEALLY_FAIL: u16 = 769;
pub const SM_GUILDBREAKALLY_OK: u16 = 770;
pub const SM_GUILDBREAKALLY_FAIL: u16 = 771;
pub const SM_DLGMSG: u16 = 772;
pub const SM_SPACEMOVE_HIDE: u16 = 800;
pub const SM_SPACEMOVE_SHOW: u16 = 801;
pub const SM_RECONNECT: u16 = 802;
pub const SM_GHOST: u16 = 803;
pub const SM_SHOWEVENT: u16 = 804;
pub const SM_HIDEEVENT: u16 = 805;
pub const SM_SPACEMOVE_HIDE2: u16 = 806;
pub const SM_SPACEMOVE_SHOW2: u16 = 807;
pub const SM_TIMECHECK_MSG: u16 = 810;
pub const SM_ADJUST_BONUS: u16 = 811;
pub const SM_OPENHEALTH: u16 = 1100;
pub const SM_CLOSEHEALTH: u16 = 1101;
pub const SM_CHANGEFACE: u16 = 1104;
pub const SM_BREAKWEAPON: u16 = 1102;
pub const SM_INSTANCEHEALGUAGE: u16 = 1103;
pub const SM_VERSION_FAIL: u16 = 1106;
pub const SM_ITEMUPDATE: u16 = 1500;
pub const SM_MONSTERSAY: u16 = 1501;
pub const SM_EXCHGTAKEON_OK: u16 = 65023;
pub const SM_EXCHGTAKEON_FAIL: u16 = 65024;
pub const SM_TEST: u16 = 65037;
pub const SM_ACTION_MIN: u16 = 65070;
pub const SM_ACTION_MAX: u16 = 65071;
pub const SM_ACTION2_MIN: u16 = 65072;
pub const SM_ACTION2_MAX: u16 = 65073;
pub const SM_SENDGAMELIST: u16 = 5002;
pub const SM_SMUGGLE_SUCESS: u16 = 41901;
pub const SM_SMUGGLE: u16 = 41900;
/// 找回密码成功
pub const SM_GETBACKPASSWD_SUCCESS: u16 = 5005;
/// 找回密码失败
pub const SM_GETBACKPASSWD_FAIL: u16 = 5006;
/// 发送服务器配置信息
pub const SM_SERVERCONFIG: u16 = 11029;
pub const SM_GAMEGOLDNAME: u16 = 5008;
pub const SM_PASSWORD: u16 = 5009;
pub const SM_HORSERUN: u16 = 5010;
pub const SM_RUNGATELOGOUT: u16 = 599;
pub const SM_PLAYERCONFIG: u16 = 560;
pub const SM_PLAYDICE: u16 = 8001;
pub const SM_PASSWORDSTATUS: u16 = 8002;
pub const SM_NEEDPASSWORD: u16 = 8003;
pub const SM_GETREGINFO: u16 = 8004;
pub const SM_SENDDEALOFFFORM: u16 = 23001;
pub const SM_SELLOFFADDITEM_OK: u16 = 23003;
pub const SM_SellOffADDITEM_FAIL: u16 = 23005;
pub const SM_SELLOFFDELITEM_OK: u16 = 23008;
pub const SM_SELLOFFDELITEM_FAIL: u16 = 23010;
pub const SM_SellOffCANCEL: u16 = 23014;
pub const SM_SELLOFFEND_OK: u16 = 23016;
pub const SM_SELLOFFEND_FAIL: u16 = 23018;
pub const SM_QUERYYBSELL: u16 = 23021;
pub const SM_QUERYYBDEAL: u16 = 23023;
pub const SM_SELLOFFBUY_OK: u16 = 23027;

// ==== RM_* — 网关↔引擎内部转译号 ====

pub const RM_SENDSELL: u16 = 9052;
pub const RM_TURN: u16 = 10001;
pub const RM_WALK: u16 = 10002;
pub const RM_HORSERUN: u16 = 50003;
pub const RM_RUN: u16 = 10003;
pub const RM_HIT: u16 = 10004;
pub const RM_BIGHIT: u16 = 10006;
pub const RM_HEAVYHIT: u16 = 10007;
pub const RM_SPELL: u16 = 10008;
pub const RM_SPELL2: u16 = 10009;
pub const RM_MOVEFAIL: u16 = 10010;
pub const RM_LONGHIT: u16 = 10011;
pub const RM_WIDEHIT: u16 = 10012;
pub const RM_FIREHIT: u16 = 10014;
pub const RM_CRSHIT: u16 = 10015;
pub const RM_DEATH: u16 = 10021;
pub const RM_SKELETON: u16 = 10024;
/// 发送登录消息
pub const RM_LOGON: u16 = 10050;
pub const RM_ABILITY: u16 = 10051;
pub const RM_HEALTHSPELLCHANGED: u16 = 10052;
pub const RM_DAYCHANGING: u16 = 10053;
pub const RM_REFMESSAGE: u16 = 10101;
pub const RM_WEIGHTCHANGED: u16 = 10115;
pub const RM_FEATURECHANGED: u16 = 10116;
pub const RM_BUTCH: u16 = 10119;
pub const RM_MAGICFIRE: u16 = 10120;
pub const RM_MAGICFIREFAIL: u16 = 10121;
pub const RM_SENDMYMAGIC: u16 = 10122;
pub const RM_MAGIC_LVEXP: u16 = 10123;
pub const RM_DURACHANGE: u16 = 10125;
pub const RM_MERCHANTDLGCLOSE: u16 = 10127;
pub const RM_SENDGOODSLIST: u16 = 10128;
pub const RM_SENDUSERSELL: u16 = 10129;
pub const RM_SENDBUYPRICE: u16 = 10130;
pub const RM_USERSELLITEM_OK: u16 = 10131;
pub const RM_USERSELLITEM_FAIL: u16 = 10132;
pub const RM_BUYITEM_SUCCESS: u16 = 10133;
pub const RM_BUYITEM_FAIL: u16 = 10134;
pub const RM_SENDDETAILGOODSLIST: u16 = 10135;
pub const RM_GOLDCHANGED: u16 = 10136;
pub const RM_CHANGELIGHT: u16 = 10137;
pub const RM_LAMPCHANGEDURA: u16 = 10138;
pub const RM_CHARSTATUSCHANGED: u16 = 10139;
pub const RM_GROUPCANCEL: u16 = 10140;
pub const RM_SENDUSERREPAIR: u16 = 10141;
pub const RM_SENDUSERSREPAIR: u16 = 50142;
pub const RM_SENDREPAIRCOST: u16 = 10142;
pub const RM_USERREPAIRITEM_OK: u16 = 10143;
pub const RM_USERREPAIRITEM_FAIL: u16 = 10144;
pub const RM_USERSTORAGEITEM: u16 = 10146;
pub const RM_USERGETBACKITEM: u16 = 10147;
pub const RM_SENDDELITEMLIST: u16 = 10148;
pub const RM_USERMAKEDRUGITEMLIST: u16 = 10149;
pub const RM_MAKEDRUG_SUCCESS: u16 = 10150;
pub const RM_MAKEDRUG_FAIL: u16 = 10151;
pub const RM_ALIVE: u16 = 10153;
pub const RM_RANDOMSPACEMOVE: u16 = 10155;
pub const RM_DIGUP: u16 = 10200;
pub const RM_DIGDOWN: u16 = 10201;
pub const RM_FLYAXE: u16 = 10202;
pub const RM_LIGHTING: u16 = 10204;
pub const RM_10205: u16 = 10205;
pub const RM_CHANGEGUILDNAME: u16 = 10301;
pub const RM_SUBABILITY: u16 = 10302;
pub const RM_BUILDGUILD_OK: u16 = 10303;
pub const RM_BUILDGUILD_FAIL: u16 = 10304;
pub const RM_DONATE_OK: u16 = 10305;
pub const RM_DONATE_FAIL: u16 = 10306;
pub const RM_MENU_OK: u16 = 10309;
pub const RM_RECONNECTION: u16 = 10332;
pub const RM_HIDEEVENT: u16 = 10333;
pub const RM_SHOWEVENT: u16 = 10334;
pub const RM_10401: u16 = 10401;
pub const RM_OPENHEALTH: u16 = 10410;
pub const RM_CLOSEHEALTH: u16 = 10411;
/// 升级武器失败
pub const RM_BREAKWEAPON: u16 = 10413;
/// 心灵启示
pub const RM_ABILSEEHEALGAUGE: u16 = 10414;
pub const RM_CHANGEFACE: u16 = 10415;
pub const RM_PASSWORD: u16 = 10416;
pub const RM_PLAYDICE: u16 = 10500;
pub const RM_HEAR: u16 = 11001;
pub const RM_WHISPER: u16 = 11002;
pub const RM_CRY: u16 = 11003;
pub const RM_SYSMESSAGE: u16 = 11004;
pub const RM_GROUPMESSAGE: u16 = 11005;
pub const RM_SYSMESSAGE2: u16 = 11006;
pub const RM_GUILDMESSAGE: u16 = 11007;
pub const RM_SYSMESSAGE3: u16 = 11008;
pub const RM_MERCHANTSAY: u16 = 11009;
pub const RM_ZEN_BEE: u16 = 8020;
pub const RM_DELAYMAGIC: u16 = 8021;
pub const RM_STRUCK: u16 = 8018;
/// 强化攻击伤害
pub const RM_STRUCKEFFECT: u16 = 10102;
/// 魔法伤害
pub const RM_MAGSTRUCK_MINE: u16 = 8030;
pub const RM_MAGHEALING: u16 = 8034;
pub const RM_POISON: u16 = 8037;
pub const RM_DOOPENHEALTH: u16 = 8040;
pub const RM_SPACEMOVE_FIRE2: u16 = 8042;
pub const RM_DELAYPUSHED: u16 = 8043;
/// 魔法伤害
pub const RM_MAGSTRUCK: u16 = 8044;
pub const RM_TRANSPARENT: u16 = 8045;
pub const RM_DOOROPEN: u16 = 8046;
pub const RM_DOORCLOSE: u16 = 8047;
pub const RM_DISAPPEAR: u16 = 8061;
pub const RM_SPACEMOVE_FIRE: u16 = 8062;
pub const RM_SENDUSEITEMS: u16 = 8074;
pub const RM_WINEXP: u16 = 8075;
pub const RM_ADJUST_BONUS: u16 = 8078;
/// 显示物品
pub const RM_ITEMSHOW: u16 = 8082;
pub const RM_GAMEGOLDCHANGED: u16 = 8084;
/// 删除物品
pub const RM_ITEMHIDE: u16 = 8085;
pub const RM_LEVELUP: u16 = 8086;
pub const RM_CHANGENAMECOLOR: u16 = 8090;
/// 往后退（野蛮冲转 抗拒火环）
pub const RM_PUSH: u16 = 8092;
pub const RM_CLEAROBJECTS: u16 = 8097;
pub const RM_CHANGEMAP: u16 = 8098;
pub const RM_SPACEMOVE_SHOW2: u16 = 8099;
pub const RM_SPACEMOVE_SHOW: u16 = 8100;
pub const RM_USERNAME: u16 = 8101;
pub const RM_MYSTATUS: u16 = 8102;
pub const RM_STRUCK_MAG: u16 = 8103;
pub const RM_RUSH: u16 = 8104;
pub const RM_RUSHKUNG: u16 = 8105;
pub const RM_PASSWORDSTATUS: u16 = 8106;
pub const RM_POWERHIT: u16 = 8107;
pub const RM_41: u16 = 9041;
pub const RM_TWINHIT: u16 = 9042;
pub const RM_43: u16 = 9043;
pub const RM_MOVEMESSAGE: u16 = 10099;
/// 祈祷套装
pub const RM_SPIRITSUITE: u16 = 9045;
pub const RM_SENDDEALOFFFORM: u16 = 23000;
pub const RM_SELLOFFADDITEM_OK: u16 = 23004;
pub const RM_SellOffADDITEM_FAIL: u16 = 23006;
pub const RM_SELLOFFDELITEM_OK: u16 = 23009;
pub const RM_SELLOFFDELITEM_FAIL: u16 = 23011;
pub const RM_SELLOFFCANCEL: u16 = 23013;
pub const RM_SELLOFFEND_OK: u16 = 23017;
pub const RM_SELLOFFEND_FAIL: u16 = 23019;
pub const RM_QUERYYBSELL: u16 = 23020;
pub const RM_QUERYYBDEAL: u16 = 23022;
pub const RM_SELLOFFBUY_OK: u16 = 23028;
pub const RM_MARKET_LIST: u16 = 11015;
pub const RM_MARKET_RESULT: u16 = 11016;
/// 更新自身视野对象
pub const RM_UPDATEVIEWRANGE: u16 = 60000;
/// 玩家杀死怪物触发消息
pub const RM_PLAYERKILLMONSTER: u16 = 60001;
/// 死亡掉落物品
pub const RM_DIEDROPITEM: u16 = 60002;
/// 主人死亡宠物叛变消息
pub const RM_MASTERDIEMUTINY: u16 = 60003;
/// 主人死亡尸体消息宠物消失消息
pub const RM_MASTERDIEGHOST: u16 = 60004;
/// 困魔咒
pub const RM_MAKEHOLYSEIZEMODE: u16 = 60005;

// ==== SS_* — 会话/登录服务内部 ====

pub const SS_OPENSESSION: u16 = 100;
pub const SS_CLOSESESSION: u16 = 101;
pub const SS_KEEPALIVE: u16 = 104;
pub const SS_KICKUSER: u16 = 111;
pub const SS_SERVERLOAD: u16 = 113;
pub const SS_200: u16 = 200;
pub const SS_201: u16 = 201;
pub const SS_202: u16 = 202;
pub const SS_203: u16 = 203;
pub const SS_204: u16 = 204;
pub const SS_205: u16 = 205;
pub const SS_206: u16 = 206;
pub const SS_207: u16 = 207;
pub const SS_208: u16 = 208;
pub const SS_209: u16 = 209;
pub const SS_210: u16 = 210;
pub const SS_211: u16 = 211;
pub const SS_212: u16 = 212;
pub const SS_213: u16 = 213;
pub const SS_214: u16 = 214;
pub const SS_WHISPER: u16 = 299;
/// 同步服务器信息
pub const SS_SERVERINFO: u16 = 103;
/// 客户端退出游戏
pub const SS_SOFTOUTSESSION: u16 = 102;
pub const SS_LOGINCOST: u16 = 30002;

// ==== ISM_* — 跨服/互联消息 ====

pub const ISM_ACCOUNTEXPIRED: u16 = 114;
/// 查询账号剩余游戏时间
pub const ISM_QUERYACCOUNTEXPIRETIME: u16 = 115;
pub const ISM_QUERYPLAYTIME: u16 = 116;
/// 减少或更新账号游戏时间
pub const ISM_GAMETIMEOFTIMECARDUSER: u16 = 112;
pub const ISM_CHECKTIMEACCOUNT: u16 = 116;
pub const ISM_GROUPSERVERHEART: u16 = 100;
pub const ISM_USERSERVERCHANGE: u16 = 200;
pub const ISM_USERLOGON: u16 = 201;
pub const ISM_USERLOGOUT: u16 = 202;
pub const ISM_WHISPER: u16 = 203;
pub const ISM_SYSOPMSG: u16 = 204;
pub const ISM_ADDGUILD: u16 = 205;
pub const ISM_DELGUILD: u16 = 206;
pub const ISM_RELOADGUILD: u16 = 207;
pub const ISM_GUILDMSG: u16 = 208;
pub const ISM_CHATPROHIBITION: u16 = 209;
pub const ISM_CHATPROHIBITIONCANCEL: u16 = 210;
pub const ISM_CHANGECASTLEOWNER: u16 = 211;
pub const ISM_RELOADCASTLEINFO: u16 = 212;
pub const ISM_RELOADADMIN: u16 = 213;
pub const ISM_FRIEND_INFO: u16 = 214;
pub const ISM_FRIEND_DELETE: u16 = 215;
pub const ISM_FRIEND_OPEN: u16 = 216;
pub const ISM_FRIEND_CLOSE: u16 = 217;
pub const ISM_FRIEND_RESULT: u16 = 218;
pub const ISM_TAG_SEND: u16 = 219;
pub const ISM_TAG_RESULT: u16 = 220;
pub const ISM_USER_INFO: u16 = 221;
pub const ISM_CHANGESERVERRECIEVEOK: u16 = 222;
pub const ISM_RELOADCHATLOG: u16 = 223;
pub const ISM_MARKETOPEN: u16 = 224;
pub const ISM_MARKETCLOSE: u16 = 225;
pub const ISM_LM_DELETE: u16 = 226;
pub const ISM_RELOADMAKEITEMLIST: u16 = 227;
pub const ISM_GUILDMEMBER_RECALL: u16 = 228;
pub const ISM_RELOADGUILDAGIT: u16 = 229;
pub const ISM_LM_WHISPER: u16 = 230;
pub const ISM_GMWHISPER: u16 = 231;
pub const ISM_LM_LOGIN: u16 = 232;
pub const ISM_LM_LOGOUT: u16 = 233;
pub const ISM_REQUEST_RECALL: u16 = 234;
pub const ISM_RECALL: u16 = 235;
pub const ISM_LM_LOGIN_REPLY: u16 = 236;
pub const ISM_LM_KILLED_MSG: u16 = 237;
pub const ISM_REQUEST_LOVERRECALL: u16 = 238;
pub const ISM_STANDARDTICKREQ: u16 = 239;
pub const ISM_STANDARDTICK: u16 = 240;
pub const ISM_GUILDWAR: u16 = 241;
/// 发送跨服组队消息
pub const ISM_GRUOPMESSAGE: u16 = 242;

// ==== DBR_* — DB 应答 ====

pub const DBR_FAIL: u16 = 2000;
pub const DBR_LOADHUMANRCD: u16 = 1100;
pub const DBR_SAVEHUMANRCD: u16 = 1102;

// ==== DB_* — DB 请求 ====

pub const DB_LOADHUMANRCD: u16 = 100;
pub const DB_SAVEHUMANRCD: u16 = 101;
pub const DB_SAVEHUMANRCDEX: u16 = 102;
/// 读取拍卖行数据
pub const DB_LOADMARKET: u8 = 100;
/// 保存拍卖行数据
pub const DB_SAVEMARKET: u8 = 101;
/// 搜索拍卖行数据
pub const DB_SEARCHMARKET: u8 = 102;
/// 搜索拍卖行数据成功
pub const DB_SEARCHMARKETSUCCESS: u8 = 103;
/// 搜索拍卖行数据失败
pub const DB_SRARCHMARKETFAIL: u8 = 104;
/// 读取拍卖行数据成功
pub const DB_LOADMARKETSUCCESS: u8 = 105;
/// 读取拍卖行数据失败
pub const DB_LOADMARKETFAIL: u8 = 106;
/// 保存拍卖行数据成功
pub const DB_SAVEMARKETSUCCESS: u8 = 107;
/// 保存拍卖行数据失败
pub const DB_SAVEMARKETFAIL: u8 = 108;
/// 获取用户拍卖行数据
pub const DB_LOADUSERMARKET: u8 = 109;
/// 获取用户拍卖行数据成功
pub const DB_LOADUSERMARKETSUCCESS: u8 = 110;
/// 获取用户拍卖行数据失败
pub const DB_LOADUSERMARKETFAIL: u8 = 111;

// ==== GM_* — 网关/引擎管理号（Messages.cs 侧） ====

pub const GM_TEST: u16 = 20;
pub const GM_STOP: u16 = 21;

/// `Grobal2.cs` 白名单常量（网关↔引擎管理帧与版本号）。
pub mod grobal2 {
    pub const ClientVersionNumber: u32 = 1200409180;
    pub const PacketCode: u32 = 2862262938;
    pub const GM_OPEN: u8 = 1;
    pub const GM_CLOSE: u8 = 2;
    pub const GM_CHECKSERVER: u8 = 3;
    pub const GM_CHECKCLIENT: u8 = 4;
    pub const GM_DATA: u8 = 5;
    pub const GM_SERVERUSERINDEX: u8 = 6;
    pub const GM_RECEIVE_OK: u8 = 7;
    pub const GM_STOP: u8 = 8;
}

/// ident 值 → 全部常量名（值可冲突；按模块边界取舍是调用方的事）。
/// 仅用于日志/回放报告的可读化，不参与分派。
#[must_use]
pub fn names_of(ident: u16) -> &'static [&'static str] {
    match ident {
        4 => &["SM_41"],
        5 => &["SM_THROW"],
        6 => &["SM_RUSH"],
        7 => &["SM_RUSHKUNG"],
        8 => &["SM_FIREHIT"],
        9 => &["SM_BACKSTEP"],
        10 => &["SM_TURN"],
        11 => &["SM_WALK"],
        12 => &["SM_SITDOWN"],
        13 => &["SM_RUN"],
        14 => &["SM_HIT"],
        15 => &["SM_HEAVYHIT"],
        16 => &["SM_BIGHIT"],
        17 => &["SM_SPELL"],
        18 => &["SM_POWERHIT"],
        19 => &["SM_LONGHIT"],
        20 => &["SM_DIGUP", "GM_TEST"],
        21 => &["SM_DIGDOWN", "GM_STOP"],
        22 => &["SM_FLYAXE"],
        23 => &["SM_LIGHTING"],
        24 => &["SM_WIDEHIT"],
        25 => &["SM_CRSHIT"],
        26 => &["SM_TWINHIT"],
        27 => &["SM_ALIVE"],
        28 => &["SM_MOVEFAIL"],
        29 => &["SM_HIDE"],
        30 => &["SM_DISAPPEAR"],
        31 => &["SM_STRUCK"],
        32 => &["SM_DEATH"],
        33 => &["SM_SKELETON"],
        34 => &["SM_NOWDEATH"],
        40 => &["SM_HEAR"],
        41 => &["SM_FEATURECHANGED"],
        42 => &["SM_USERNAME", "CM_42HIT"],
        43 => &["SM_43"],
        44 => &["SM_WINEXP"],
        45 => &["SM_LEVELUP"],
        46 => &["SM_DAYCHANGING"],
        50 => &["SM_LOGON"],
        51 => &["SM_NEWMAP"],
        52 => &["SM_ABILITY"],
        53 => &["SM_HEALTHSPELLCHANGED"],
        54 => &["SM_MAPDESCRIPTION"],
        80 => &["CM_QUERYUSERNAME"],
        81 => &["CM_QUERYBAGITEMS"],
        82 => &["CM_QUERYUSERSTATE"],
        99 => &["SM_MOVEMESSAGE"],
        100 => &[
            "CM_QUERYCHR",
            "SM_SYSMESSAGE",
            "SS_OPENSESSION",
            "DB_LOADHUMANRCD",
            "ISM_GROUPSERVERHEART",
        ],
        101 => &[
            "CM_NEWCHR",
            "SM_GROUPMESSAGE",
            "SS_CLOSESESSION",
            "DB_SAVEHUMANRCD",
        ],
        102 => &[
            "CM_DELCHR",
            "SM_CRY",
            "SS_SOFTOUTSESSION",
            "DB_SAVEHUMANRCDEX",
        ],
        103 => &["CM_SELCHR", "SM_WHISPER", "SS_SERVERINFO"],
        104 => &["CM_SELECTSERVER", "SM_GUILDMESSAGE", "SS_KEEPALIVE"],
        111 => &["SS_KICKUSER"],
        112 => &["ISM_GAMETIMEOFTIMECARDUSER"],
        113 => &["SM_HWID", "SS_SERVERLOAD"],
        114 => &["ISM_ACCOUNTEXPIRED"],
        115 => &["ISM_QUERYACCOUNTEXPIRETIME"],
        116 => &["ISM_QUERYPLAYTIME", "ISM_CHECKTIMEACCOUNT"],
        117 => &["SM_SPELL2"],
        199 => &["UNKNOWMSG"],
        200 => &["SM_ADDITEM", "SS_200", "ISM_USERSERVERCHANGE"],
        201 => &["SM_BAGITEMS", "SS_201", "ISM_USERLOGON"],
        202 => &["SM_DELITEM", "SS_202", "ISM_USERLOGOUT"],
        203 => &["SM_UPDATEITEM", "SS_203", "ISM_WHISPER"],
        204 => &["SS_204", "ISM_SYSOPMSG"],
        205 => &["SS_205", "ISM_ADDGUILD"],
        206 => &["SS_206", "ISM_DELGUILD"],
        207 => &["SS_207", "ISM_RELOADGUILD"],
        208 => &["SS_208", "ISM_GUILDMSG"],
        209 => &["SS_209", "ISM_CHATPROHIBITION"],
        210 => &["SM_ADDMAGIC", "SS_210", "ISM_CHATPROHIBITIONCANCEL"],
        211 => &["SM_SENDMYMAGIC", "SS_211", "ISM_CHANGECASTLEOWNER"],
        212 => &["SM_DELMAGIC", "SS_212", "ISM_RELOADCASTLEINFO"],
        213 => &["SM_ATTACKMODE", "SS_213", "ISM_RELOADADMIN"],
        214 => &["SS_214", "ISM_FRIEND_INFO"],
        215 => &["SM_QUERYVALUE", "ISM_FRIEND_DELETE"],
        216 => &["ISM_FRIEND_OPEN"],
        217 => &["ISM_FRIEND_CLOSE"],
        218 => &["ISM_FRIEND_RESULT"],
        219 => &["ISM_TAG_SEND"],
        220 => &["ISM_TAG_RESULT"],
        221 => &["ISM_USER_INFO"],
        222 => &["ISM_CHANGESERVERRECIEVEOK"],
        223 => &["ISM_RELOADCHATLOG"],
        224 => &["ISM_MARKETOPEN"],
        225 => &["ISM_MARKETCLOSE"],
        226 => &["ISM_LM_DELETE"],
        227 => &["ISM_RELOADMAKEITEMLIST"],
        228 => &["ISM_GUILDMEMBER_RECALL"],
        229 => &["ISM_RELOADGUILDAGIT"],
        230 => &["ISM_LM_WHISPER"],
        231 => &["ISM_GMWHISPER"],
        232 => &["ISM_LM_LOGIN"],
        233 => &["ISM_LM_LOGOUT"],
        234 => &["ISM_REQUEST_RECALL"],
        235 => &["ISM_RECALL"],
        236 => &["ISM_LM_LOGIN_REPLY"],
        237 => &["ISM_LM_KILLED_MSG"],
        238 => &["ISM_REQUEST_LOVERRECALL"],
        239 => &["ISM_STANDARDTICKREQ"],
        240 => &["ISM_STANDARDTICK"],
        241 => &["ISM_GUILDWAR"],
        242 => &["ISM_GRUOPMESSAGE"],
        299 => &["SS_WHISPER"],
        500 => &["SM_CERTIFICATION_SUCCESS"],
        501 => &["SM_CERTIFICATION_FAIL"],
        502 => &["SM_ID_NOTFOUND"],
        503 => &["SM_PASSWD_FAIL"],
        504 => &["SM_NEWID_SUCCESS"],
        505 => &["SM_NEWID_FAIL"],
        506 => &["SM_CHGPASSWD_SUCCESS"],
        507 => &["SM_CHGPASSWD_FAIL"],
        520 => &["SM_QUERYCHR"],
        521 => &["SM_NEWCHR_SUCCESS"],
        522 => &["SM_NEWCHR_FAIL"],
        523 => &["SM_DELCHR_SUCCESS"],
        524 => &["SM_DELCHR_FAIL"],
        525 => &["SM_STARTPLAY"],
        526 => &["SM_STARTFAIL"],
        527 => &["SM_QUERYCHR_FAIL"],
        528 => &["SM_OUTOFCONNECTION"],
        529 => &["SM_PASSOK_SELECTSERVER"],
        530 => &["SM_SELECTSERVER_OK"],
        531 => &["SM_NEEDUPDATE_ACCOUNT"],
        532 => &["SM_UPDATEID_SUCCESS"],
        533 => &["SM_UPDATEID_FAIL"],
        560 => &["SM_PLAYERCONFIG"],
        599 => &["SM_RUNGATELOGOUT"],
        600 => &["SM_DROPITEM_SUCCESS"],
        601 => &["SM_DROPITEM_FAIL"],
        610 => &["SM_ITEMSHOW"],
        611 => &["SM_ITEMHIDE"],
        612 => &["SM_OPENDOOR_OK"],
        613 => &["SM_OPENDOOR_LOCK"],
        614 => &["SM_CLOSEDOOR"],
        615 => &["SM_TAKEON_OK"],
        616 => &["SM_TAKEON_FAIL"],
        619 => &["SM_TAKEOFF_OK"],
        620 => &["SM_TAKEOFF_FAIL"],
        621 => &["SM_SENDUSEITEMS"],
        622 => &["SM_WEIGHTCHANGED"],
        623 => &["SM_QUERYITEMDLG"],
        624 => &["SM_ITEMDLGSELECT"],
        633 => &["SM_CLEAROBJECTS"],
        634 => &["SM_CHANGEMAP"],
        635 => &["SM_EAT_OK"],
        636 => &["SM_EAT_FAIL"],
        637 => &["SM_BUTCH"],
        638 => &["SM_MAGICFIRE"],
        639 => &["SM_MAGICFIRE_FAIL"],
        640 => &["SM_MAGIC_LVEXP"],
        642 => &["SM_DURACHANGE"],
        643 => &["SM_MERCHANTSAY"],
        644 => &["SM_MERCHANTDLGCLOSE"],
        645 => &["SM_SENDGOODSLIST"],
        646 => &["SM_SENDUSERSELL"],
        647 => &["SM_SENDBUYPRICE"],
        648 => &["SM_USERSELLITEM_OK"],
        649 => &["SM_USERSELLITEM_FAIL"],
        650 => &["SM_BUYITEM_SUCCESS"],
        651 => &["SM_BUYITEM_FAIL"],
        652 => &["SM_SENDDETAILGOODSLIST"],
        653 => &["SM_GOLDCHANGED"],
        654 => &["SM_CHANGELIGHT"],
        655 => &["SM_LAMPCHANGEDURA"],
        656 => &["SM_CHANGENAMECOLOR"],
        657 => &["SM_CHARSTATUSCHANGED"],
        658 => &["SM_SENDNOTICE"],
        659 => &["SM_GROUPMODECHANGED"],
        660 => &["SM_CREATEGROUP_OK"],
        661 => &["SM_CREATEGROUP_FAIL"],
        662 => &["SM_GROUPADDMEM_OK"],
        663 => &["SM_GROUPDELMEM_OK"],
        664 => &["SM_GROUPADDMEM_FAIL"],
        665 => &["SM_GROUPDELMEM_FAIL"],
        666 => &["SM_GROUPCANCEL"],
        667 => &["SM_GROUPMEMBERS"],
        668 => &["SM_SENDUSERREPAIR"],
        669 => &["SM_USERREPAIRITEM_OK"],
        670 => &["SM_USERREPAIRITEM_FAIL"],
        671 => &["SM_SENDREPAIRCOST"],
        673 => &["SM_DEALMENU"],
        674 => &["SM_DEALTRY_FAIL"],
        675 => &["SM_DEALADDITEM_OK"],
        676 => &["SM_DEALADDITEM_FAIL"],
        677 => &["SM_DEALDELITEM_OK"],
        678 => &["SM_DEALDELITEM_FAIL"],
        681 => &["SM_DEALCANCEL"],
        682 => &["SM_DEALREMOTEADDITEM"],
        683 => &["SM_DEALREMOTEDELITEM"],
        684 => &["SM_DEALCHGGOLD_OK"],
        685 => &["SM_DEALCHGGOLD_FAIL"],
        686 => &["SM_DEALREMOTECHGGOLD"],
        687 => &["SM_DEALSUCCESS"],
        700 => &["SM_SENDUSERSTORAGEITEM"],
        701 => &["SM_STORAGE_OK"],
        702 => &["SM_STORAGE_FULL"],
        703 => &["SM_STORAGE_FAIL"],
        704 => &["SM_SAVEITEMLIST"],
        705 => &["SM_TAKEBACKSTORAGEITEM_OK"],
        706 => &["SM_TAKEBACKSTORAGEITEM_FAIL"],
        707 => &["SM_TAKEBACKSTORAGEITEM_FULLBAG"],
        708 => &["SM_MYSTATUS"],
        709 => &["SM_DELITEMS"],
        710 => &["SM_READMINIMAP_OK"],
        711 => &["SM_READMINIMAP_FAIL"],
        712 => &["SM_SENDUSERMAKEDRUGITEMLIST"],
        713 => &["SM_MAKEDRUG_SUCCESS"],
        714 => &["SM_MAKEDRUG_FAIL"],
        716 => &["SM_716"],
        750 => &["SM_CHANGEGUILDNAME"],
        751 => &["SM_SENDUSERSTATE"],
        752 => &["SM_SUBABILITY"],
        753 => &["SM_OPENGUILDDLG"],
        754 => &["SM_OPENGUILDDLG_FAIL"],
        756 => &["SM_SENDGUILDMEMBERLIST"],
        757 => &["SM_GUILDADDMEMBER_OK"],
        758 => &["SM_GUILDADDMEMBER_FAIL"],
        759 => &["SM_GUILDDELMEMBER_OK"],
        760 => &["SM_GUILDDELMEMBER_FAIL"],
        761 => &["SM_GUILDRANKUPDATE_FAIL"],
        762 => &["SM_BUILDGUILD_OK"],
        763 => &["SM_BUILDGUILD_FAIL"],
        764 => &["SM_DONATE_OK"],
        765 => &["SM_DONATE_FAIL"],
        766 => &["SM_AREASTATE"],
        767 => &["SM_MENU_OK"],
        768 => &["SM_GUILDMAKEALLY_OK"],
        769 => &["SM_GUILDMAKEALLY_FAIL"],
        770 => &["SM_GUILDBREAKALLY_OK"],
        771 => &["SM_GUILDBREAKALLY_FAIL"],
        772 => &["SM_DLGMSG"],
        800 => &["SM_SPACEMOVE_HIDE"],
        801 => &["SM_SPACEMOVE_SHOW"],
        802 => &["SM_RECONNECT"],
        803 => &["SM_GHOST"],
        804 => &["SM_SHOWEVENT"],
        805 => &["SM_HIDEEVENT"],
        806 => &["SM_SPACEMOVE_HIDE2"],
        807 => &["SM_SPACEMOVE_SHOW2"],
        810 => &["SM_TIMECHECK_MSG"],
        811 => &["SM_ADJUST_BONUS"],
        1000 => &["CM_DROPITEM"],
        1001 => &["CM_PICKUP"],
        1002 => &["CM_OPENDOOR"],
        1003 => &["CM_TAKEONITEM"],
        1004 => &["CM_TAKEOFFITEM"],
        1005 => &["CM_1005"],
        1006 => &["CM_EAT"],
        1007 => &["CM_BUTCH"],
        1008 => &["CM_MAGICKEYCHANGE"],
        1009 => &["CM_SOFTCLOSE"],
        1010 => &["CM_CLICKNPC"],
        1011 => &["CM_MERCHANTDLGSELECT"],
        1012 => &["CM_MERCHANTQUERYSELLPRICE"],
        1013 => &["CM_USERSELLITEM"],
        1014 => &["CM_USERBUYITEM"],
        1015 => &["CM_USERGETDETAILITEM"],
        1016 => &["CM_DROPGOLD"],
        1017 => &["CM_TEST"],
        1018 => &["CM_LOGINNOTICEOK"],
        1019 => &["CM_GROUPMODE"],
        1020 => &["CM_CREATEGROUP"],
        1021 => &["CM_ADDGROUPMEMBER"],
        1022 => &["CM_DELGROUPMEMBER"],
        1023 => &["CM_USERREPAIRITEM"],
        1024 => &["CM_MERCHANTQUERYREPAIRCOST"],
        1025 => &["CM_DEALTRY"],
        1026 => &["CM_DEALADDITEM"],
        1027 => &["CM_DEALDELITEM"],
        1028 => &["CM_DEALCANCEL"],
        1029 => &["CM_DEALCHGGOLD"],
        1030 => &["CM_DEALEND"],
        1031 => &["CM_USERSTORAGEITEM"],
        1032 => &["CM_USERTAKEBACKSTORAGEITEM"],
        1033 => &["CM_WANTMINIMAP"],
        1034 => &["CM_USERMAKEDRUGITEM"],
        1035 => &["CM_OPENGUILDDLG"],
        1036 => &["CM_GUILDHOME"],
        1037 => &["CM_GUILDMEMBERLIST"],
        1038 => &["CM_GUILDADDMEMBER"],
        1039 => &["CM_GUILDDELMEMBER"],
        1040 => &["CM_GUILDUPDATENOTICE"],
        1041 => &["CM_GUILDUPDATERANKINFO"],
        1042 => &["CM_1042"],
        1043 => &["CM_ADJUST_BONUS"],
        1044 => &["CM_GUILDALLY"],
        1045 => &["CM_GUILDBREAKALLY"],
        1065 => &["CM_QUERYVAL"],
        1100 => &["SM_OPENHEALTH", "DBR_LOADHUMANRCD"],
        1101 => &["SM_CLOSEHEALTH"],
        1102 => &["SM_BREAKWEAPON", "DBR_SAVEHUMANRCD"],
        1103 => &["SM_INSTANCEHEALGUAGE"],
        1104 => &["SM_CHANGEFACE"],
        1106 => &["SM_VERSION_FAIL"],
        1500 => &["SM_ITEMUPDATE"],
        1501 => &["SM_MONSTERSAY"],
        2000 => &["CM_PROTOCOL", "DBR_FAIL"],
        2001 => &["CM_IDPASSWORD", "CM_PASSWORD"],
        2002 => &["CM_ADDNEWUSER", "CM_CHGPASSWORD"],
        2003 => &["CM_CHANGEPASSWORD"],
        2004 => &["CM_UPDATEUSER", "CM_SETPASSWORD"],
        3005 => &["CM_THROW"],
        3010 => &["CM_TURN"],
        3011 => &["CM_WALK"],
        3012 => &["CM_SITDOWN"],
        3013 => &["CM_RUN"],
        3014 => &["CM_HIT"],
        3015 => &["CM_HEAVYHIT"],
        3016 => &["CM_BIGHIT"],
        3017 => &["CM_SPELL"],
        3018 => &["CM_POWERHIT"],
        3019 => &["CM_LONGHIT"],
        3024 => &["CM_WIDEHIT"],
        3025 => &["CM_FIREHIT"],
        3030 => &["CM_SAY"],
        3035 => &["CM_HORSERUN"],
        3036 => &["CM_CRSHIT"],
        3037 => &["CM_3037"],
        3038 => &["CM_TWINHIT"],
        3040 => &["CM_QUERYUSERSET"],
        3500 => &["CM_SPEEDHACKMSG"],
        5001 => &["CM_GETGAMELIST"],
        5002 => &["SM_SENDGAMELIST"],
        5003 => &["CM_GETBACKPASSWORD"],
        5005 => &["SM_GETBACKPASSWD_SUCCESS"],
        5006 => &["SM_GETBACKPASSWD_FAIL"],
        5008 => &["SM_GAMEGOLDNAME"],
        5009 => &["SM_PASSWORD"],
        5010 => &["SM_HORSERUN"],
        8001 => &["SM_PLAYDICE"],
        8002 => &["SM_PASSWORDSTATUS"],
        8003 => &["SM_NEEDPASSWORD"],
        8004 => &["SM_GETREGINFO"],
        8018 => &["RM_STRUCK"],
        8020 => &["RM_ZEN_BEE"],
        8021 => &["RM_DELAYMAGIC"],
        8030 => &["RM_MAGSTRUCK_MINE"],
        8034 => &["RM_MAGHEALING"],
        8037 => &["RM_POISON"],
        8040 => &["RM_DOOPENHEALTH"],
        8042 => &["RM_SPACEMOVE_FIRE2"],
        8043 => &["RM_DELAYPUSHED"],
        8044 => &["RM_MAGSTRUCK"],
        8045 => &["RM_TRANSPARENT"],
        8046 => &["RM_DOOROPEN"],
        8047 => &["RM_DOORCLOSE"],
        8061 => &["RM_DISAPPEAR"],
        8062 => &["RM_SPACEMOVE_FIRE"],
        8074 => &["RM_SENDUSEITEMS"],
        8075 => &["RM_WINEXP"],
        8078 => &["RM_ADJUST_BONUS"],
        8082 => &["RM_ITEMSHOW"],
        8084 => &["RM_GAMEGOLDCHANGED"],
        8085 => &["RM_ITEMHIDE"],
        8086 => &["RM_LEVELUP"],
        8090 => &["RM_CHANGENAMECOLOR"],
        8092 => &["RM_PUSH"],
        8097 => &["RM_CLEAROBJECTS"],
        8098 => &["RM_CHANGEMAP"],
        8099 => &["RM_SPACEMOVE_SHOW2"],
        8100 => &["RM_SPACEMOVE_SHOW"],
        8101 => &["RM_USERNAME"],
        8102 => &["RM_MYSTATUS"],
        8103 => &["RM_STRUCK_MAG"],
        8104 => &["RM_RUSH"],
        8105 => &["RM_RUSHKUNG"],
        8106 => &["RM_PASSWORDSTATUS"],
        8107 => &["RM_POWERHIT"],
        9041 => &["RM_41"],
        9042 => &["RM_TWINHIT"],
        9043 => &["RM_43"],
        9045 => &["RM_SPIRITSUITE"],
        9050 => &["CM_SENDSELL"],
        9051 => &["SM_SENDSELL"],
        9052 => &["RM_SENDSELL"],
        9053 => &["CM_MYSHOPEXIT"],
        9054 => &["SM_MYSHOPEXIT"],
        9055 => &["CM_MYSHOPHAM"],
        9056 => &["SM_MYSHOPHAM"],
        9057 => &["CM_HAMSHOPBUY"],
        9058 => &["SM_HAMSHOPBUYA"],
        9059 => &["SM_HAMSHOPBUYB"],
        9060 => &["SM_HAMSHOPTYPE"],
        9061 => &["CM_OPHAMSHOP"],
        9062 => &["SM_OPHAMSHOP"],
        9063 => &["CM_CHACKITEM"],
        9064 => &["SM_CHACKITEM"],
        10001 => &["RM_TURN"],
        10002 => &["RM_WALK"],
        10003 => &["RM_RUN"],
        10004 => &["RM_HIT"],
        10006 => &["RM_BIGHIT"],
        10007 => &["RM_HEAVYHIT"],
        10008 => &["RM_SPELL"],
        10009 => &["RM_SPELL2"],
        10010 => &["RM_MOVEFAIL"],
        10011 => &["RM_LONGHIT"],
        10012 => &["RM_WIDEHIT"],
        10014 => &["RM_FIREHIT"],
        10015 => &["RM_CRSHIT"],
        10021 => &["RM_DEATH"],
        10024 => &["RM_SKELETON"],
        10050 => &["RM_LOGON"],
        10051 => &["RM_ABILITY"],
        10052 => &["RM_HEALTHSPELLCHANGED"],
        10053 => &["RM_DAYCHANGING"],
        10099 => &["RM_MOVEMESSAGE"],
        10101 => &["RM_REFMESSAGE"],
        10102 => &["RM_STRUCKEFFECT"],
        10115 => &["RM_WEIGHTCHANGED"],
        10116 => &["RM_FEATURECHANGED"],
        10119 => &["RM_BUTCH"],
        10120 => &["RM_MAGICFIRE"],
        10121 => &["RM_MAGICFIREFAIL"],
        10122 => &["RM_SENDMYMAGIC"],
        10123 => &["RM_MAGIC_LVEXP"],
        10125 => &["RM_DURACHANGE"],
        10127 => &["RM_MERCHANTDLGCLOSE"],
        10128 => &["RM_SENDGOODSLIST"],
        10129 => &["RM_SENDUSERSELL"],
        10130 => &["RM_SENDBUYPRICE"],
        10131 => &["RM_USERSELLITEM_OK"],
        10132 => &["RM_USERSELLITEM_FAIL"],
        10133 => &["RM_BUYITEM_SUCCESS"],
        10134 => &["RM_BUYITEM_FAIL"],
        10135 => &["RM_SENDDETAILGOODSLIST"],
        10136 => &["RM_GOLDCHANGED"],
        10137 => &["RM_CHANGELIGHT"],
        10138 => &["RM_LAMPCHANGEDURA"],
        10139 => &["RM_CHARSTATUSCHANGED"],
        10140 => &["RM_GROUPCANCEL"],
        10141 => &["RM_SENDUSERREPAIR"],
        10142 => &["RM_SENDREPAIRCOST"],
        10143 => &["RM_USERREPAIRITEM_OK"],
        10144 => &["RM_USERREPAIRITEM_FAIL"],
        10146 => &["RM_USERSTORAGEITEM"],
        10147 => &["RM_USERGETBACKITEM"],
        10148 => &["RM_SENDDELITEMLIST"],
        10149 => &["RM_USERMAKEDRUGITEMLIST"],
        10150 => &["RM_MAKEDRUG_SUCCESS"],
        10151 => &["RM_MAKEDRUG_FAIL"],
        10153 => &["RM_ALIVE"],
        10155 => &["RM_RANDOMSPACEMOVE"],
        10200 => &["RM_DIGUP"],
        10201 => &["RM_DIGDOWN"],
        10202 => &["RM_FLYAXE"],
        10204 => &["RM_LIGHTING"],
        10205 => &["RM_10205"],
        10301 => &["RM_CHANGEGUILDNAME"],
        10302 => &["RM_SUBABILITY"],
        10303 => &["RM_BUILDGUILD_OK"],
        10304 => &["RM_BUILDGUILD_FAIL"],
        10305 => &["RM_DONATE_OK"],
        10306 => &["RM_DONATE_FAIL"],
        10309 => &["RM_MENU_OK"],
        10332 => &["RM_RECONNECTION"],
        10333 => &["RM_HIDEEVENT"],
        10334 => &["RM_SHOWEVENT"],
        10401 => &["RM_10401"],
        10410 => &["RM_OPENHEALTH"],
        10411 => &["RM_CLOSEHEALTH"],
        10413 => &["RM_BREAKWEAPON"],
        10414 => &["RM_ABILSEEHEALGAUGE"],
        10415 => &["RM_CHANGEFACE"],
        10416 => &["RM_PASSWORD"],
        10430 => &["CM_SPEEDHACKUSER"],
        10500 => &["RM_PLAYDICE"],
        11001 => &["RM_HEAR"],
        11002 => &["RM_WHISPER"],
        11003 => &["RM_CRY"],
        11004 => &["RM_SYSMESSAGE"],
        11005 => &["RM_GROUPMESSAGE"],
        11006 => &["RM_SYSMESSAGE2"],
        11007 => &["RM_GUILDMESSAGE"],
        11008 => &["RM_SYSMESSAGE3"],
        11009 => &["RM_MERCHANTSAY"],
        11015 => &["RM_MARKET_LIST"],
        11016 => &["RM_MARKET_RESULT"],
        11029 => &["SM_SERVERCONFIG"],
        15999 => &["CM_CHECKTIME"],
        23000 => &["RM_SENDDEALOFFFORM"],
        23001 => &["SM_SENDDEALOFFFORM"],
        23002 => &["CM_SELLOFFADDITEM"],
        23003 => &["SM_SELLOFFADDITEM_OK"],
        23004 => &["RM_SELLOFFADDITEM_OK"],
        23005 => &["SM_SellOffADDITEM_FAIL"],
        23006 => &["RM_SellOffADDITEM_FAIL"],
        23007 => &["CM_SELLOFFDELITEM"],
        23008 => &["SM_SELLOFFDELITEM_OK"],
        23009 => &["RM_SELLOFFDELITEM_OK"],
        23010 => &["SM_SELLOFFDELITEM_FAIL"],
        23011 => &["RM_SELLOFFDELITEM_FAIL"],
        23012 => &["CM_SELLOFFCANCEL"],
        23013 => &["RM_SELLOFFCANCEL"],
        23014 => &["SM_SellOffCANCEL"],
        23015 => &["CM_SELLOFFEND"],
        23016 => &["SM_SELLOFFEND_OK"],
        23017 => &["RM_SELLOFFEND_OK"],
        23018 => &["SM_SELLOFFEND_FAIL"],
        23019 => &["RM_SELLOFFEND_FAIL"],
        23020 => &["RM_QUERYYBSELL"],
        23021 => &["SM_QUERYYBSELL"],
        23022 => &["RM_QUERYYBDEAL"],
        23023 => &["SM_QUERYYBDEAL"],
        23024 => &["CM_CANCELSELLOFFITEMING"],
        23025 => &["CM_SELLOFFBUYCANCEL"],
        23026 => &["CM_SELLOFFBUY"],
        23027 => &["SM_SELLOFFBUY_OK"],
        23028 => &["RM_SELLOFFBUY_OK"],
        30002 => &["SS_LOGINCOST"],
        41900 => &["SM_SMUGGLE"],
        41901 => &["SM_SMUGGLE_SUCESS"],
        41902 => &["CM_SMUGGLE"],
        41903 => &["CM_SMUGGLE_SUCESS"],
        41905 => &["CM_CHECKCLIENT_RES"],
        50003 => &["RM_HORSERUN"],
        50142 => &["RM_SENDUSERSREPAIR"],
        60000 => &["RM_UPDATEVIEWRANGE"],
        60001 => &["RM_PLAYERKILLMONSTER"],
        60002 => &["RM_DIEDROPITEM"],
        60003 => &["RM_MASTERDIEMUTINY"],
        60004 => &["RM_MASTERDIEGHOST"],
        60005 => &["RM_MAKEHOLYSEIZEMODE"],
        65023 => &["SM_EXCHGTAKEON_OK"],
        65024 => &["SM_EXCHGTAKEON_FAIL"],
        65037 => &["SM_TEST"],
        65070 => &["SM_ACTION_MIN"],
        65071 => &["SM_ACTION_MAX"],
        65072 => &["SM_ACTION2_MIN"],
        65073 => &["SM_ACTION2_MAX"],
        65074 => &["CM_SERVERREGINFO"],
        _ => &[],
    }
}
