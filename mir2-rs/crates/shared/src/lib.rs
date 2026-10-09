//! OpenMir2 公共工具。
//!
//! - [`rng`]：复刻 `System.Random`（固定种子走 legacy 减法算法）与
//!   `src/OpenMir2/RandomNumber.cs` 的包装语义。
//! - [`hutil`]：`src/OpenMir2/HUtil32.cs` 中数值公式用到的等价物。

pub mod hutil;
pub mod rng;
