//! 本地工具执行模块 —— 文件读写、项目创建、目录扫描、OCR 文本提取、红队工具集。
//!
//! Agent 通过 function calling 请求执行本地操作（写文件、建项目等），
//! 用户确认后由本模块实际执行并返回结果。
//!
//! 红队工具集（redteam_tools）：
//! 涵盖信息侦察、Web渗透、密码攻击、后渗透、社工工程、逆向分析等完整攻击链。

pub mod executor;
pub mod ocr;
pub mod redteam_tools;
pub use executor::execute_tool;
pub use executor::tool_definitions;
pub use ocr::detect_at_cursor;
pub use ocr::extract_file_text;
pub use ocr::parse_at_references;
pub use ocr::scan_files_for_autocomplete;
pub use redteam_tools::redteam_tool_definitions;
