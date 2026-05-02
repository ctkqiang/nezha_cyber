//! 本地工具执行模块 —— 文件读写、项目创建、目录扫描、OCR 文本提取。
//!
//! Agent 通过 function calling 请求执行本地操作（写文件、建项目等），
//! 用户确认后由本模块实际执行并返回结果。
//!
//! OCR 子模块支持 PDF / 图片 / 文本文件的文字提取，
//! 通过 @文件路径 语法在输入框中引用文件。

pub mod executor;
pub mod ocr;
pub use executor::execute_tool;
pub use executor::tool_definitions;
pub use ocr::detect_at_cursor;
pub use ocr::extract_file_text;
pub use ocr::parse_at_references;
pub use ocr::scan_files_for_autocomplete;
