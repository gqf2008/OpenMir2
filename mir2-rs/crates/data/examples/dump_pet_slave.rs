//! 临时：用 `crates/data` 的 `PetSlaveConfig` 真读现网 `Server.conf`，核对部署值。
fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("usage: dump_pet_slave <Server.conf>");
    let c = mir2_data::pet_slave::PetSlaveConfig::load(std::path::Path::new(&path))
        .expect("read Server.conf");
    println!("{c:#?}");
}
