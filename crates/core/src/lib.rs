pub mod comments;
pub mod config;
pub mod csv_data;
pub mod history;
pub mod merge;
pub mod merge_git;
pub mod project;
pub mod storage;

pub type Result<T> = std::result::Result<T, String>;

pub fn lf(value: &str) -> String {
    value.replace("\r\n", "\n").replace('\r', "\n")
}
