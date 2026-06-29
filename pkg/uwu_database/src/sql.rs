use crate::config::{DbConfig, SqlBackend};
use crate::error::{DbError, Result};
use sqlx::pool::PoolOptions;
use sqlx::Row;

/// 多后端 Pool 枚举。
#[derive(Clone, Debug)]
pub enum DbPool {
    #[cfg(feature = "postgres")]
    Postgres(sqlx::PgPool),
    #[cfg(feature = "mysql")]
    MySql(sqlx::MySqlPool),
    #[cfg(feature = "sqlite")]
    Sqlite(sqlx::SqlitePool),
}

impl DbPool {
    pub fn backend(&self) -> SqlBackend {
        match self {
            #[cfg(feature = "postgres")]
            DbPool::Postgres(_) => SqlBackend::Postgres,
            #[cfg(feature = "mysql")]
            DbPool::MySql(_) => SqlBackend::MySql,
            #[cfg(feature = "sqlite")]
            DbPool::Sqlite(_) => SqlBackend::Sqlite,
        }
    }

    pub async fn close(&self) {
        match self {
            #[cfg(feature = "postgres")]
            DbPool::Postgres(p) => p.close().await,
            #[cfg(feature = "mysql")]
            DbPool::MySql(p) => p.close().await,
            #[cfg(feature = "sqlite")]
            DbPool::Sqlite(p) => p.close().await,
        }
    }

    #[cfg(feature = "postgres")]
    pub fn as_postgres(&self) -> Result<&sqlx::PgPool> {
        #[allow(irrefutable_let_patterns, unreachable_patterns)]
        match self {
            DbPool::Postgres(p) => Ok(p),
            #[allow(unreachable_patterns)]
            _ => Err(DbError::Unsupported("expected postgres pool".into())),
        }
    }

    #[cfg(feature = "mysql")]
    pub fn as_mysql(&self) -> Result<&sqlx::MySqlPool> {
        #[allow(irrefutable_let_patterns, unreachable_patterns)]
        match self {
            DbPool::MySql(p) => Ok(p),
            #[allow(unreachable_patterns)]
            _ => Err(DbError::Unsupported("expected mysql pool".into())),
        }
    }

    #[cfg(feature = "sqlite")]
    pub fn as_sqlite(&self) -> Result<&sqlx::SqlitePool> {
        #[allow(irrefutable_let_patterns, unreachable_patterns)]
        match self {
            DbPool::Sqlite(p) => Ok(p),
            #[allow(unreachable_patterns)]
            _ => Err(DbError::Unsupported("expected sqlite pool".into())),
        }
    }

    /// 执行原始 SQL（DDL/DML），不返回结果。
    /// 适用于建表、ALTER、INSERT 等语句。
    /// 多条语句用分号分隔，会逐条执行。
    pub async fn exec(&self, sql: &str) -> Result<()> {
        for stmt in split_sql(sql) {
            match self {
                #[cfg(feature = "postgres")]
                DbPool::Postgres(p) => { sqlx::query(&stmt).execute(p).await?; }
                #[cfg(feature = "mysql")]
                DbPool::MySql(p)    => { sqlx::query(&stmt).execute(p).await?; }
                #[cfg(feature = "sqlite")]
                DbPool::Sqlite(p)   => { sqlx::query(&stmt).execute(p).await?; }
            }
        }
        Ok(())
    }

    /// 查询版本跟踪表，返回 (version, name, applied_at) 列表。
    pub async fn fetch_version_records(
        &self,
        table: &str,
    ) -> Result<Vec<(i64, String, String)>> {
        let sql = format!(
            "SELECT version, name, applied_at FROM \"{}\" ORDER BY version",
            table.replace('"', "\"\"")  // 简单防注入
        );
        match self {
            #[cfg(feature = "postgres")]
            DbPool::Postgres(p) => {
                let rows = sqlx::query(&sql).fetch_all(p).await?;
                Ok(rows.into_iter().map(|r| {
                    (r.get::<i64,_>("version"), r.get::<String,_>("name"), r.get::<String,_>("applied_at"))
                }).collect())
            }
            #[cfg(feature = "mysql")]
            DbPool::MySql(p) => {
                let rows = sqlx::query(&sql).fetch_all(p).await?;
                Ok(rows.into_iter().map(|r| {
                    (r.get::<i64,_>("version"), r.get::<String,_>("name"), r.get::<String,_>("applied_at"))
                }).collect())
            }
            #[cfg(feature = "sqlite")]
            DbPool::Sqlite(p) => {
                let rows = sqlx::query(&sql).fetch_all(p).await?;
                Ok(rows.into_iter().map(|r| {
                    (r.get::<i64,_>("version"), r.get::<String,_>("name"), r.get::<String,_>("applied_at"))
                }).collect())
            }
        }
    }

    /// 向版本跟踪表插入一条记录（使用字符串拼接，所有值均来自可信来源）。
    pub async fn insert_version_record(
        &self,
        table: &str,
        version: i64,
        name: &str,
        applied_at: &str,
        checksum: &str,
    ) -> Result<()> {
        // 所有值均来自代码内部，无用户注入风险
        let sql = format!(
            "INSERT INTO \"{}\" (version, name, applied_at, checksum) \
             VALUES ({}, '{}', '{}', '{}') \
             ON CONFLICT (version) DO UPDATE SET \
               name = EXCLUDED.name, \
               applied_at = EXCLUDED.applied_at, \
               checksum = EXCLUDED.checksum",
            table.replace('"', "\"\""),
            version,
            escape_sql_str(name),
            escape_sql_str(applied_at),
            escape_sql_str(checksum),
        );
        self.exec(&sql).await
    }

    /// 从版本跟踪表删除一条记录。
    pub async fn delete_version_record(
        &self,
        table: &str,
        version: i64,
    ) -> Result<()> {
        let sql = format!(
            "DELETE FROM \"{}\" WHERE version = {}",
            table.replace('"', "\"\""),
            version
        );
        self.exec(&sql).await
    }
}

// ── SQL 工具 ───────────────────────────────────────────────

/// 将 SQL 脚本按分号拆成多条语句（忽略字符串内的分号）。
fn split_sql(sql: &str) -> Vec<String> {
    let mut stmts = Vec::new();
    let mut cur = String::new();
    let mut in_string: Option<char> = None;
    let mut chars = sql.chars().peekable();
    while let Some(c) = chars.next() {
        cur.push(c);
        if in_string.is_some() {
            if c == '\\' {
                if let Some(next) = chars.next() { cur.push(next); }
            } else if c == in_string.unwrap() {
                in_string = None;
            }
            continue;
        }
        match c {
            '\'' | '"' => { in_string = Some(c); }
            ';' => {
                let s = cur.trim().to_string();
                if !s.is_empty() && !s.starts_with("--") && !s.starts_with('#') {
                    stmts.push(s);
                }
                cur.clear();
            }
            _ => {}
        }
    }
    let s = cur.trim();
    if !s.is_empty() && !s.starts_with("--") && !s.starts_with('#') {
        stmts.push(s.to_string());
    }
    stmts
}

/// 转义 SQL 字符串中的单引号。
fn escape_sql_str(s: &str) -> String {
    s.replace('\'', "''")
}

/// 按配置构建连接池，并应用性能相关默认值。
pub async fn build_pool(cfg: &DbConfig) -> Result<DbPool> {
    match cfg.backend {
        SqlBackend::Postgres => {
            #[cfg(feature = "postgres")]
            {
                use sqlx::postgres::PgConnectOptions;
                use std::str::FromStr;

                let mut opts = PgConnectOptions::from_str(&cfg.url)
                    .map_err(|e| DbError::Config(e.to_string()))?
                    .statement_cache_capacity(cfg.statement_cache_capacity);
                if let Some(name) = &cfg.application_name {
                    opts = opts.application_name(name);
                }
                let pool = PoolOptions::<sqlx::Postgres>::new()
                    .max_connections(cfg.max_connections)
                    .min_connections(cfg.min_connections)
                    .acquire_timeout(cfg.acquire_timeout())
                    .idle_timeout(Some(cfg.idle_timeout()))
                    .max_lifetime(Some(cfg.max_lifetime()))
                    .test_before_acquire(cfg.test_before_acquire)
                    .connect_with(opts)
                    .await?;
                Ok(DbPool::Postgres(pool))
            }
            #[cfg(not(feature = "postgres"))]
            { Err(DbError::Unsupported("postgres feature disabled".into())) }
        }
        SqlBackend::MySql => {
            #[cfg(feature = "mysql")]
            {
                use sqlx::mysql::MySqlConnectOptions;
                use std::str::FromStr;

                let opts = MySqlConnectOptions::from_str(&cfg.url)
                    .map_err(|e| DbError::Config(e.to_string()))?
                    .statement_cache_capacity(cfg.statement_cache_capacity);
                let pool = PoolOptions::<sqlx::MySql>::new()
                    .max_connections(cfg.max_connections)
                    .min_connections(cfg.min_connections)
                    .acquire_timeout(cfg.acquire_timeout())
                    .idle_timeout(Some(cfg.idle_timeout()))
                    .max_lifetime(Some(cfg.max_lifetime()))
                    .test_before_acquire(cfg.test_before_acquire)
                    .connect_with(opts)
                    .await?;
                Ok(DbPool::MySql(pool))
            }
            #[cfg(not(feature = "mysql"))]
            { Err(DbError::Unsupported("mysql feature disabled".into())) }
        }
        SqlBackend::Sqlite => {
            #[cfg(feature = "sqlite")]
            {
                use sqlx::sqlite::SqliteConnectOptions;
                use std::str::FromStr;

                let opts = SqliteConnectOptions::from_str(&cfg.url)
                    .map_err(|e| DbError::Config(e.to_string()))?
                    .statement_cache_capacity(cfg.statement_cache_capacity);
                let pool = PoolOptions::<sqlx::Sqlite>::new()
                    .max_connections(cfg.max_connections)
                    .min_connections(cfg.min_connections)
                    .acquire_timeout(cfg.acquire_timeout())
                    .idle_timeout(Some(cfg.idle_timeout()))
                    .max_lifetime(Some(cfg.max_lifetime()))
                    .test_before_acquire(cfg.test_before_acquire)
                    .connect_with(opts)
                    .await?;
                Ok(DbPool::Sqlite(pool))
            }
            #[cfg(not(feature = "sqlite"))]
            { Err(DbError::Unsupported("sqlite feature disabled".into())) }
        }
    }
}
