use uwu_logger::{Logger};

fn main() {
    let mut logger = Logger::new();
    logger.println(true);
    logger.info("1234", "dfg", "服务启动成功，监听端口 8080");
    logger.warn("56", "lkkj", "配置文件未找到，使用默认配置");
    logger.success("fv", "7989", "数据库连接成功：duration=1.234s");
    logger.error("bn", "dfgh", "数据库连接失败：connection refused");
    logger.rollback("78", "ty", "事务已回滚：tx_id=abc123");
}
