
//! 本地工具执行模块 —— 文件读写、项目创建、目录扫描。
//!
//! Agent 通过 function calling 请求执行本地操作（写文件、建项目等），
//! 用户确认后由本模块实际执行并返回结果。

pub mod executor;
pub use executor::execute_tool;
pub use executor::tool_definitions;
