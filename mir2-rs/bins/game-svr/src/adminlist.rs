//! `Envir/AdminList.txt` 的**宿主装载半 + `GetHumPermission` 查表半**
//! （卡 `openmir2-envir-adminlist-consumer`）。
//!
//! 解析半在 `crates/data/src/lists.rs`（`LocalDb.LoadAdminList`，`DB/LocalDB.cs:16-96` 的逐行
//! 1:1，含 `GetValidStrCap` 的越界语义）；本模块只做「文件名口径 + `File.Exists` 门 + GBK 解码」
//! 的宿主侧接线（与 [`crate::guardlist`] / [`crate::contentload`] 同款），外加 oracle
//! `WorldServer.GetHumPermission`（`Word/WorldServer.cs:1021-1037`）的查表半。
//!
//! ## oracle 事实（逐条实读；决定本片**不**接「权限生效」）
//!
//! 1. **装载是活的**：`GameApp.cs:209` 在 `Initialize` 里无条件调 `LocalDb.LoadAdminList()`，
//!    `:210` 紧接 `LogService.Info("管理员列表加载成功...")`（与返回值无关）。装进
//!    `SystemShare.WorldEngine.AdminList`（`WorldServer.cs:38`，`:82` 初始化）。
//! 2. **读取是死的**：`GetHumPermission` 的**唯一**调用点在 `PlayObject.cs:1188-1194`，整段被
//!    注释 ⇒ `PlayObject.Permission`（`PlayObject.cs:58`，auto-property 无初值 ⇒ 默认 0）
//!    **永远拿不到 AdminList 的等级**。⇒ 现网 GM 权限恒 0（`gmcmd.rs` 的
//!    `GmSession::permission = 0` 是本事实的 Rust 侧落点），185 条命令里只有 perm 0 的 5 条 +
//!    内建 `@help` 可达。
//! 3. **部署件本身也产不出行**：`Envir/AdminList.txt` 全文只有一行 `*<TAB>阿瑞斯乐园`（14 字节，
//!    无 IP 段）⇒ `LoadAdminList` 的空字段门（`LocalDB.cs:80-82`）把整行 `continue` 掉 ⇒
//!    **表恒空**。
//! 4. **热重载同样被注释**：`LoadAdminCommand.cs:19`（现网 `@LoadAdmin`）与
//!    `ReLoadAdminCommand.cs:16`（`@重读GM`）的 `LoadAdminList()` 都被注释 ⇒ 本模块**不**提供
//!    运行期重载入口（`gmcmd.rs` 已把两条命令按「纯文本/未接」照抄）。
//!
//! ## 本片边界
//!
//! 复刻「装进表 + 提供查表」两半；**不**把 [`AdminList::get_hum_permission`] 接到任何
//! `Permission` 写点（oracle 没有这个行为，接上就是发明逻辑，须另开 `D-*` 偏差卡）。因此
//! `EnterConfig` 的 `admin_list` 字段是「装载即登记」的载体，与 oracle 的
//! `WorldEngine.AdminList` 同形：**装载是活的、读取是死的**。
//!
//! `[Setup] StartPermission`（缺省 0，`GameSvrConf.cs:1466`；部署 `Server.conf:365` = 0）本仓
//! **未解析**，由调用方按 oracle 缺省值 0 传入（只有两处不一致才可能有差；都是 0 ⇒ 行为等价，登记）。

use mir2_data::lists::{AdminInfo, ADMIN_LIST_FILE};
use mir2_envir::paths::envir_file_path;
use mir2_envir::MapLoader;

/// `SystemShare.WorldEngine.AdminList` 的宿主读数。
///
/// 把 `GetHumPermission` 的两个 `ref` 出参 + 布尔返回值合成一个结构。
///
/// `found=true` ⇒ `permission`/`ipaddr` 来自该行（oracle `WorldServer.GetHumPermission` 的
/// `btPermission = adminInfo.Level; sIPaddr = adminInfo.IPaddr;`）。
///
/// `found=false` ⇒ `permission` 保持 `Config.StartPermission`（oracle 在循环**之前**先赋该值）；
/// `ipaddr` —— oracle 不改写 `ref sIPaddr`，调用方传入值原样返回。**本仓固定给空串**：该调用点
/// 在 oracle 里整段被注释（`PlayObject` 的 `//if (... ref sIPaddr, ref Permission)`），且
/// `sIPaddr` 在 `PlayObject.cs` 全文**只出现在那两行注释里、没有声明**（注释块即便取消注释也
/// 编译不过）⇒「调用方初值」没有可引用的 oracle 事实，空串是本仓的表示选择（登记）。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HumPermission {
    /// `GetHumPermission` 的 `bool` 返回值：是否命中名表。
    pub found: bool,
    /// 命中行的 `AdminInfo.IPaddr`；未命中为**空串**（见上）。
    pub ipaddr: String,
    /// 命中行的 `AdminInfo.Level`；未命中为 `Config.StartPermission`。
    pub permission: u8,
}

/// `SystemShare.WorldEngine.AdminList`（`WorldServer.cs:38`）的 Rust 承载：装载结果 + 查表半。
///
/// **现网无生效消费点**（模块头事实 2）——保留此类型是为了让「装载 → 可查」这条链在 Rust 侧
/// 与 oracle 同形；将来若开 `D-*` 卡接 `Permission` 写点，只需在登录支调
/// [`AdminList::get_hum_permission`]。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AdminList {
    /// `WorldEngine.AdminList`（`List<AdminInfo>`，按下标保序）。
    pub admins: Vec<AdminInfo>,
}

impl AdminList {
    /// 用装载结果构造（宿主装载半的产物）。
    #[must_use]
    pub fn new(admins: Vec<AdminInfo>) -> Self {
        AdminList { admins }
    }

    /// 行数（日志口径：`AdminList.Count`）。
    #[must_use]
    pub fn len(&self) -> usize {
        self.admins.len()
    }

    /// 空表（oracle `Count == 0`）。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.admins.is_empty()
    }

    /// `WorldServer.GetHumPermission`（`WorldServer.cs:1021-1037`）的逐行 1:1。
    ///
    /// ```text
    /// btPermission = Config.StartPermission;          // :1024
    /// for (i = 0; i < AdminList.Count; i++) {         // :1025
    ///     if (Compare(adminInfo.ChrName, sUserName, OrdinalIgnoreCase) == 0) {  // :1028
    ///         btPermission = adminInfo.Level; sIPaddr = adminInfo.IPaddr;
    ///         result = true; break;                   // :1030-1033
    ///     }
    /// }
    /// ```
    ///
    /// `start_permission` = `SystemShare.Config.StartPermission`（未命中时的缺省等级）。
    ///
    /// **比较口径（域限定；登记见 `tests/parity/whitelist.md` 的 `T-173`）**：oracle 是
    /// `string.Compare(a, b, StringComparison.OrdinalIgnoreCase)`，.NET 在 ICC 上做的是
    /// **Unicode 简单大小写折叠**（实测 .NET 8：`Ä`/`ä`、`Σ`/`σ`、`É`/`é`、全角 `Ａ`/`ａ`
    /// 均相等）；本仓 `mir2_data::lists::lookup_admin` 用 `eq_ignore_ascii_case`，
    /// **只折叠 ASCII** ⇒ **仅 ASCII + GBK 汉字语料等价**（汉字无大小写映射；GBK 双字节亦不参与
    /// ASCII 折叠），**非 ASCII 大小写不折叠，属已登记近似**（`Ä` 与 `ä` 在本仓不等、在 oracle 相等）。
    /// 差异由 [`tests::lookup_folds_ascii_only`] 显式钉住。
    #[must_use]
    pub fn get_hum_permission(&self, user_name: &str, start_permission: u8) -> HumPermission {
        match mir2_data::lists::lookup_admin(&self.admins, user_name) {
            Some(admin) => HumPermission {
                found: true,
                ipaddr: admin.ipaddr.clone(),
                permission: admin.level,
            },
            None => HumPermission {
                found: false,
                ipaddr: String::new(),
                permission: start_permission,
            },
        }
    }
}

/// `LocalDb.LoadAdminList`（`DB/LocalDB.cs:16-96`）的**文件半**。
///
/// `M2Share.GetEnvirFilePath("AdminList.txt")` = [`envir_file_path`]；`File.Exists` 假 ⇒
/// `Ok(None)`（oracle `:22-25` 直接 `return false`，且**不清表**）。读入后走
/// `mir2_data::lists::load_admin_list_file`（`StringList.LoadFromFile` 口径：BOM 感知 / 无 BOM
/// 按 cp936，行切分同 `ReadLine`）。
///
/// **行切分缺口（引用既有登记，本片不修）**：`ReadLine` 把**孤立 `\r`**（非 CRLF）也当换行，
/// Rust 侧用 `str::lines()`，不按孤立 CR 切行 ⇒ 含孤立 CR 的文件会少切行。该缺口在解析层
/// 模块头的「刻意偏离 4」（`crates/data/src/lists.rs` 注释体第 4 条，原文「oracle 的 `\r`
/// （单独 CR，非 CRLF）行切分未覆盖」，位于该文件 `use` 语句之前）与本表白名单 `T-149`/`T-150`
/// 均已登记；部署件 `AdminList.txt` 为 `\r\n` ⇒ 不可达。
///
/// # Errors
/// 读失败（非 `NotFound`）或行级 `CaptureString`（行内前置 `"` ⇒ oracle
/// `IndexOutOfRangeException`；`GameApp.cs:209` 调用点无 try/catch ⇒ 宿主按 WRN 登记、表置空）。
pub fn load_admin_list_file(loader: &MapLoader) -> Result<Option<AdminList>, String> {
    let path = envir_file_path(&loader.base_path, &loader.envir_dir, "", ADMIN_LIST_FILE);
    match mir2_data::lists::load_admin_list_file(&path) {
        Ok(Some(admins)) => Ok(Some(AdminList::new(admins))),
        Ok(None) => Ok(None),
        Err(e) => Err(format!("加载 {} 失败：{e}", path.display())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 用例级唯一临时目录序号（照 `guardlist.rs`/`contentload.rs` 测试的 pid+原子计数口径）。
    static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

    struct Fixture {
        base: std::path::PathBuf,
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.base);
        }
    }

    impl Fixture {
        fn new() -> Self {
            let base = std::env::temp_dir().join(format!(
                "gs-adminlist-{}-{}",
                std::process::id(),
                COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
            std::fs::create_dir_all(base.join("Envir")).unwrap();
            Fixture { base }
        }

        fn loader(&self) -> MapLoader {
            MapLoader::new(self.base.clone(), "Map", "Envir")
        }

        fn write(&self, body: &[u8]) {
            std::fs::write(self.base.join("Envir").join(ADMIN_LIST_FILE), body).unwrap();
        }
    }

    /// 三态 ①：**无文件** ⇒ `Ok(None)`（oracle `File.Exists` 假 ⇒ `return false`、不清表）。
    #[test]
    fn admin_list_missing_is_none() {
        let f = Fixture::new();
        assert!(load_admin_list_file(&f.loader()).unwrap().is_none());
    }

    /// 三态 ②：**空文件** ⇒ `Ok(Some(空表))`（文件在 ⇒ oracle 走装载、清表后 0 行）。
    #[test]
    fn admin_list_empty_file_is_empty_table() {
        let f = Fixture::new();
        f.write(b"");
        let list = load_admin_list_file(&f.loader()).unwrap().expect("Some");
        assert!(list.is_empty(), "空文件 ⇒ 0 行");
        assert_eq!(list.len(), 0);
    }

    /// 三态 ③：**唯一行缺 IP 字段被丢弃** —— 部署件实况 `*<TAB>阿瑞斯乐园`（14 字节 GBK）。
    ///
    /// `GetValidStrCap` 取完 `ChrName` 后 `sIPaddr` 为空 ⇒ `LocalDB.cs:80-82` 的
    /// `if (isEmpty(ChrName) || isEmpty(IPaddr)) continue;` 丢弃整行 ⇒ 表恒空。
    /// 红检（改坏必红）：删掉该 `continue` 分支 ⇒ 本用例 `is_empty()` 必红。
    #[test]
    fn admin_list_deployed_single_line_without_ip_is_dropped() {
        let f = Fixture::new();
        // `*\t阿瑞斯乐园\r\n`（GBK：260 242 310 360 313 271 300 326 324 260）。
        f.write(b"*\t\xb0\xa2\xc8\xf0\xcb\xb9\xc0\xd6\xd4\xb0\r\n");
        let list = load_admin_list_file(&f.loader()).unwrap().expect("Some");
        assert!(
            list.is_empty(),
            "缺 IP 段 ⇒ 整行丢弃（LocalDB.cs:80-82），得 {} 行",
            list.len()
        );
    }

    /// 正常行：`*` → 10、`1` → 9；字段 = (ChrName, IPaddr)，首个 token（级别字符）丢弃。
    #[test]
    fn admin_list_normal_lines_parse_fields() {
        let f = Fixture::new();
        f.write(b"*\tGM01\t127.0.0.1\r\n1 GM02 10.0.0.2\r\n;comment\r\n");
        let list = load_admin_list_file(&f.loader()).unwrap().expect("Some");
        assert_eq!(list.len(), 2, "';' 行跳过");
        assert_eq!(
            list.admins[0],
            AdminInfo {
                level: 10,
                chr_name: "GM01".to_string(),
                ipaddr: "127.0.0.1".to_string(),
            },
            "`*` ⇒ 10；TAB 与空格同为分隔符"
        );
        assert_eq!(list.admins[1].level, 9, "`1` ⇒ 9");
        assert_eq!(list.admins[1].chr_name, "GM02");
        assert_eq!(list.admins[1].ipaddr, "10.0.0.2");
    }

    /// `GetHumPermission`：命中 ⇒ 该行等级 + IP；未命中 ⇒ `StartPermission` + 空 IP。
    #[test]
    fn get_hum_permission_hits_and_misses() {
        let list = AdminList::new(vec![AdminInfo {
            level: 10,
            chr_name: "GM01".to_string(),
            ipaddr: "127.0.0.1".to_string(),
        }]);
        let hit = list.get_hum_permission("gm01", 0);
        assert!(hit.found, "OrdinalIgnoreCase ⇒ `gm01` 命中 `GM01`");
        assert_eq!(hit.permission, 10);
        assert_eq!(hit.ipaddr, "127.0.0.1");
        let miss = list.get_hum_permission("nobody", 0);
        assert!(!miss.found);
        assert_eq!(
            miss.permission, 0,
            "未命中 ⇒ Config.StartPermission（部署 = 0）"
        );
        assert_eq!(miss.ipaddr, "", "未命中 ⇒ sIPaddr 保持调用方初值（空串）");
    }

    /// 口径钉住（登记 `T-173`）：本仓 `eq_ignore_ascii_case` **只折叠 ASCII**，不等价 .NET
    /// `OrdinalIgnoreCase` 的 Unicode 大小写折叠（`Ä`/`ä`、`Σ`/`σ`、`É`/`é`、全角 `Ａ`/`ａ`
    /// 在 .NET 8 下相等，在本仓不等）。
    ///
    /// 现网语料（ASCII + GBK 汉字）下两者判定相同；本用例把「非 ASCII 分叉」钉成**显式事实**——
    /// 将来若改成完整 Unicode 折叠，本用例会红 ⇒ 必须同步更新 `T-173`。
    #[test]
    fn lookup_folds_ascii_only() {
        let list = AdminList::new(vec![AdminInfo {
            level: 10,
            chr_name: "\u{00C4}rger".to_string(), // `Ärger`
            ipaddr: "127.0.0.1".to_string(),
        }]);
        assert!(
            !list.get_hum_permission("\u{00E4}rger", 0).found,
            "Rust 侧只折叠 ASCII：U+00C4 vs U+00E4 不等（.NET OrdinalIgnoreCase 相等，见 T-173）"
        );
        // 对照组：ASCII 大小写仍然折叠（oracle 与 Rust 同）。
        let ascii = AdminList::new(vec![AdminInfo {
            level: 10,
            chr_name: "GM01".to_string(),
            ipaddr: "127.0.0.1".to_string(),
        }]);
        assert!(
            ascii.get_hum_permission("gm01", 0).found,
            "ASCII 大小写折叠两侧一致"
        );
    }

    /// 部署件实况端到端：唯一行被丢弃 ⇒ 查表恒未命中 ⇒ `Permission` 停在 `StartPermission`。
    ///
    /// 这就是「现网 Permission 恒 0」的完整链条（装载活、表空、读取死）。
    #[test]
    fn deployed_file_never_grants_permission() {
        let f = Fixture::new();
        f.write(b"*\t\xb0\xa2\xc8\xf0\xcb\xb9\xc0\xd6\xd4\xb0\r\n");
        let list = load_admin_list_file(&f.loader()).unwrap().expect("Some");
        let p = list.get_hum_permission("阿瑞斯乐园", 0);
        assert!(!p.found, "缺 IP 段 ⇒ 该行不入表 ⇒ 查不到");
        assert_eq!(p.permission, 0);
    }
}
