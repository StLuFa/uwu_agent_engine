use crate::LoggerType;
use super::{color_str, LoggerModelBase, LoggerInfo};

pub struct PrintlnLogger;
impl LoggerModelBase for PrintlnLogger {
    fn log(info: LoggerInfo) {
        println!("{}", format!(
            "[{}] {} 「{}」【{}】 {} \n",
            info.get_time_str(),
            info.id,
            info.tag,
            info.get_type_str(),
            info.get_message()
        ));
    }

    fn save(&self) {
        todo!()
    }

    fn load(&self) {
        todo!()
    }
}
