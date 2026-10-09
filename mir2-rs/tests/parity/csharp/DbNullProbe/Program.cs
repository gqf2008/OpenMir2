using System;
using MySqlConnector;

// 只读探针：stditems/monsters/magics/goldsales 各被读列的 NULL 分布 + 表更新时间
using var conn = new MySqlConnection("server=127.0.0.1;uid=root;pwd=;database=mir2_data;");
conn.Open();

void Scalar(string label, string sql)
{
    using var cmd = new MySqlCommand(sql, conn);
    var v = cmd.ExecuteScalar();
    Console.WriteLine($"{label}: {v}");
}

string[] itemCols = { "Name","StdMode","Shape","Weight","AniCount","Source","Reserved","ImgIndex","DuraMax",
    "Ac","AcMax","Mac","MacMax","Dc","DcMax","Mc","McMax","Sc","ScMax","Need","NeedLevel","Price","Stock",
    "Atkspd","AgilIty","Accurate","Mgavoid","Strong","Undead","HpAdd","MpAdd","ExpAdd",
    "Efftype1","Effrate1","Effvalue1","Efftype2","Effrate2","Effvalue2","SlowDown","Tox","ToxAvoid",
    "UniqueItem","OverlapItem","LIGHT","ItemType","ItemSet","Reference" };
foreach (var c in itemCols)
    Scalar($"stditems NULL({c})", $"SELECT COUNT(*) FROM stditems WHERE `{c}` IS NULL");

Scalar("stditems rows", "SELECT COUNT(*) FROM stditems");
Scalar("monsters rows", "SELECT COUNT(*) FROM monsters");
Scalar("monsters NULL 分布", "SELECT CONCAT_WS(',', SUM(`Name` IS NULL), SUM(`HP` IS NULL), SUM(`WaLkWait` IS NULL)) FROM monsters");
Scalar("magics rows", "SELECT COUNT(*) FROM magics");
Scalar("magics NULL 分布", "SELECT CONCAT_WS(',', SUM(`MagID` IS NULL), SUM(`MagName` IS NULL), SUM(`Descr` IS NULL), SUM(`MagID`=0)) FROM magics");
Scalar("goldsales rows", "SELECT COUNT(*) FROM goldsales");
Scalar("stditems UPDATE_TIME", "SELECT IFNULL(UPDATE_TIME,'n/a') FROM information_schema.TABLES WHERE TABLE_SCHEMA='mir2_data' AND TABLE_NAME='stditems'");
