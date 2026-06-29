mod log;

pub use log::{LoggerModelBase, LoggerType};
pub use log::println_logger::PrintlnLogger;
pub use log::LoggerInfo;

pub struct Logger {
    use_println: bool,
}

impl Logger {
    pub fn new() -> Logger {
        Logger {
            use_println: false,
        }
    }

    pub fn info(&self, id: &str, tag: &str, msg: &str) {
        let info = Logger::build_info(LoggerType::INFO, id.to_string(), tag.to_string(), msg.to_string());
        self.log(info);
    }
    pub fn warn(&self, id: &str, tag: &str, msg: &str) {
        let info = Logger::build_info(LoggerType::WARN, id.to_string(), tag.to_string(), msg.to_string());
        self.log(info);
    }
    pub fn success(&self, id: &str, tag: &str, msg: &str) {
        let info = Logger::build_info(LoggerType::SUCCESS, id.to_string(), tag.to_string(), msg.to_string());
        self.log(info);
    }
    pub fn error(&self, id: &str, tag: &str, msg: &str) {
        let info = Logger::build_info(LoggerType::ERROR, id.to_string(), tag.to_string(), msg.to_string());
        self.log(info);
    }
    pub fn rollback(&self, id: &str, tag: &str, msg: &str) {
        let info = Logger::build_info(LoggerType::ROLLBACK, id.to_string(), tag.to_string(), msg.to_string());
        self.log(info);
    }

    pub fn println(&mut self, use_println: bool) {
        self.use_println = use_println;
    }
    fn log(&self, info: LoggerInfo) {
        if self.use_println {
            PrintlnLogger::log(info)
        }
    }
    fn build_info(logger_type: LoggerType, id: String, tag: String, msg: String) -> LoggerInfo {
        LoggerInfo::new(id, tag, msg, logger_type)
    }
}

