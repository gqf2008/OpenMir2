//! Envir 内容清单的**宿主装载半**（R0-2 `MiniMap.txt` / R0-3 `MakeItem.txt`）。
//!
//! 解析半在 `crates/envir`（`minimap.rs` / `makeitem.rs` 的逐行 1:1）；本模块只做
//! 「文件名口径 + `File.Exists` 门 + GBK 解码」的宿主侧接线，与
//! [`crate::guardlist::load_guard_list_file`] 同款：
//!
//! - [`load_minimap_file`] = `Maps.LoadMinMap`（`Maps.cs:387-414`）的文件半：
//!   `M2Share.GetEnvirFilePath("MiniMap.txt")` + `File.Exists`（`:393-394`）；
//! - [`load_make_item_file`] = `LocalDB.LoadMakeItem`（`LocalDB.cs:149-190`）的文件半：
//!   `M2Share.GetEnvirFilePath("MakeItem.txt")` + `File.Exists`（`:154-155`）。
//!
//! `Ok(None)` = 文件不存在（oracle `File.Exists` 假 ⇒ 静默不装载）；
//! `Err` = 读失败 / 解析异常（MiniMap 解析不会失败；MakeItem 的重复组名 ⇒ 对应
//! `Dictionary.Add` 抛异常，宿主按 WRN 登记）。
//!
//! 消费点（**不在本片**，登记）：MiniMap 下发帧 `CM_WANTMINIMAP` → `SM_READMINIMAP_OK/FAIL`
//! 与 `Envirnoment.MinMap`；MakeItem → `CM_USERMAKEDRUGITEM` → `Merchant.ClientMakeDrugCheckNeedItem`
//! （两条都在 R1 入站红名单，Rust 侧未实现）。

use mir2_envir::paths::envir_file_path;
use mir2_envir::{decode_text, MakeItemRegistry, MapLoader, MiniMapRegistry};

/// `Maps.LoadMinMap` 的文件半（`Maps.cs:393-398`）。
///
/// 返回 `(注册表, 被丢弃的重复地图名)`——重复名列表供宿主复刻 `:411` 的
/// `LogService.Error("重复小地图配置信息[...]")`。
///
/// # Errors
/// 读文件失败（非 `NotFound`）。MiniMap 解析本身不产生 `Err`。
pub fn load_minimap_file(
    loader: &MapLoader,
) -> Result<Option<(MiniMapRegistry, Vec<String>)>, String> {
    let path = envir_file_path(&loader.base_path, &loader.envir_dir, "", "MiniMap.txt");
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("读 {} 失败：{e}", path.display())),
    };
    Ok(Some(mir2_envir::load_minimap(&decode_text(&bytes))))
}

/// `LocalDB.LoadMakeItem` 的文件半（`LocalDB.cs:154-158`）。
///
/// `Ok(None)` = 无文件（oracle `File.Exists` 门 ⇒ 空表）。
///
/// # Errors
/// 读文件失败，或解析层冒泡的 `Err`（重复组名 ⇒ `Dictionary.Add` 抛异常，`LocalDB.cs:170/187`）。
pub fn load_make_item_file(loader: &MapLoader) -> Result<Option<MakeItemRegistry>, String> {
    let path = envir_file_path(&loader.base_path, &loader.envir_dir, "", "MakeItem.txt");
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("读 {} 失败：{e}", path.display())),
    };
    mir2_envir::parse_make_item_lines(&decode_text(&bytes))
        .map(Some)
        .map_err(|e| format!("解析 MakeItem.txt 失败：{e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mir2_envir::MakeItem;

    /// 用例级唯一临时目录序号（照 `guardlist.rs` 测试的 pid+原子计数口径）。
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
                "gs-contentload-{}-{}",
                std::process::id(),
                COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
            std::fs::create_dir_all(base.join("Envir")).unwrap();
            Fixture { base }
        }

        fn loader(&self) -> MapLoader {
            MapLoader::new(self.base.clone(), "Map", "Envir")
        }
    }

    /// 文件半：无 `MiniMap.txt` ⇒ `Ok(None)`（oracle `File.Exists` 门 ⇒ 不装载）。
    #[test]
    fn minimap_missing_is_none() {
        let f = Fixture::new();
        assert!(load_minimap_file(&f.loader()).unwrap().is_none());
    }

    /// 文件半：部署件形态（ASCII + CRLF + 空行）⇒ 逐字段解析 + 重复丢弃。
    #[test]
    fn minimap_reads_deployed_shape() {
        let f = Fixture::new();
        // 部署件前几行 + 一处重复（大小写不同）+ 一处空行。
        let body = b"D001  1\r\nD002  2\r\n\r\n631  200\r\n64   202\r\nd001  999\r\n";
        std::fs::write(f.base.join("Envir").join("MiniMap.txt"), body).unwrap();
        let (reg, dups) = load_minimap_file(&f.loader()).unwrap().expect("Some");
        assert_eq!(reg.len(), 4, "5 行里 1 行重复被丢");
        assert_eq!(reg.get("D001"), Some(1), "先到保留");
        assert_eq!(reg.get("631"), Some(200));
        assert_eq!(reg.get("64"), Some(202));
        assert_eq!(dups, vec!["d001".to_string()]);
    }

    /// 文件半：无 `MakeItem.txt` ⇒ `Ok(None)`。
    #[test]
    fn make_item_missing_is_none() {
        let f = Fixture::new();
        assert!(load_make_item_file(&f.loader()).unwrap().is_none());
    }

    /// 文件半：部署件 `MakeItem.txt` 的 **GBK 字节**（`[赤血魔剑]` = `5B B3E0 D1AA C4A7 BDA3 5D`）
    /// ⇒ GBK 解码 + 逐字段解析。
    #[test]
    fn make_item_reads_deployed_gbk_shape() {
        let f = Fixture::new();
        let mut body: Vec<u8> = vec![b'['];
        // 赤血魔剑（GBK = B3E0 D1AA C4A7 BDA3）
        body.extend_from_slice(&[0xB3, 0xE0, 0xD1, 0xAA, 0xC4, 0xA7, 0xBD, 0xA3]);
        body.push(b']');
        body.extend_from_slice(b"\r\n");
        // 罗刹\t1（罗刹 = C2DE C9B2）
        body.extend_from_slice(&[0xC2, 0xDE, 0xC9, 0xB2]);
        body.extend_from_slice(b"\t1\r\n");
        // 攻击神水\t1（攻击神水 = B9 A5 BB F7 C9 F1 CB AE）
        body.extend_from_slice(&[0xB9, 0xA5, 0xBB, 0xF7, 0xC9, 0xF1, 0xCB, 0xAE]);
        body.extend_from_slice(b"\t1");
        std::fs::write(f.base.join("Envir").join("MakeItem.txt"), &body).unwrap();

        let reg = load_make_item_file(&f.loader()).unwrap().expect("Some");
        assert_eq!(reg.len(), 1);
        let items = reg.get("赤血魔剑").expect("GBK 组名解码");
        assert_eq!(
            items,
            [
                MakeItem {
                    item_name: "罗刹".to_string(),
                    item_count: 1
                },
                MakeItem {
                    item_name: "攻击神水".to_string(),
                    item_count: 1
                },
            ]
        );
    }

    /// 文件半：重复组名 ⇒ `Err`（`Dictionary.Add` 抛异常的对照；宿主按 WRN 登记）。
    #[test]
    fn make_item_duplicate_group_is_err() {
        let f = Fixture::new();
        std::fs::write(
            f.base.join("Envir").join("MakeItem.txt"),
            "[A]\n甲 1\n[A]\n乙 2\n",
        )
        .unwrap();
        let err = load_make_item_file(&f.loader()).unwrap_err();
        assert!(err.contains("MakeItem.txt"), "错误信息应点名文件：{err}");
    }
}
