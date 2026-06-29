use std::any::Any;

pub mod println_logger;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum LoggerType {
    INFO,
    WARN,
    SUCCESS,
    ERROR,
    ROLLBACK,
}

pub fn color_str(str: &str, r: i64, g: i64, b: i64) -> String {
    let date = chrono::Local::now();
    return format!("\x1b[38;2;{};{};{}m{}\x1b[0m", r, g, b, str);
}

pub fn build_type_color_str(logger_type: LoggerType, str: &str) -> String {
    match logger_type {
        LoggerType::INFO => color_str(&str, 180, 180, 180),
        LoggerType::WARN => color_str(&str, 255, 255, 0),
        LoggerType::SUCCESS => color_str(&str, 0, 255, 0),
        LoggerType::ERROR => color_str(&str, 255, 0, 0),
        LoggerType::ROLLBACK => color_str(&str, 255, 0, 125),
        // blue
        // _ => color_str(&str, 0, 200, 255),
    }
}

pub trait LoggerModelBase {
    fn log(info: LoggerInfo);
    fn save(&self);
    fn load(&self);
}

pub struct LoggerInfo {
    id: String,
    tag: String,
    message: String,
    time: chrono::DateTime<chrono::Local>,
    logger_type: LoggerType,
}

impl LoggerInfo {
    pub fn new(key: String, tag: String, message: String, logger_type: LoggerType) -> Self {
        let new_date = chrono::Local::now();
        return LoggerInfo {
            id: key,
            tag,
            message,
            time: new_date,
            logger_type,
        };
    }
    pub fn get_message(&self) -> String {
        build_type_color_str(self.logger_type, self.message.as_str())
    }
    pub fn get_time_str(&self) -> String {
        self.time.format("%Y/%m/%d %H:%M:%S%.f").to_string()
    }

    pub fn get_type_str(&self) -> String {
        match self.logger_type {
            LoggerType::INFO => build_type_color_str(LoggerType::INFO, "INFO"),
            LoggerType::WARN => build_type_color_str(LoggerType::WARN, "WARN"),
            LoggerType::SUCCESS => build_type_color_str(LoggerType::SUCCESS, "SUCCESS"),
            LoggerType::ERROR => build_type_color_str(LoggerType::ERROR, "ERROR"),
            LoggerType::ROLLBACK => build_type_color_str(LoggerType::ROLLBACK, "ROLLBACK"),
        }
    }
}
