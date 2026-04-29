# 哪吒网络安全 (Nezha Cyber)

**基于 DeepSeek 大模型的终端 UI 红队辅助工具**

---

## 目录

1. [项目概述](#项目概述)
2. [核心特性](#核心特性)
3. [技术架构](#技术架构)
4. [依赖栈](#依赖栈)
5. [安装指南](#安装指南)
6. [配置说明](#配置说明)
7. [使用指南](#使用指南)
8. [智能体系统](#智能体系统)
9. [DeepSeek API 集成](#deepseek-api-集成)
10. [快捷键速查](#快捷键速查)
11. [项目结构](#项目结构)
12. [开发指南](#开发指南)
13. [故障排除](#故障排除)
14. [路线图](#路线图)
15. [贡献指南](#贡献指南)
16. [许可证](#许可证)
17. [联系方式](#联系方式)

---

## 项目概述

哪吒网络安全是一款高性能终端 UI (TUI) 应用，专为从事红队演练、渗透测试和漏洞研究的网络安全专业人员打造。本应用作为智能指挥控制界面，利用 DeepSeek 大语言模型提供实时流式对话、基于函数调用 (Function Calling) 的自动化工具编排，以及多智能体会话管理——这一切均在轻量级终端环境中完成。

项目 UI/UX 参考了 Docker AI Agent (Gordon)，但完全聚焦于攻击安全场景。它提供可折叠侧边栏的分屏界面、多标签页会话管理、可搜索命令面板，以及带费用估算的实时 Token 用量追踪。

**核心设计哲学**：采用 Elm Architecture (Model-Update-View) 模式，纯函数状态转换，通过 `tokio::sync::mpsc` 通道传递事件，遵循零成本抽象的 Rust 惯用模式。

**作者**：钟智强
**仓库地址**：https://gitcode.com/ctkqiang_sr/nezha_cyber
**语言**：Rust (2021 edition)
**许可证**：MIT

---

## 核心特性

### 实时流式对话

- 基于 SSE (Server-Sent Events) 的 DeepSeek Chat Completions API 流式通信
- 逐 Token 渲染，新消息自动滚动到底部
- 完整支持流式 `tool_calls` 增量累积与内联卡片渲染

### 多智能体会话管理

- 多个并发会话标签页，各自独立智能体上下文
- 三个内置专业智能体（红队渗透助手、代码审计专家、威胁情报分析师）
- 基于 YAML 的智能体配置，支持热重载
- 按标签页切换模型与智能体，无需重启

### 函数调用（工具使用）

- 与 OpenAI 兼容的函数调用协议
- 工具调用生命周期可视化：等待 -> 运行中 -> 成功 / 失败
- 可展开的工具调用卡片，显示参数详情
- 高风险操作执行前用户确认对话框

### Token 用量与计费追踪

- 每个会话标签页实时 Token 计数
- 接入 DeepSeek 定价模型（输入 Token + 输出 Token 分别计费）
- 累计费用计算，显示在侧边栏
- 通过 YAML 配置自定义定价

### 终端 UI 特性

- 分屏布局：可折叠侧边栏（25% 宽度）+ 主聊天区域
- 命令面板 (Ctrl+K) 支持所有操作的模糊搜索
- 两套内置配色主题（赛博暗色 + 日光亮色）
- 斜杠命令支持快速操作
- 多行输入缓冲区

### 安全优先设计

- API 密钥从环境变量或 `.env` 文件读取
- `.env` 已加入 `.gitignore`，不入版本控制
- 代码库中无任何硬编码凭据
- 退出时优雅恢复终端状态（raw mode + alternate screen 清理）

---

## 技术架构

### Elm Architecture (Model-Update-View)

```
用户按键 / API 事件 / Tick 定时器
            │
            ▼
   Action (mpsc 通道)
            │
            ▼
   update() ────────► 修改 App 状态
       │                      │
       ▼                      ▼
   发起异步任务           触发重绘
   (DeepSeek API 调用)   (ratatui Frame 绘制)
```

**Model 层** (`app.rs`)：`App` 结构体是整个应用的唯一状态源，持有标签页会话、智能体配置、UI 焦点状态、主题偏好以及 DeepSeek API 客户端。所有状态变更仅通过 `update()` 函数进行。

**Update 层** (`app.rs::update()`)：纯函数，对 `Action` 枚举进行模式匹配，返回修改后的 `App` 状态。处理 20 余种 Action 变体，涵盖用户输入、流式 API 响应、工具调用和系统事件。

**View 层** (`ui/render.rs`)：七个纯渲染函数，接收 `App` 的引用，通过 `ratatui` 绘制终端界面。渲染过程中不修改任何状态。

### 事件通信

所有事件通过单一的 `tokio::sync::mpsc::unbounded_channel<Action>` 传递，包括：

- 用户按键 (crossterm `KeyEvent`)
- 周期性 `Tick` 事件（100ms 间隔自动重绘）
- 终端尺寸变更 `Resize` 事件
- DeepSeek API 异步响应（从后台 tokio 任务流式回传）

### API 调用数据流

1. 用户在输入栏键入文本后按 `Enter`
2. `process_key()` 发出 `Action::SubmitMessage`
3. `update()` 将用户消息追加到会话历史
4. `DeepSeekClient::stream_chat()` 作为后台 tokio 任务启动
5. SSE 流事件以 `StreamStart`、`StreamChunk`、`StreamDone` 或 `StreamError` 形式推回主循环
6. `update()` 增量构建助手消息内容
7. 每次状态变更时 `render()` 重绘界面

---

## 依赖栈

| 依赖库                                | 版本        | 用途                                 |
| ------------------------------------- | ----------- | ------------------------------------ |
| `ratatui`                             | 0.29        | 终端 UI 框架（crossterm 后端）       |
| `crossterm`                           | 0.28        | 终端控制（raw mode、事件、样式）     |
| `tokio`                               | 1           | 异步运行时，支持 HTTP 请求与事件循环 |
| `reqwest`                             | 0.12        | HTTP 客户端，支持 SSE 流             |
| `futures`                             | 0.3         | SSE 流解析的 Stream 组合子           |
| `serde` + `serde_json` + `serde_yaml` | 1 / 1 / 0.9 | 序列化框架                           |
| `uuid`                                | 1           | 消息与会话唯一标识符                 |
| `thiserror`                           | 2           | 错误类型派生                         |
| `chrono`                              | 0.4         | 时间戳格式化                         |
| `unicode-width`                       | 0.2         | 中日韩字符宽度计算                   |
| `anyhow`                              | 1           | 主函数灵活错误处理                   |
| `dotenvy`                             | 0.15        | 加载 `.env` 文件                     |

总计 13 个直接依赖，通过 `cargo build --release` 编译为单一静态二进制文件。

---

## 安装指南

### 前置条件

- Rust 工具链（stable，2021 edition 或更高版本）
  - 通过 [rustup](https://rustup.rs) 安装：`curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
- Git（用于克隆仓库）
- DeepSeek API Token（从 https://platform.deepseek.com 获取）

### 从源码构建

```bash
git clone https://gitcode.com/ctkqiang_sr/nezha_cyber.git
cd nezha_cyber

# 创建 .env 文件并填入你的 API Token
echo 'DEEPSEEK_TOKEN=sk-你的token' > .env

cargo build --release
```

编译产物位于 `target/release/nezha_cyber`。

### 验证安装

```bash
./target/release/nezha_cyber
```

应用将启动并显示终端 UI。按 `Ctrl+C` 退出。

---

## 配置说明

### 环境变量

在项目根目录创建 `.env` 文件：

```env
DEEPSEEK_TOKEN=sk-你的-deepseek-api-token
```

也可以在启动前设置环境变量：

```bash
export DEEPSEEK_TOKEN=sk-你的-deepseek-api-token
./target/release/nezha_cyber
```

应用同时支持旧版 `DEEPSEEK_API_KEY` 环境变量作为回退选项。

### YAML 配置文件

**`config.yaml`** —— 应用级配置：

```yaml
api_base: 'https://api.deepseek.com/v1'
api_key: 'sk-...' # 生产环境建议使用环境变量
default_model: 'deepseek-chat'
default_pricing:
  prompt_price_per_m: 2.0 # 每百万输入 Token 价格（元）
  completion_price_per_m: 8.0 # 每百万输出 Token 价格（元）
```

**`agents.yaml`** —— 智能体定义：

```yaml
agents:
  - name: '红队渗透助手'
    description: '辅助渗透测试与漏洞利用'
    system_prompt: '你是一个专业的网络安全红队专家...'
    model: 'deepseek-chat'
    tools:
      - name: 'run_nmap'
        description: '对目标执行 Nmap 扫描'
        parameters:
          type: 'object'
          properties:
            target:
              type: 'string'
              description: '目标 IP 地址或域名'
            ports:
              type: 'string'
              description: '端口范围，如 1-1000'
          required: ['target']
```

以上两个文件均为可选。缺失时应用将使用合理默认值，并加载三个内置智能体。

---

## 使用指南

### 启动应用

```bash
./target/release/nezha_cyber
```

终端将进入 alternate screen 模式并显示哪吒网络安全界面。

### 基本交互流程

1. 在屏幕底部的输入区键入消息。
2. 按 `Enter` 提交消息。
3. 助手回复逐 Token 实时流式显示。
4. 左侧边栏可查看 Token 用量和费用。
5. 通过顶部标签栏切换不同会话标签页。

### 多标签页会话

- 按 `Ctrl+N` 创建新标签页
- 每个标签页维护独立的对话历史
- 不同标签页可分配不同智能体
- 模型选择按标签页独立配置

### 斜杠命令（通过命令面板）

按 `Ctrl+K` 打开命令面板，然后输入以下命令：

| 命令            | 功能                                    |
| --------------- | --------------------------------------- |
| `/compact`      | 压缩对话上下文                          |
| `/fork`         | 复制当前会话                            |
| `/model <名称>` | 切换当前模型                            |
| `/theme <名称>` | 切换 "default"（赛博暗色）或 "daylight" |
| `/export`       | 导出对话到文件                          |
| `/new`          | 新建标签页                              |
| `/close`        | 关闭当前标签页                          |

---

## 智能体系统

哪吒网络安全内置三个预配置的专业智能体，各针对不同的网络安全工作场景设计。每个智能体都有定制的系统提示词，可扩展自定义工具。

### 红队渗透助手

**聚焦领域**：渗透测试、漏洞利用、社会工程学

**系统提示词**："你是一个专业的网络安全红队专家，精通渗透测试、漏洞挖掘、社会工程学攻击和防御。你熟悉 Kali Linux、Metasploit、Nmap、Burp Suite 等工具的使用。你的回答应当专业、准确、可操作。"

### 代码审计专家

**聚焦领域**：源代码安全审查、漏洞分析、安全编码规范

**系统提示词**："你是一个资深的代码安全审计专家，擅长发现代码中的安全漏洞、逻辑缺陷和不合规的编码实践。你精通多种编程语言的安全最佳实践。"

### 威胁情报分析师

**聚焦领域**：威胁情报收集、APT 分析、恶意软件行为分析、IOC 提取

**系统提示词**："你是一个经验丰富的威胁情报分析师，擅长分析 APT 攻击、恶意软件行为、攻击链追踪和 IOC 提取。你能够解读 MITRE ATT&CK 框架中的战术和技术。"

### 自定义智能体

通过在项目根目录创建 `agents.yaml` 文件来定义自定义智能体。`tools` 字段支持完整的 OpenAI/DeepSeek 函数调用规范，可集成自定义渗透测试工具。

---

## DeepSeek API 集成

### 流式协议

应用通过 SSE（Server-Sent Events）流式协议与 `https://api.deepseek.com/v1/chat/completions` 通信。API 返回的每个数据块解析流程如下：

```
data: {"id":"...","choices":[{"delta":{"content":"你好"}}]}
data: {"id":"...","choices":[{"delta":{"content":"，世界"}}]}
data: [DONE]
```

`DeepSeekClient` 处理以下环节：

- 带 Bearer Token 认证的 HTTP 请求构造
- SSE 逐行解析，检测 `[DONE]` 终止标记
- 内容增量累积到消息缓冲区
- 工具调用增量累积与组装（函数调用场景）
- HTTP 状态码、流断开、JSON 反序列化失败等异常处理

### 函数调用（工具使用）

工具定义遵循 OpenAI 兼容规范。SSE 流以多个增量片段的形式传递工具调用数据，由客户端累积并组装为完整的 `ToolCall` 结构：

1. 首个增量块：工具调用 `id` + 函数 `name`
2. 后续增量块：`arguments` JSON 片段（分部传输）
3. 最终组装：完整的 `ToolFunction`，含解析后的 `serde_json::Value`

TUI 将未完成的工具调用渲染为"等待中"状态，工具执行完成后切换为"成功"或"失败"。

### Token 定价

默认定价采用 DeepSeek 官方标准费率：

| Token 类型         | 价格（元 / 百万 Token） |
| ------------------ | ----------------------- |
| 输入（Prompt）     | 2.00                    |
| 输出（Completion） | 8.00                    |

定价可通过 `config.yaml` 自定义：

```yaml
default_pricing:
  prompt_price_per_m: 2.0
  completion_price_per_m: 8.0
```

---

## 快捷键速查

### 全局快捷键

| 快捷键   | 功能                 |
| -------- | -------------------- |
| `Ctrl+C` | 退出应用             |
| `Ctrl+N` | 新建会话标签页       |
| `Ctrl+K` | 切换命令面板         |
| `Ctrl+B` | 折叠/展开侧边栏      |
| `Tab`    | 在 UI 区域间切换焦点 |
| `Esc`    | 关闭命令面板或对话框 |

### 输入区快捷键

| 快捷键      | 功能                 |
| ----------- | -------------------- |
| `Enter`     | 发送消息             |
| `Backspace` | 删除最后一个字符     |
| `Ctrl+J`    | 插入换行（多行输入） |

---

## 项目结构

```
nezha_cyber/
|-- Cargo.toml                    # Rust 项目清单（13 个依赖）
|-- Cargo.lock                    # 依赖版本锁定文件
|-- .gitignore                    # 排除 /target、.trae、.env
|-- .env                          # 环境变量（不入版本控制）
|-- config.yaml                   # 应用配置（可选）
|-- agents.yaml                   # 智能体定义（可选）
|-- src/
|   |-- main.rs                   # 入口 + 事件循环 + 键盘处理
|   |-- action.rs                 # Action 枚举（20+ 事件变体）
|   |-- app.rs                    # App 全局状态 + update() 纯函数
|   |-- api/
|   |   |-- mod.rs                # API 模块声明
|   |   |-- types.rs              # 30+ 数据结构（Message、Usage、StreamResponse 等）
|   |   |-- deepseek.rs           # DeepSeek SSE 流式客户端
|   |-- agent/
|   |   |-- mod.rs                # Agent 模块声明
|   |   |-- config.rs             # YAML 智能体配置解析 + AppConfig
|   |-- ui/
|       |-- mod.rs                # UI 模块声明
|       |-- theme.rs              # 2 套内置配色主题（赛博暗色、日光亮色）
|       |-- layout.rs             # 纯函数布局计算
|       |-- render.rs             # 7 个渲染函数（侧边栏、消息、输入等）
|       |-- widgets/              # 自定义 UI 组件（预留目录）
|-- target/                       # 构建产物（gitignore）
```

模块依赖拓扑图：

```
action.rs  <--  api/types.rs  <--  api/deepseek.rs  <--  agent/config.rs
                                                           |
                                                           v
                                                        app.rs  <--  ui/render.rs
                                                           ^            ^
                                                           |            |
                                                      ui/layout.rs  ui/theme.rs
                                                           |
                                                           v
                                                        main.rs
```

---

## 开发指南

### 搭建开发环境

```bash
git clone https://gitcode.com/ctkqiang_sr/nezha_cyber.git
cd nezha_cyber
# 编辑 .env 填入你的 DeepSeek API Token
cargo build
cargo run
```

### 代码风格与规范

本项目强制执行严格的代码规范：

- **模块级文档**：每个 `.rs` 文件以 `//!` 模块文档注释开头，描述模块职责。
- **项目级文档**：每个 `pub fn`、`pub struct`、`pub enum` 上方均有 `///` 文档注释，描述参数、返回值、错误处理方式及使用示例。所有注释使用中文。
- **函数内部零注释**：函数体内不得有任何注释。代码逻辑通过清晰的变量命名和结构来表达。
- **现代 C++ 风格的 Rust**：清晰的模块化、RAII 模式、所有权明确、无过度抽象、优先使用标准库。
- **数据结构规范**：`Vec` 用于序列、`HashMap`/`BTreeMap` 用于键值映射、`BinaryHeap` 用于优先级队列。禁止使用 `LinkedList`。
- **显式错误处理**：除测试和不可恢复的初始化阶段外，不得使用 `.unwrap()`。所有 I/O 错误必须显式处理。
- **Rust 惯用实现**：`From`/`Into` 转换、`Display`/`Debug` 实现、`thiserror` 错误类型。

### 添加新智能体

1. 创建或编辑 `agents.yaml`：

```yaml
agents:
  - name: '你的智能体名称'
    description: '智能体功能描述'
    system_prompt: '给 LLM 的详细系统指令'
    model: 'deepseek-chat'
    tools: []
```

2. 重启应用。新智能体将出现在侧边栏并可选。

### 添加自定义工具

在智能体的 `tools` 字段下添加工具定义：

```yaml
tools:
  - name: '你的工具名称'
    description: '工具功能描述'
    parameters:
      type: 'object'
      properties:
        param1:
          type: 'string'
          description: '参数1的描述'
      required: ['param1']
```

### 运行测试

```bash
cargo test
```

### 生产构建

```bash
cargo build --release
```

发布版二进制文件启用 LTO 优化，生成单一自包含可执行文件。

---

## 故障排除

### 应用无法启动

**现象**：`cargo run` 立即退出或显示错误信息。

**解决方案**：

1. 确保 Rust 工具链为最新版本：`rustup update stable`
2. 验证所有依赖可编译：`cargo check`
3. macOS 用户需确保已安装 Xcode Command Line Tools：`xcode-select --install`

### DeepSeek API 返回错误

**现象**：状态栏显示 "错误: API 返回 401" 或类似信息。

**解决方案**：

1. 验证 `.env` 中的 API Token：`cat .env`
2. 在 https://platform.deepseek.com/api_keys 检查 Token 有效性
3. 确认 Token 以 `sk-` 开头
4. 测试连通性：`curl -H "Authorization: Bearer $DEEPSEEK_TOKEN" https://api.deepseek.com/v1/models`

### 终端显示异常

**现象**：调整窗口大小后出现乱码或 UI 组件错位。

**解决方案**：

1. 应用自动响应 `Resize` 事件。只需调整终端窗口大小即可。
2. 若异常持续，按 `Ctrl+C` 干净退出后重新启动。
3. 确保终端模拟器支持真彩色（至少 256 色）。

### macOS 编译错误

**现象**：与系统库相关的链接错误。

**解决方案**：
`reqwest` 配合 `rustls`（macOS 默认选项）应无需额外依赖即可工作。若使用 `native-tls`，请安装 OpenSSL：

```bash
brew install openssl
```

---

## 路线图

### 短期目标 (v0.2.0)

- [ ] 将 `DeepSeekClient::stream_chat()` 接入 Enter 键处理器，实现真实 API 对话
- [ ] 实现多行输入支持（Ctrl+J 插入换行）
- [ ] 实现 `@` 触发文件附件功能，支持文件系统模糊搜索
- [ ] 实现 `/compact`、`/fork`、`/export` 等斜杠命令执行引擎

### 中期目标 (v0.3.0)

- [ ] 渗透测试命令执行工具沙箱
- [ ] 会话导出为 Markdown、JSON、HTML 格式
- [ ] 通过 YAML 配置自定义主题
- [ ] 鼠标支持：侧边栏点击切换、滚轮滚动

### 长期目标 (v1.0.0)

- [ ] gRPC 和 WebSocket 作为 REST+SSE 的替代传输方案
- [ ] 自定义 Agent 工具插件系统
- [ ] 终端录屏与回放，用于审计追溯
- [ ] 跨平台 CI/CD 流水线实现自动化发布

---

## 贡献指南

欢迎所有形式的贡献。请遵循标准的 Fork + Pull Request 工作流：

### 工作流

1. 在 GitCode 上 Fork 本仓库。
2. 从 `main` 分支创建特性分支：
   ```bash
   git checkout -b feature/你的特性名称
   ```
3. 实现你的修改，遵循项目代码规范（见开发指南章节）。
4. 确保所有修改编译无警告：
   ```bash
   cargo build
   cargo test
   ```
5. 使用 Conventional Commits 格式编写清晰的提交信息：
   ```
   feat: 添加某功能描述
   fix: 修复某问题描述
   chore: 维护描述
   ```
6. 推送分支并在 GitCode 创建 Pull Request，目标分支为 `main`。
7. 确保 PR 描述清楚地说明问题、解决方案及任何破坏性变更。

### 提交信息规范

本项目遵循 [Conventional Commits](https://www.conventionalcommits.org/) 规范：

| 前缀        | 用途                     |
| ----------- | ------------------------ |
| `feat:`     | 新功能或新特性           |
| `fix:`      | Bug 修复                 |
| `chore:`    | 维护、依赖更新、工具配置 |
| `docs:`     | 文档修改                 |
| `refactor:` | 代码重构（无行为变更）   |
| `test:`     | 添加或修改测试           |

### 代码审查标准

所有 PR 必须满足：

- 零编译器警告（`cargo build` 干净通过）
- 遵守上述代码风格规范
- 完善的中文模块级和项目级文档注释
- 函数体内部无任何注释

---

## 许可证

本项目采用 MIT 许可证。

```
Copyright (c) 2026 钟智强

在此授予任何获取本软件及相关文档文件副本的人士免费许可，
允许其不受限制地使用、复制、修改、合并、发布、分发、再许可
和/或销售本软件的副本，并允许获得本软件的人士在满足以下条件
的情况下这样做：

上述版权声明和本许可声明应包含在本软件的所有副本或实质部分中。

本软件按"原样"提供，不提供任何形式的明示或暗示担保，包括但不
限于适销性、特定用途适用性和非侵权的担保。在任何情况下，作者
或版权持有人均不对因本软件或本软件的使用或其他交易引起的任何
索赔、损害或其他责任负责，无论诉讼是合同、侵权还是其他形式。
```

---

## 联系方式

**作者**：钟智强 <哪吒网络安全>

- 邮箱：johnmelodymel@qq.com
- 钉钉：ctkqiang@dingtalk.com

**仓库地址**：https://gitcode.com/ctkqiang_sr/nezha_cyber

若需报告 Bug、请求新功能或进行安全披露，请在仓库中提交 Issue。涉及敏感安全事项，请直接通过邮件联系作者。

---

_Rust 锻造，DeepSeek 驱动。为追求速度、精准与终端原生体验的网络安全专业人士而打造。_
