//! MonItems 挂接的判据（卡 `openmir2-m3-monitems-attach`，写集 `crates/data/**` + `bins/game-svr/**`）。
//!
//! oracle 锚点：`MySqlDB.LoadMonsterDB`（`src/GameSrv/DB/MySqlDB.cs:226`）在 `select * from monsters`
//! 的行循环里 `monster.ItemList = null;` 后调 `LocalDB.LoadMonitems(name, ref ItemList)`
//! （`src/GameSrv/DB/LocalDB.cs:596`，GBK，路径 = `GetEnvirFilePath("MonItems", "<name>.txt")`）。
//!
//! 判据（构造级，离线可跑；不需要起栈/端口）：
//! 1. **挂表前后 `ItemList` 内容**：先置空再按文件存在性挂接——缺文件 ⇒ `None`、空文件 ⇒
//!    `Some(空)`、有表 ⇒ 与独立 `parse_monitems_gbk` 逐字段相同；顺序/名字不被改动。
//! 2. **缺表/坏表 fail-closed**：坏表（存在但读不了）⇒ `Err`，**不**静默降级成「无掉落」。
//! 3. **真数据覆盖（`#[ignore]`，需本机 MySQL + `E:/MirServer`）**：对 705 只怪逐只核
//!    `item_list.is_some() == 磁盘上文件存在`，且 `report.attached` 与独立计数一致。
//!
//! 「改坏必红」见证据包 `evidence-m3-monitems-attach-20261013/03-mutation-red-*.txt`：
//! 把 `attach_monitems` 的先置空删掉（或把 `is_file` 换成恒真）⇒ 本文件用例必 FAIL。

use std::path::PathBuf;

use mir2_data::models::MonsterInfo;
use mir2_data::monitems::{attach_monitems, AttachReport, MONITEMS_DIR};

/// 夹具统一走 GBK 编码（MonItems 真实文件格式）。
fn gbk(s: &str) -> Vec<u8> {
    encoding_rs::GBK.encode(s).0.into_owned()
}

/// 用例级唯一临时 Envir 目录（跟随仓库既有写法：`std::env::temp_dir` + 进程号，无外部依赖）。
fn temp_envir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("m3-monitems-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join(MONITEMS_DIR)).unwrap();
    dir
}

fn monster(name: &str) -> MonsterInfo {
    MonsterInfo {
        name: name.to_string(),
        ..MonsterInfo::default()
    }
}

fn stale() -> Option<Vec<mir2_data::MonsterDropItem>> {
    Some(vec![mir2_data::MonsterDropItem {
        sel_point: 0,
        max_point: 1,
        item_name: "脏数据".to_string(),
        count: 1,
    }])
}

/// 判据①：挂表前后 `ItemList` 内容（构造级）。
#[test]
fn attach_resets_and_matches_each_table_field_by_field() {
    let envir = temp_envir("attach");
    let mitems = envir.join(MONITEMS_DIR);
    let table = "; 注释行\n10/100     金币 300\n20/300  \"裁决之杖\" 2\n";
    std::fs::write(mitems.join("稻草人.txt"), gbk(table)).unwrap();
    // 空文件：oracle `new List<MonsterDropItem>()` ⇒ Some(空)，与「不存在」的 None 不同。
    std::fs::write(mitems.join("空表怪.txt"), b"").unwrap();
    // 同名目录：oracle `File.Exists` 为假 ⇒ 视同不存在（不得报 IO 错）。
    std::fs::create_dir_all(mitems.join("目录怪.txt")).unwrap();

    // 4 只怪，全部预置脏 ItemList —— 验证 oracle 的 `ItemList = null` 先置空语义。
    let mut monsters = vec![
        monster("稻草人"),
        monster("无表怪"),
        monster("空表怪"),
        monster("目录怪"),
    ];
    for m in &mut monsters {
        m.item_list = stale();
    }

    let report = attach_monitems(&mut monsters, &envir).unwrap();
    assert_eq!(
        report,
        AttachReport {
            total: 4,
            attached: 2,
            entries: 2
        }
    );

    // 顺序/名字不被改动（oracle 在 DB 行循环里就地挂接）。
    let names: Vec<&str> = monsters.iter().map(|m| m.name.as_str()).collect();
    assert_eq!(names, ["稻草人", "无表怪", "空表怪", "目录怪"]);

    // 有表 ⇒ 与独立解析逐字段相同（不是「只数个数」）。
    let expected = mir2_data::monitems::load_monitems(&mitems.join("稻草人.txt")).unwrap();
    assert_eq!(expected.len(), 2);
    assert_eq!(monsters[0].item_list.as_ref().unwrap(), &expected);
    // 缺文件 / 同名目录 ⇒ None（脏值不得残留）。
    assert_eq!(monsters[1].item_list, None);
    assert_eq!(monsters[3].item_list, None);
    // 空文件 ⇒ Some(空)，不是 None。
    assert_eq!(monsters[2].item_list, Some(Vec::new()));

    std::fs::remove_dir_all(&envir).unwrap();
}

/// 判据②：坏表（存在但读不了）⇒ fail-closed `Err`，不静默当「无掉落」。
#[cfg(windows)]
#[test]
fn attach_fails_closed_on_unreadable_table() {
    use std::os::windows::fs::OpenOptionsExt;

    let envir = temp_envir("badtable");
    let path = envir.join(MONITEMS_DIR).join("锁表怪.txt");
    std::fs::write(&path, gbk("10/100 金币 1\n")).unwrap();
    // share_mode(0) = FILE_SHARE_NONE ⇒ 任何后继 open 得 ERROR_SHARING_VIOLATION。
    let held = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(&path)
        .unwrap();

    let mut monsters = vec![monster("锁表怪")];
    monsters[0].item_list = stale();
    let err = attach_monitems(&mut monsters, &envir).unwrap_err();
    // 必须是**真的 IO 错误**（Windows 共享冲突 → OS 错误码），不是被静默成 None。
    assert!(
        err.raw_os_error().is_some(),
        "期望 OS 级 IO 错误，得 {err:?}"
    );
    // fail-closed：坏表时停在「已置空」的 None，绝不伪造成 Some(空)。
    assert_eq!(monsters[0].item_list, None);

    drop(held);
    std::fs::remove_dir_all(&envir).unwrap();
}

/// 判据③（真数据覆盖，`#[ignore]`）：705 只怪逐只核 `Some ⇔ 磁盘有文件`。
///
/// 需要本机 MySQL（`MIR2_DATA_URL`，缺省 `mysql://root@127.0.0.1:3306/mir2_data`）与
/// `E:/MirServer/M2GameSvr/Envir`（可用 `MIR2_MONITEMS_ENVIR` 覆盖）。
/// 显式运行：
/// `cargo test -p game-svr --test monitems_attach -- --ignored --nocapture`
#[tokio::test]
#[ignore = "需要本机 MySQL + E:/MirServer Envir；显式 --ignored 运行"]
async fn real_monster_db_every_monster_matches_its_table_on_disk() {
    let url = std::env::var("MIR2_DATA_URL")
        .unwrap_or_else(|_| "mysql://root@127.0.0.1:3306/mir2_data".to_string());
    let envir = PathBuf::from(
        std::env::var("MIR2_MONITEMS_ENVIR")
            .unwrap_or_else(|_| "E:/MirServer/M2GameSvr/Envir".to_string()),
    );
    let mitems = envir.join(MONITEMS_DIR);
    assert!(mitems.is_dir(), "MonItems 目录不存在：{}", mitems.display());

    let pool = sqlx::mysql::MySqlPoolOptions::new()
        .max_connections(4)
        .connect(&url)
        .await
        .expect("连接 mir2_data 失败");
    // MonsterPowerRate=10（现网 Server.conf）
    let mut monsters = mir2_data::loaders::DataLoaders::new(pool, 10)
        .load_monster_db()
        .await
        .expect("monsters 加载失败");
    assert!(
        monsters.len() >= 700,
        "monsters 条数异常：{}",
        monsters.len()
    );
    // 挂表**前**：`load_monster_db` 只读 DB、不读 Envir ⇒ 一律 None。
    assert!(
        monsters.iter().all(|m| m.item_list.is_none()),
        "load_monster_db 阶段不应挂 MonItems（挂接归 attach_monitems）"
    );

    let report = attach_monitems(&mut monsters, &envir).expect("挂接失败");
    assert_eq!(report.total, monsters.len());

    // 独立计数（不信任被测实现）：逐只查磁盘文件是否存在。
    let mut independent_attached = 0usize;
    let mut independent_entries = 0usize;
    for m in &monsters {
        let p = mitems.join(format!("{}.txt", m.name));
        match m.item_list.as_ref() {
            Some(list) => {
                assert!(
                    p.is_file(),
                    "怪物[{}] 挂了表但磁盘无文件：{}",
                    m.name,
                    p.display()
                );
                // 有表 ⇒ 内容与独立解析逐字段相同。
                let expected = mir2_data::monitems::load_monitems(&p).unwrap();
                assert_eq!(list, &expected, "怪物[{}] 掉落表内容不一致", m.name);
                independent_attached += 1;
                independent_entries += expected.len();
            }
            None => {
                assert!(
                    !p.is_file(),
                    "怪物[{}] 磁盘有表 {} 但 item_list=None",
                    m.name,
                    p.display()
                );
            }
        }
    }
    assert_eq!(report.attached, independent_attached);
    assert_eq!(report.entries, independent_entries);

    // 信息列（不作判据）：磁盘上存在、但 DB 里没有同名怪的表。
    let orphans: Vec<String> = std::fs::read_dir(&mitems)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|e| e.path().is_file())
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            name.strip_suffix(".txt").map(str::to_string)
        })
        .filter(|stem| !monsters.iter().any(|m| &m.name == stem))
        .collect();
    println!(
        "MonItems 覆盖：monsters={} attached={} entries={} 磁盘孤儿表={:?}",
        report.total, report.attached, report.entries, orphans
    );
}

/// 判据③ 的离线半边：`envir_dir/MonItems` 之外的 `*.txt` 不参与挂接。
#[test]
fn attach_only_reads_from_monitems_subdir() {
    let envir = temp_envir("subdir");
    std::fs::write(envir.join("outside.txt"), gbk("10/100 金币 1\n")).unwrap();
    let mut monsters = vec![monster("outside")];
    let report = attach_monitems(&mut monsters, &envir).unwrap();
    assert_eq!(report.attached, 0);
    assert_eq!(monsters[0].item_list, None);

    std::fs::write(
        envir.join(MONITEMS_DIR).join("outside.txt"),
        gbk("10/100 金币 1\n"),
    )
    .unwrap();
    let report = attach_monitems(&mut monsters, &envir).unwrap();
    assert_eq!(report.attached, 1);
    assert_eq!(monsters[0].item_list.as_ref().unwrap().len(), 1);

    std::fs::remove_dir_all(&envir).unwrap();
}

/// `game_svr::monitems::with_monitems`（生产装载入口）与 `attach_monitems` 同口径，
/// 并把坏表错误包成人话（fail-closed）。
#[test]
fn with_monitems_wrapper_is_fail_closed() {
    let envir = temp_envir("wrapper");
    std::fs::write(
        envir.join(MONITEMS_DIR).join("好表怪.txt"),
        gbk("10/100 金币 1\n"),
    )
    .unwrap();
    let loaded =
        game_svr::monitems::with_monitems(vec![monster("好表怪"), monster("缺表怪")], &envir)
            .unwrap();
    assert_eq!(loaded.report.attached, 1);
    assert_eq!(loaded.monsters.len(), 2);
    assert!(loaded.monsters[0].item_list.is_some());
    assert!(loaded.monsters[1].item_list.is_none());
    std::fs::remove_dir_all(&envir).unwrap();

    // MonItems 目录为空 ⇒ 所有怪 None（oracle `File.Exists` 为假），不是错误。
    let missing = temp_envir("wrapper-missing");
    let loaded = game_svr::monitems::with_monitems(vec![monster("好表怪")], &missing).unwrap();
    assert_eq!(loaded.report.attached, 0);
    std::fs::remove_dir_all(&missing).unwrap();
}
