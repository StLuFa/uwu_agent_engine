//! 迁移模块：Migrator 主逻辑与 embedded_sql! 宏。
//!
//! # 快速开始
//!
//! ```rust,ignore
//! use uwu_database::migrate::{Migrator, SqlMigration};
//!
//! let migrator = Migrator::new()
//!     .add(SqlMigration::new(1, "create_users",
//!         "CREATE TABLE users (id SERIAL PRIMARY KEY, name TEXT NOT NULL)",
//!         Some("DROP TABLE users"),
//!     ));
//!
//! // 应用所有待迁移
//! migrator.up(&db.pool).await?;
//! ```


use std::collections::BTreeMap;
use std::path::Path;

use async_trait::async_trait;
use tracing::{info, warn};

use crate::error::Result;
use crate::sql::DbPool;

mod support;
use support::now_rfc3339;

// ── 公开宏 ─────────────────────────────────────────────

/// 便捷宏：从字面量创建 [`SqlMigration`]。
///
/// ```rust,ignore
/// let m = embedded_sql!(1, "init", "CREATE TABLE t (id INT);", None);
/// ```
#[macro_export]
macro_rules! embedded_sql {
    ($version:expr, $name:expr, $up:expr, $down:expr) => {
        $crate::migrate::SqlMigration::new($version, $name, $up, $down)
    };
    ($version:expr, $name:expr, $up:expr) => {
        $crate::migrate::SqlMigration::new(
            $version, $name, $up, ::core::option::Option::None
        )
    };
}

pub use embedded_sql;

// ── 版本表 DDL ───────────────────────────────────────

const DEFAULT_VERSION_TABLE: &str = "_uwu_schema_version";

fn create_version_table_sql(table: &str) -> String {
    format!(
        r#"CREATE TABLE IF NOT EXISTS "{table}" (
            version     BIGINT PRIMARY KEY,
            name        TEXT    NOT NULL,
            applied_at  TEXT    NOT NULL,
            checksum    TEXT
        )"#
    )
}

// ── Migration trait ─────────────────────────────────────

/// 单个迁移的定义。
#[async_trait]
pub trait Migration: Send + Sync + 'static {
    fn version(&self) -> i64;
    fn name(&self) -> &str;
    async fn up(&self, pool: &DbPool) -> Result<()>;
    fn down_sql(&self) -> Option<&str>;
}

// ── SqlMigration ───────────────────────────────────────

/// 基于 SQL 语句的迁移。
///
/// **注意—事务语义：** PostgreSQL 和 MySQL 的 DDL 支持事务回滚；
/// SQLite 的 DDL 在事务中执行时行为不同（某些语句会隐式提交）。
/// 如需跨语句原子性，请在 SQL 文件中显式书写 `BEGIN;` / `COMMIT;`。
pub struct SqlMigration {
    pub version: i64,
    pub name: String,
    pub up_sql: String,
    pub down_sql: Option<String>,
}

impl SqlMigration {
    pub fn new(
        version: i64,
        name: impl Into<String>,
        up_sql: impl Into<String>,
        down_sql: Option<impl Into<String>>,
    ) -> Self {
        Self {
            version,
            name: name.into(),
            up_sql: up_sql.into(),
            down_sql: down_sql.map(Into::into),
        }
    }

    pub fn from_files(
        version: i64,
        name: impl Into<String>,
        up_path: impl AsRef<Path>,
        down_path: Option<impl AsRef<Path>>,
    ) -> std::io::Result<Self> {
        let up_sql = std::fs::read_to_string(up_path)
            .map_err(|e| std::io::Error::new(e.kind(), format!("read up.sql: {e}")))?;
        let down_sql = match down_path {
            Some(p) => Some(
                std::fs::read_to_string(p)
                    .map_err(|e| std::io::Error::new(e.kind(), format!("read down.sql: {e}")))?,
            ),
            None => None,
        };
        Ok(Self::new(version, name, up_sql, down_sql))
    }
}

#[async_trait]
impl Migration for SqlMigration {
    fn version(&self) -> i64 { self.version }
    fn name(&self) -> &str { &self.name }
    async fn up(&self, pool: &DbPool) -> Result<()> {
        pool.exec(&self.up_sql).await
    }
    fn down_sql(&self) -> Option<&str> {
        self.down_sql.as_deref()
    }
}

// ── MigrationRecord ─────────────────────────────────────

#[derive(Debug, Clone)]
pub struct MigrationRecord {
    pub version: i64,
    pub name: String,
    pub applied_at: Option<String>,
    pub pending: bool,
}

// ── Migrator ───────────────────────────────────────────

/// 迁移管理器。
pub struct Migrator {
    migrations: BTreeMap<i64, Box<dyn Migration>>,
    version_table: String,
}

impl Migrator {
    pub fn new() -> Self {
        Self {
            migrations: BTreeMap::new(),
            version_table: DEFAULT_VERSION_TABLE.to_string(),
        }
    }

    pub fn with_version_table(mut self, table: impl Into<String>) -> Self {
        self.version_table = table.into();
        self
    }

    pub fn add(mut self, m: impl Migration + 'static) -> Self {
        let v = m.version();
        self.migrations.insert(v, Box::new(m));
        self
    }

    /// 从目录加载 SQL 文件迁移。
    ///
    /// 文件命名：`<version>_<name>.up.sql` / `<version>_<name>.down.sql`
    pub fn load_dir(path: impl AsRef<Path>) -> Result<Self> {
        let dir = path.as_ref();
        if !dir.is_dir() {
            return Err(crate::error::DbError::Migrate(format!(
                "migrations directory not found: {}", dir.display()
            )));
        }

        let mut migrator = Self::new();
        let mut seen: BTreeMap<i64, String> = BTreeMap::new();

        let entries = std::fs::read_dir(dir).map_err(|e| {
            crate::error::DbError::Migrate(format!("read migrations dir: {e}"))
        })?;
        for entry in entries {
            let entry = entry.map_err(|e| {
                crate::error::DbError::Migrate(format!("read dir entry: {e}"))
            })?;
            let fname = entry.file_name().to_string_lossy().to_string();
            if let Some((version, name)) = parse_filename(&fname) {
                seen.insert(version, name);
            }
        }

        for (version, name) in seen {
            let up_path = dir.join(format!("{version:04}_{name}.up.sql"));
            let down_path = dir.join(format!("{version:04}_{name}.down.sql"));
            let down_sql = if down_path.exists() {
                Some(std::fs::read_to_string(&down_path).map_err(|e| {
                    crate::error::DbError::Migrate(format!("read down.sql: {e}"))
                })?)
            } else {
                None
            };
            let up_sql = std::fs::read_to_string(&up_path).map_err(|e| {
                crate::error::DbError::Migrate(format!("read up.sql: {e}"))
            })?;
            migrator = migrator.add(SqlMigration {
                version,
                name,
                up_sql,
                down_sql,
            });
        }

        Ok(migrator)
    }

    async fn ensure_version_table(&self, pool: &DbPool) -> Result<()> {
        let sql = create_version_table_sql(&self.version_table);
        pool.exec(&sql).await?;
        Ok(())
    }

    async fn get_applied(&self, pool: &DbPool) -> Result<Vec<(i64, String, String)>> {
        pool.fetch_version_records(&self.version_table).await
    }

    // ── 公共 API ───────────────────────────────────────

    pub async fn status(&self, pool: &DbPool) -> Result<Vec<MigrationRecord>> {
        self.ensure_version_table(pool).await?;
        let applied = self.get_applied(pool).await?;
        let applied_set: std::collections::HashSet<i64> =
            applied.iter().map(|(v, _, _)| *v).collect();

        Ok(self.migrations.iter().map(|(v, m)| {
            let applied_info = applied.iter().find(|(av, _, _)| av == v);
            MigrationRecord {
                version: *v,
                name: m.name().to_string(),
                applied_at: applied_info.map(|(_, _, t)| t.clone()),
                pending: !applied_set.contains(v),
            }
        }).collect())
    }

    pub async fn up(&self, pool: &DbPool) -> Result<()> {
        self.up_to(pool, None).await
    }

    pub async fn up_to(&self, pool: &DbPool, target: Option<i64>) -> Result<()> {
        self.ensure_version_table(pool).await?;
        let applied = self.get_applied(pool).await?;
        let applied_set: std::collections::HashSet<i64> =
            applied.iter().map(|(v, _, _)| *v).collect();

        let target = target.unwrap_or(i64::MAX);

        for (v, m) in &self.migrations {
            if *v > target { break; }
            if applied_set.contains(v) { continue; }
            info!(version = v, name = m.name(), "applying migration");
            m.up(pool).await?;
            let now = now_rfc3339();
            pool.insert_version_record(&self.version_table, *v, m.name(), &now, "")
                .await?;
            info!(version = v, name = m.name(), "migration applied");
        }
        Ok(())
    }

    pub async fn down(&self, pool: &DbPool, target: i64) -> Result<()> {
        self.ensure_version_table(pool).await?;
        let applied = self.get_applied(pool).await?;

        let mut to_rollback: Vec<i64> = applied
            .iter().map(|(v, _, _)| *v)
            .filter(|v| *v > target)
            .collect();
        to_rollback.sort_unstable_by(|a, b| b.cmp(a));

        for v in to_rollback {
            let Some(m) = self.migrations.get(&v) else {
                warn!(version = v, "migration definition not found, skipping");
                continue;
            };
            let Some(down_sql) = m.down_sql() else {
                return Err(crate::error::DbError::Migrate(format!(
                    "migration {v} ({}) has no down script", m.name()
                )));
            };
            info!(version = v, name = m.name(), "rolling back migration");
            pool.exec(down_sql).await?;
            pool.delete_version_record(&self.version_table, v).await?;
            info!(version = v, name = m.name(), "migration rolled back");
        }
        Ok(())
    }
}

impl Default for Migrator {
    fn default() -> Self { Self::new() }
}

// ── 文件名解析 ─────────────────────────────────────────

fn parse_filename(fname: &str) -> Option<(i64, String)> {
    if let Some(stem) = fname.strip_suffix(".sql") {
        let (prefix, ext) = stem.rsplit_once('.')?;
        if ext != "up" { return None; }
        let mut parts = prefix.splitn(2, '_');
        let ver_str = parts.next()?;
        let name = parts.next()?;
        let version = ver_str.parse::<i64>().ok()?;
        Some((version, name.to_string()))
    } else {
        None
    }
}

// ── 测试 ───────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_filename_up() {
        assert_eq!(
            parse_filename("0001_init_users.up.sql"),
            Some((1, "init_users".to_string()))
        );
        assert_eq!(parse_filename("0010_add_email.down.sql"), None);
        assert_eq!(parse_filename("readme.md"), None);
    }
}
