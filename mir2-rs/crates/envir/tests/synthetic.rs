//! 合成件判据（卡 `openmir2-m2-map-loading` §3）。
//!
//! 全部按定案九条**在测试里生成 3×3 字节**（不依赖仓外文件），断言：
//! - `(0,0)` 只有 `wBkImg` bit15 ⇒ HighWall；只有 `wMidImg` ⇒ Walk；只有 `wFrImg` ⇒ LowWall；
//!   `wBkImg|wFrImg` ⇒ LowWall；三者全置位 ⇒ LowWall；
//! - `k=1` 单点 ⇒ 落在 `(0,1)`（X 主序）；
//! - 门字节 9 种取值 + doormerge 三门的共享关系；
//! - 红检：把覆盖顺序写反 / 把 `wMidImg` 当属性 ⇒ 判据必须能分辨（见 `red_check_*`）。

use mir2_envir::{
    parse_map_bytes, parse_points, CellAttribute, MapFile, CELL_BYTES, DOOR_BIT, HEADER_BYTES,
    WALL_BIT,
};

/// 按部署件布局生成 `.map` 字节：头 52B 只写 `int16 W/H`，随后按文件序（`k = x*H + y`）每格 12B。
fn synth(width: u16, height: u16, words: &[(u16, u16, u16)], doors: &[u8]) -> Vec<u8> {
    let n = usize::from(width) * usize::from(height);
    assert_eq!(words.len(), n, "格数不符");
    assert_eq!(doors.len(), n, "门字节数不符");
    let mut buf = vec![0u8; HEADER_BYTES + n * CELL_BYTES];
    buf[0..2].copy_from_slice(&width.to_le_bytes());
    buf[2..4].copy_from_slice(&height.to_le_bytes());
    for (k, ((bk, mid, fr), door)) in words.iter().zip(doors).enumerate() {
        let o = HEADER_BYTES + k * CELL_BYTES;
        buf[o..o + 2].copy_from_slice(&bk.to_le_bytes());
        buf[o + 2..o + 4].copy_from_slice(&mid.to_le_bytes());
        buf[o + 4..o + 6].copy_from_slice(&fr.to_le_bytes());
        buf[o + 6] = *door;
    }
    buf
}

fn load(bytes: &[u8]) -> MapFile {
    parse_map_bytes(bytes).expect("合成件应加载成功")
}

/// 定案①②③④：坐标序 + word→属性 + 覆盖顺序。
#[test]
fn pattern_words_and_override() {
    let words = [
        (0x8000, 0, 0),           // k0 (0,0) => HighWall
        (0, 0x8000, 0),           // k1 (0,1) => Walk（wMidImg 忽略）
        (0, 0, 0x8000),           // k2 (0,2) => LowWall
        (0x8000, 0x8000, 0),      // k3 (1,0) => HighWall
        (0x8000, 0, 0x8000),      // k4 (1,1) => LowWall（Low 覆盖 High）
        (0, 0x8000, 0x8000),      // k5 (1,2) => LowWall
        (0x8000, 0x8000, 0x8000), // k6 (2,0) => LowWall
        (0, 0, 0),                // k7 (2,1) => Walk
        (0x8000, 0, 0),           // k8 (2,2) => HighWall + 门
    ];
    let mut doors = vec![0u8; 9];
    doors[8] = 0x85;
    let m = load(&synth(3, 3, &words, &doors));

    assert_eq!((m.width, m.height), (3, 3));
    assert_eq!(m.attribute(0, 0), CellAttribute::HighWall);
    assert_eq!(
        m.attribute(0, 1),
        CellAttribute::Walk,
        "wMidImg 不得产生属性"
    );
    assert_eq!(m.attribute(0, 2), CellAttribute::LowWall);
    assert_eq!(m.attribute(1, 0), CellAttribute::HighWall);
    assert_eq!(m.attribute(1, 1), CellAttribute::LowWall, "Low 覆盖 High");
    assert_eq!(m.attribute(1, 2), CellAttribute::LowWall);
    assert_eq!(
        m.attribute(2, 0),
        CellAttribute::LowWall,
        "全置位 ⇒ LowWall"
    );
    assert_eq!(m.attribute(2, 1), CellAttribute::Walk);
    assert_eq!(m.attribute(2, 2), CellAttribute::HighWall);

    // 计数（high 只计未被 Low 覆盖者）：k0/k3/k8 = 3，low = k2/k4/k5/k6 = 4，walk = k1/k7 = 2。
    assert_eq!(m.high_count(), 3);
    assert_eq!(m.low_count(), 4);
    assert_eq!(m.walk_count(), 2);

    // k8 的门：0x85 ⇒ n08=5。
    assert_eq!(m.doors.len(), 1);
    assert_eq!((m.doors[0].x, m.doors[0].y, m.doors[0].n08), (2, 2, 5));
}

/// 定案②：单点图钉死文件序↔坐标（`k = x*H + y`，X 主序）。
#[test]
fn onehot_pins_file_order_to_coordinates() {
    // k=1 只在 wBkImg 置 bit15 ⇒ 必须落在 (0,1)。
    let mut words = vec![(0u16, 0u16, 0u16); 9];
    words[1].0 = WALL_BIT;
    let m = load(&synth(3, 3, &words, &[0u8; 9]));
    assert_eq!(m.attribute(0, 1), CellAttribute::HighWall);
    assert_eq!(m.attribute(1, 0), CellAttribute::Walk);
    assert_eq!(m.high_count(), 1);

    // k=3 ⇒ 必须落在 (1,0)。
    let mut words = vec![(0u16, 0u16, 0u16); 9];
    words[3].0 = WALL_BIT;
    let m = load(&synth(3, 3, &words, &[0u8; 9]));
    assert_eq!(m.attribute(1, 0), CellAttribute::HighWall);
    assert_eq!(m.attribute(0, 1), CellAttribute::Walk);

    // k=5 只在 wFrImg 置 bit15 ⇒ (1,2) 为 LowWall（顺带验证 wFrImg 就是 Low 触发字）。
    let mut words = vec![(0u16, 0u16, 0u16); 9];
    words[5].2 = WALL_BIT;
    let m = load(&synth(3, 3, &words, &[0u8; 9]));
    assert_eq!(m.attribute(1, 2), CellAttribute::LowWall);
    assert_eq!(m.low_count(), 1);
}

/// 定案⑤：门字节 9 种取值。
#[test]
fn door_byte_scan() {
    for (value, expect) in [
        (0x00u8, 0usize),
        (0x01, 0),
        (0x05, 0),
        (0x7F, 0),
        (0x80, 0), // bit7 置位但 point=0 ⇒ 不登记
        (0x81, 1),
        (0x85, 1),
        (0x87, 1),
        (0xFF, 1),
    ] {
        let words = vec![(0u16, 0u16, 0u16); 9];
        let mut doors = vec![0u8; 9];
        doors[0] = value;
        let m = load(&synth(3, 3, &words, &doors));
        assert_eq!(m.doors.len(), expect, "门字节 {value:#04X}");
        if expect == 1 {
            assert_eq!(m.doors[0].n08, value & 0x7F, "门字节 {value:#04X} 的 n08");
            assert_eq!((m.doors[0].x, m.doors[0].y), (0, 0));
            assert!(value & DOOR_BIT != 0);
        }
    }
}

/// 定案⑥：10 格内同 index ⇒ 共享同一 DoorStatus；否则各自新建。
#[test]
fn door_status_merge() {
    // 三门 0x81 相邻（k0/k1/k2）⇒ 3 条门，1 个 Status 实例。
    let words = vec![(0u16, 0u16, 0u16); 9];
    let mut doors = vec![0u8; 9];
    doors[0] = 0x81;
    doors[1] = 0x81;
    doors[2] = 0x81;
    let m = load(&synth(3, 3, &words, &doors));
    assert_eq!(m.doors.len(), 3, "DoorList 仍是各自独立条目");
    assert_eq!(m.door_status.len(), 1, "同组共用一个 Status");
    assert!(m.doors.iter().all(|d| d.status_id == 0));

    // 相邻但 n08 不同 ⇒ 不共享。
    let mut doors = vec![0u8; 9];
    doors[0] = 0x81;
    doors[1] = 0x82;
    let m = load(&synth(3, 3, &words, &doors));
    assert_eq!(m.doors.len(), 2);
    assert_eq!(m.door_status.len(), 2);
    assert_ne!(m.doors[0].status_id, m.doors[1].status_id);

    // |dy| == 10 ⇒ 合并；|dy| == 11 ⇒ 不合并（列主序 k = x*13 + y，x=0 时 k=y）。
    let words = vec![(0u16, 0u16, 0u16); 2 * 13];
    let mut at10 = vec![0u8; 2 * 13];
    at10[0] = 0x81;
    at10[10] = 0x81;
    let m = load(&synth(2, 13, &words, &at10));
    assert_eq!(m.door_status.len(), 1, "|dy|=10 应合并");

    let mut at11 = vec![0u8; 2 * 13];
    at11[0] = 0x81;
    at11[11] = 0x81;
    let m = load(&synth(2, 13, &words, &at11));
    assert_eq!(m.door_status.len(), 2, "|dy|=11 不应合并");
}

/// 定案⑧：`ObjList` 只在首个墙格之前分配；无墙则全部分配。
#[test]
fn obj_list_allocation_before_first_wall() {
    // 无墙 ⇒ 全部分配。
    let words = vec![(0u16, 0u16, 0u16); 4];
    let m = load(&synth(2, 2, &words, &[0u8; 4]));
    assert_eq!(m.first_wall_index, None);
    assert!(m.obj_list_allocated_at(0));
    assert!(m.obj_list_allocated_at(3));

    // 首个墙格在 k=1 ⇒ 只有 k0 分配。
    let mut words = vec![(0u16, 0u16, 0u16); 4];
    words[1].0 = WALL_BIT;
    let m = load(&synth(2, 2, &words, &[0u8; 4]));
    assert_eq!(m.first_wall_index, Some(1));
    assert!(m.obj_list_allocated_at(0));
    assert!(!m.obj_list_allocated_at(1));
    assert!(!m.obj_list_allocated_at(2));

    // pattern 首格即墙 ⇒ 一个都不分配。
    let words = [(0x8000u16, 0u16, 0u16), (0, 0, 0), (0, 0, 0), (0, 0, 0)];
    let m = load(&synth(2, 2, &words, &[0u8; 4]));
    assert_eq!(m.first_wall_index, Some(0));
    assert!(!m.obj_list_allocated_at(0));
}

/// 定案⑨ + `to_grid`：属性进 `MapGrid` 后，可走性（`Valid == Attribute == Walk`）必须一致。
///
/// 注意（与卡 §6/§9 同源）：`MapGrid` 的公开 `cell()` 走 `get_cell_info`，对**墙格会退化成
/// `cells[0]`**，所以墙格属性只能通过 `cell_valid`（直接读真实格）观测；High/Low 细分在
/// `MapFile::attribute` 里已由 `pattern_words_and_override` 锁住。
#[test]
fn to_grid_preserves_attributes() {
    let words = [
        (0x8000u16, 0u16, 0u16),
        (0, 0x8000, 0),
        (0, 0, 0x8000),
        (0x8000, 0, 0x8000),
    ];
    let m = load(&synth(2, 2, &words, &[0u8; 4]));
    let grid = m.to_grid();
    assert_eq!(grid.width, 2);
    assert_eq!(grid.height, 2);
    // Walk 格可走；墙格（High/Low 皆然）不可走。
    assert!(grid.cell_valid(0, 1));
    assert!(!grid.cell_valid(0, 0));
    assert!(!grid.cell_valid(1, 0));
    assert!(!grid.cell_valid(1, 1));
    // 与源地图逐格一致（含 High/Low 细分）。
    for x in 0..2 {
        for y in 0..2 {
            assert_eq!(
                grid.cell_valid(x, y),
                m.attribute(x, y) == CellAttribute::Walk
            );
        }
    }
}

/// 报错面：<= 52 字节、尺寸 <=1、截断，都必须 Err（对应 C# `result=false`）。
#[test]
fn rejects_malformed_maps() {
    assert!(parse_map_bytes(&[0u8; 52]).is_err(), "长度 == 52 应失败");
    assert!(parse_map_bytes(&[]).is_err());

    // 尺寸 1x1 ⇒ Initialize 不分配 CellArray ⇒ C# 抛 NRE 被吞 ⇒ result=false。
    let mut small = vec![0u8; HEADER_BYTES + CELL_BYTES];
    small[0..2].copy_from_slice(&1u16.to_le_bytes());
    small[2..4].copy_from_slice(&1u16.to_le_bytes());
    assert!(parse_map_bytes(&small).is_err());

    // 声明 3x3 但只有 2 格数据 ⇒ 截断。
    let mut short = vec![0u8; HEADER_BYTES + 2 * CELL_BYTES];
    short[0..2].copy_from_slice(&3u16.to_le_bytes());
    short[2..4].copy_from_slice(&3u16.to_le_bytes());
    assert!(parse_map_bytes(&short).is_err());
}

/// 点位表：`;` 注释、`,`/`\t` 分隔、越界丢弃、空格不是分隔符。
#[test]
fn points_table_parsing() {
    let text = concat!(
        "; 点位注释\r\n",
        "10,20\r\n",
        " 5 , 6 \n",
        "bad\n",
        "30\t40\n",
        "-1,5\n",
        "99,99\n",
        "12 34\n",
        "\n",
    );
    let pts = parse_points(text, 50, 50);
    assert_eq!(
        pts,
        vec![
            mir2_envir::PointInfo { x: 10, y: 20 },
            mir2_envir::PointInfo { x: 5, y: 6 },
            mir2_envir::PointInfo { x: 30, y: 40 },
        ]
    );
}

// ---------- 红检：证明上面这些判据对「写反 / 误用」是敏感的（会红） ----------

/// 红检（卡原文判据②）：翻转某格的 `wBkImg` bit15 ⇒ 该格可走性必须变。
#[test]
fn red_check_flip_bit15_changes_walkability() {
    // 3x3 全走格。
    let words = vec![(0u16, 0u16, 0u16); 9];
    let bytes = synth(3, 3, &words, &[0u8; 9]);
    let m = load(&bytes);
    assert_eq!(m.attribute(1, 2), CellAttribute::Walk);
    assert!(m.to_grid().cell_valid(1, 2));

    // 人工翻转 (1,2)（文件序 k = 1*3 + 2 = 5）的 wBkImg bit15（小端 u16 的高字节 bit7）。
    let mut flipped = bytes.clone();
    let o = HEADER_BYTES + 5 * CELL_BYTES;
    flipped[o + 1] |= 0x80;
    let m2 = load(&flipped);
    assert_eq!(
        m2.attribute(1, 2),
        CellAttribute::HighWall,
        "翻转后应变为墙"
    );
    assert!(!m2.to_grid().cell_valid(1, 2), "翻转后应不可走");
    // 其余格不受影响。
    assert_eq!(m2.walk_count(), 8);
}

/// 反事实实现 1：把覆盖顺序写反（先判 Low 再判 High ⇒ High 胜）。
fn attribute_high_wins(bytes: &[u8], x: i32, y: i32, height: i32) -> CellAttribute {
    let o = HEADER_BYTES + ((x * height + y) as usize) * CELL_BYTES;
    let bk = u16::from_le_bytes([bytes[o], bytes[o + 1]]);
    let fr = u16::from_le_bytes([bytes[o + 4], bytes[o + 5]]);
    if bk & WALL_BIT != 0 {
        CellAttribute::HighWall
    } else if fr & WALL_BIT != 0 {
        CellAttribute::LowWall
    } else {
        CellAttribute::Walk
    }
}

/// 反事实实现 2：把 `wMidImg` 当属性。
fn attribute_mid_as_wall(bytes: &[u8], x: i32, y: i32, height: i32) -> CellAttribute {
    let o = HEADER_BYTES + ((x * height + y) as usize) * CELL_BYTES;
    let mid = u16::from_le_bytes([bytes[o + 2], bytes[o + 3]]);
    if mid & WALL_BIT != 0 {
        CellAttribute::HighWall
    } else {
        CellAttribute::Walk
    }
}

#[test]
fn red_check_coverage_order_and_mid_are_detected() {
    let words = [
        (0x8000u16, 0u16, 0u16),  // k0 (0,0) 只有 bk
        (0, 0x8000, 0),           // k1 (0,1) 只有 mid
        (0x8000, 0, 0x8000),      // k2 (1,0) bk|fr
        (0x8000, 0x8000, 0x8000), // k3 (1,1) 全置位
    ];
    // 直接用 2x2。
    let bytes = synth(2, 2, &words, &[0u8; 4]);
    let m = load(&bytes);

    // (0,1) 只有 mid ⇒ 正确实现 Walk；「mid 当属性」会判成 HighWall。
    assert_eq!(m.attribute(0, 1), CellAttribute::Walk);
    assert_eq!(
        attribute_mid_as_wall(&bytes, 0, 1, 2),
        CellAttribute::HighWall
    );
    assert_ne!(m.attribute(0, 1), attribute_mid_as_wall(&bytes, 0, 1, 2));

    // (1,0) bk|fr ⇒ 正确实现 LowWall；「写反顺序」会判成 HighWall。
    assert_eq!(m.attribute(1, 0), CellAttribute::LowWall);
    assert_eq!(
        attribute_high_wins(&bytes, 1, 0, 2),
        CellAttribute::HighWall
    );
    assert_ne!(m.attribute(1, 0), attribute_high_wins(&bytes, 1, 0, 2));

    // (1,1) 全置位同理会红。
    assert_eq!(m.attribute(1, 1), CellAttribute::LowWall);
    assert_ne!(m.attribute(1, 1), attribute_high_wins(&bytes, 1, 1, 2));
}
