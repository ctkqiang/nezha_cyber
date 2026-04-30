//! 智能体配置 —— 从 YAML 文件解析 Agent 定义。
//!
//! 每个 Agent 包含名称、描述、系统提示、默认模型及可用工具列表。
//! 配置文件名默认为 `agents.yaml`，可通过命令行参数指定。

use serde::{Deserialize, Serialize};

use crate::api::types::Tool;

/// 单个智能体的完整配置
///
/// 示例 YAML：
/// ```yaml
/// agents:
///   - name: "红队渗透助手"
///     description: "辅助渗透测试与漏洞分析"
///     system_prompt: "你是一个专业的网络安全红队专家..."
///     model: "deepseek-chat"
///     tools:
///       - name: "run_nmap"
///         description: "执行 Nmap 扫描"
///         parameters:
///           type: "object"
///           properties:
///             target:
///               type: "string"
///               description: "扫描目标 IP 或域名"
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    pub name: String,
    pub description: String,
    pub system_prompt: String,
    pub model: String,
    #[serde(default)]
    pub tools: Vec<AgentToolConfig>,
}

/// 智能体配置中声明的工具
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentToolConfig {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

/// 所有智能体的配置包装
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentsConfig {
    pub agents: Vec<AgentConfig>,
}

/// 默认 DeepSeek 模型定价（CNY / 1M tokens）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefaultPricing {
    pub prompt_price_per_m: f64,
    pub completion_price_per_m: f64,
}

/// 应用全局配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default = "default_api_base")]
    pub api_base: String,
    #[serde(default)]
    pub api_key: String,
    #[serde(default)]
    pub default_model: String,
    #[serde(default)]
    pub default_pricing: Option<DefaultPricing>,
}

fn default_api_base() -> String {
    "https://api.deepseek.com/v1".into()
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            api_base: default_api_base(),
            api_key: String::new(),
            default_model: "deepseek-v4-pro".into(),
            default_pricing: Some(DefaultPricing {
                prompt_price_per_m: 2.0,
                completion_price_per_m: 8.0,
            }),
        }
    }
}

impl AgentConfig {
    /// 将 AgentConfig 中声明的工具转换为 API Tool 列表
    pub fn to_api_tools(&self) -> Vec<Tool> {
        self.tools
            .iter()
            .map(|t| Tool {
                tool_type: "function".to_string(),
                function: crate::api::types::FunctionDefinition {
                    name: t.name.clone(),
                    description: t.description.clone(),
                    parameters: t.parameters.clone(),
                },
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_tool_config(name: &str) -> AgentToolConfig {
        AgentToolConfig {
            name: name.into(),
            description: format!("{} 的描述", name),
            parameters: serde_json::json!({
                "type": "object",
                "properties": { "target": { "type": "string" } }
            }),
        }
    }

    #[test]
    fn agent_config_to_api_tools_empty() {
        let agent = AgentConfig {
            name: "测试Agent".into(),
            description: "测试".into(),
            system_prompt: "你是一个测试助手".into(),
            model: "deepseek-chat".into(),
            tools: vec![],
        };
        let api_tools = agent.to_api_tools();
        assert!(api_tools.is_empty());
    }

    #[test]
    fn agent_config_to_api_tools_single() {
        let agent = AgentConfig {
            name: "测试Agent".into(),
            description: "测试".into(),
            system_prompt: "你是一个测试助手".into(),
            model: "deepseek-chat".into(),
            tools: vec![make_tool_config("run_nmap")],
        };
        let api_tools = agent.to_api_tools();
        assert_eq!(api_tools.len(), 1);
        assert_eq!(api_tools[0].tool_type, "function");
        assert_eq!(api_tools[0].function.name, "run_nmap");
    }

    #[test]
    fn agent_config_to_api_tools_multiple() {
        let agent = AgentConfig {
            name: "测试Agent".into(),
            description: "测试".into(),
            system_prompt: "你是一个测试助手".into(),
            model: "deepseek-chat".into(),
            tools: vec![
                make_tool_config("tool_a"),
                make_tool_config("tool_b"),
                make_tool_config("tool_c"),
            ],
        };
        let api_tools = agent.to_api_tools();
        assert_eq!(api_tools.len(), 3);
        assert_eq!(api_tools[0].function.name, "tool_a");
        assert_eq!(api_tools[2].function.name, "tool_c");
    }

    #[test]
    fn app_config_default_values() {
        let config = AppConfig::default();
        assert_eq!(config.api_base, "https://api.deepseek.com/v1");
        assert_eq!(config.default_model, "deepseek-v4-pro");
        assert!(config.api_key.is_empty());
        let pricing = config.default_pricing.unwrap();
        assert_eq!(pricing.prompt_price_per_m, 2.0);
        assert_eq!(pricing.completion_price_per_m, 8.0);
    }

    #[test]
    fn app_config_default_api_base_matches_fn() {
        assert_eq!(default_api_base(), "https://api.deepseek.com/v1");
    }

    #[test]
    fn yaml_deserialize_agents_config() {
        let yaml = r#"
agents:
  - name: "测试Agent"
    description: "一个测试智能体"
    system_prompt: "你是一个测试助手"
    model: "deepseek-chat"
    tools:
      - name: "test_tool"
        description: "测试工具"
        parameters:
          type: "object"
          properties: {}
"#;
        let config: AgentsConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.agents.len(), 1);
        assert_eq!(config.agents[0].name, "测试Agent");
        assert_eq!(config.agents[0].tools.len(), 1);
        assert_eq!(config.agents[0].tools[0].name, "test_tool");
    }

    #[test]
    fn yaml_deserialize_no_tools_defaults_to_empty() {
        let yaml = r#"
agents:
  - name: "无工具Agent"
    description: "测试"
    system_prompt: "你是一个助手"
    model: "deepseek-chat"
"#;
        let config: AgentsConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(config.agents[0].tools.is_empty());
    }
}
