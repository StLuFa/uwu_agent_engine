//! SkillTarget + SkillVersion

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Skill 部署目标
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SkillTarget {
    /// 本地代码（写入 crate）
    LocalCode {
        crate_name: String,
    },
    /// 远程 MCP 工具
    McpRemote {
        server_id: String,
        tool_name: String,
        endpoint: String,
    },
    /// 本地偏好更新
    LocalPreference,
}

/// Skill 版本 —— 自进化防护基础
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillVersion {
    /// 版本 ID（hash）
    pub version_id: String,
    /// Skill 名称
    pub skill_name: String,
    /// 部署目标
    pub target: SkillTarget,
    /// 内容哈希（SHA256）
    pub hash: String,
    /// 是否经过沙箱验证
    pub verified: bool,
    /// 是否为当前活跃版本
    pub active: bool,
    /// 创建时间
    pub created_at: DateTime<Utc>,
    /// 关联的 Episode ID
    pub episode_id: String,
    /// 置信度
    pub confidence: f32,
}

impl SkillVersion {
    pub fn new(
        skill_name: impl Into<String>,
        target: SkillTarget,
        content: impl Into<String>,
        episode_id: impl Into<String>,
        confidence: f32,
    ) -> Self {
        let content = content.into();
        // Simple hash: use string length + first chars as pseudo-hash
        let hash = format!("{:x}", md5_like(&content));

        Self {
            version_id: uuid::Uuid::new_v4().to_string(),
            skill_name: skill_name.into(),
            target,
            hash,
            verified: false,
            active: true,
            created_at: Utc::now(),
            episode_id: episode_id.into(),
            confidence: confidence.clamp(0.0, 1.0),
        }
    }

    /// 沙箱验证通过
    pub fn verify(&mut self) {
        self.verified = true;
    }

    /// 回滚此版本
    pub fn deactivate(&mut self) {
        self.active = false;
    }
}

/// 简单的伪哈希（开发用，生产用 SHA256）
fn md5_like(s: &str) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skill_version_lifecycle() {
        let mut sv = SkillVersion::new(
            "click-popup",
            SkillTarget::LocalCode {
                crate_name: "agent-reaction".into(),
            },
            "fn handle_popup() { /* ... */ }",
            "ep-1",
            0.85,
        );
        assert!(!sv.verified);
        sv.verify();
        assert!(sv.verified);
        sv.deactivate();
        assert!(!sv.active);
    }
}
