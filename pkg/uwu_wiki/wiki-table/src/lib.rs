//! # wiki-table
//!
//! 智能表格骨架。领域实体（行）自适配为 `wiki_llm::TextUnit` 后调用 wiki-llm 端口，
//! 不反向依赖横切层的领域类型。

use serde::{Deserialize, Serialize};
use wiki_llm::TextUnit;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ColumnId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RowId(pub String);

/// 列类型。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ColumnType {
    Text,
    Number,
    Checkbox,
    Url,
    Date,
    Select,
    MultiSelect,
    Relation,
    Rollup,
    Formula,
    LlmFill,
    CreatedAt,
    UpdatedAt,
    CreatedBy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Column {
    pub id: ColumnId,
    pub name: String,
    pub ty: ColumnType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cell {
    pub column: ColumnId,
    pub value: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Row {
    pub id: RowId,
    pub cells: Vec<Cell>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Table {
    pub id: String,
    pub name: String,
    pub columns: Vec<Column>,
    pub rows: Vec<Row>,
}

impl Table {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            columns: Vec::new(),
            rows: Vec::new(),
        }
    }

    /// 把一行适配为领域无关的 TextUnit（供 wiki-llm 处理）。
    pub fn row_to_text_unit(&self, row: &Row) -> TextUnit {
        let text = row
            .cells
            .iter()
            .map(|c| c.value.to_string())
            .collect::<Vec<_>>()
            .join(" | ");
        TextUnit {
            id: row.id.0.clone(),
            text,
            path: vec![self.id.clone(), row.id.0.clone()],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn row_adapts_to_text_unit() {
        let mut t = Table::new("t1", "tasks");
        t.columns.push(Column {
            id: ColumnId("c1".into()),
            name: "title".into(),
            ty: ColumnType::Text,
        });
        let row = Row {
            id: RowId("r1".into()),
            cells: vec![Cell {
                column: ColumnId("c1".into()),
                value: serde_json::json!("hello"),
            }],
        };
        let unit = t.row_to_text_unit(&row);
        assert_eq!(unit.id, "r1");
        assert_eq!(unit.path, vec!["t1".to_string(), "r1".to_string()]);
    }
}
