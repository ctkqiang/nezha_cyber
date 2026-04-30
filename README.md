# 哪吒网络安全 (Nezha Cyber)

**DeepSeek 驱动的终端 UI 红队辅助工具 —— 哪吒之魔童降世角色人格**

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
9. [文件工具与项目创建](#文件工具与项目创建)
10. [对话持久化](#对话持久化)
11. [DeepSeek API 集成](#deepseek-api-集成)
12. [快捷键速查](#快捷键速查)
13. [项目结构](#项目结构)
14. [开发指南](#开发指南)
15. [故障排除](#故障排除)
16. [路线图](#路线图)
17. [贡献指南](#贡献指南)
18. [许可证](#许可证)

---

## 项目概述

哪吒网络安全是一款高性能终端 UI (TUI) 应用，专为红队演练、渗透测试和漏洞研究打造。默认 AI 角色「哪吒」来自电影《哪吒之魔童降世》——魔丸转世、叛逆正义、嘴毒心软——以少年语气与你对话。

**核心设计哲学**：Elm Architecture (Model-Update-View) 模式，纯函数状态转换，`tokio::sync::mpsc` 通道传递事件，零成本抽象的 Rust 惯用模式。

**作者**：钟智强
**仓库**：https://gitcode.com/ctkqiang_sr/nezha_cyber
**语言**：Rust (2021 edition) | **许可证**：MIT | **测试**：107 passed

---

## 核心特性

### 角色人格 AI

- 默认 Agent「哪吒」—— 魔丸降世，自称"小爷"，口头禅"我是小妖怪，逍遥又自在"
- 纯中文输出，绝不说英文
- 按 `Ctrl+1/2/3` 或 `Ctrl+Tab` 快速切换 Agent（哪吒 / 代码审计专家 / 威胁情报分析师）

### 实时流式对话

- SSE 流式通信，逐 Token 渲染
- AI 思考中动画指示器（"[哪吒] 正在思考..."）
- 自动滚动到底部，手动上滚查看历史时不自动滚动

### 文件工具与项目创建（Claude Code 风格）

- 4 个本地文件操作工具：`write_file` / `read_file` / `create_directory` / `list_directory`
- AI 请求工具调用 → 弹出居中确认对话框 → 用户按 `Y` 确认 / `N` 拒绝
- 路径沙箱保护，禁止越界访问当前目录
- `read_file` 输出自动截断 200 行

### 对话持久化（SQLite）

- 基于 `rusqlite` 内嵌 SQLite，无需系统安装
- `/save` 保存当前对话，`/load <id>` 加载历史对话
- `/history` 列出所有已保存对话
- 退出时自动保存（可配置 `auto_save` 偏好）

### 命令面板（Ctrl+K）

- 模糊搜索所有命令：`/model` `/theme` `/agent` `/save` `/load` `/history` `/new` `/close`
- 实时过滤，Enter 执行

### Token 用量与账户余额

- 实时 Token 计数（提示 / 生成 / 总计）
- 费用计算（基于 DeepSeek 定价 ¥2.0 / ¥8.0 每百万 Token）
- 侧边栏实时显示账户余额（¥）

### 终端 UI

- 分屏布局：左侧可折叠侧边栏 + 右侧聊天区
- 消息自动宽度换行，中文友好
- 两套内置主题：赛博暗色 + 日光亮色 (`/theme daylight`)
- 多标签页会话管理（`Ctrl+N`）

---

## 技术架构

### Elm Architecture

```
用户按键 / API 事件 / Tick
     │
     ▼
Action (mpsc 通道)  ──►  update()  ──►  修改 App 状态
                               │
                               ▼
                         render() 重绘界面
```

- **Model**: `App` 全局状态（标签页、Agent 配置、记忆库、余额、确认对话框）
- **Update**: `update()` 处理 30+ Action 变体
- **View**: 9 个纯渲染函数

---

## 依赖栈

| 依赖 | 版本 | 用途 |
|------|------|------|
| `ratatui` | 0.29 | 终端 UI 框架 |
| `crossterm` | 0.28 | 终端控制 |
| `tokio` | 1 | 异步运行时 |
| `reqwest` | 0.12 | HTTP + SSE 流 |
| `rusqlite` | 0.31 (bundled) | 对话持久化 |
| `serde` + `serde_json` + `serde_yaml` | 1 / 1 / 0.9 | 序列化 |
| `uuid` | 1 | 消息唯一标识 |
| `chrono` | 0.4 | 时间戳 |
| `unicode-width` | 0.2 | 中日韩字符宽度 |
| `dotenvy` | 0.15 | .env 文件加载 |

总计 14 个直接依赖，编译为单一二进制文件。

---

## 安装指南

### 前置条件

- Rust toolchain（stable，2021 edition+）：`curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
- DeepSeek API Token（https://platform.deepseek.com）

### 从源码构建

```bash
git clone https://gitcode.com/ctkqiang_sr/nezha_cyber.git
cd nezha_cyber
echo 'DEEPSEEK_TOKEN=sk-你的token' > .env
cargo build --release
./target/release/nezha_cyber
```

### 通过 cargo install 安装

```bash
cargo install --git https://gitcode.com/ctkqiang_sr/nezha_cyber.git
```

---

## 配置说明

### 三种方式设置 API Key

| 方式 | 说明 |
|------|------|
| `.env` 文件 | 项目根目录创建 `.env` → `DEEPSEEK_TOKEN=sk-xxx` |
| `export` | `export DEEPSEEK_TOKEN=sk-xxx && nezha_cyber` |
| `config.yaml` | YAML 配置文件中的 `api_key` 字段 |

优先级：`.env` → `config.yaml` → 环境变量。同时支持旧版 `DEEPSEEK_API_KEY`。

### config.yaml（可选）

```yaml
api_base: 'https://api.deepseek.com'
default_model: 'deepseek-v4-pro'
default_pricing:
  prompt_price_per_m: 2.0
  completion_price_per_m: 8.0
```

### agents.yaml（可选）

```yaml
agents:
  - name: '自定义Agent'
    description: '功能描述'
    system_prompt: '系统提示词...'
    model: 'deepseek-v4-pro'
    tools: []
```

---

## 使用指南

### 基本交互

1. 在底部输入区键入消息
2. 按 `Enter` 发送
3. AI 回复逐 Token 流式显示
4. 左侧边栏查看 Token 用量、费用和余额

### Agent 切换

| 方式 | 操作 |
|------|------|
| 快捷键 | `Ctrl+1` `Ctrl+2` `Ctrl+3` 或 `Ctrl+Tab` 循环 |
| 命令面板 | `Ctrl+K` → `/agent 哪吒` |

### 文件工具工作流

```
用户: "帮我写一个 Python FastAPI 项目"
→ 哪吒: 调用 list_directory 查看当前目录
→ 弹出确认对话框 ─ 按 Y 确认
→ 哪吒规划架构 → 调用 create_directory + write_file × N
→ 每次写入弹出确认 ─ 按 Y/N 决定是否落盘
→ 项目创建完成
```

### 对话记忆

| 命令 | 功能 |
|------|------|
| `/save` | 保存当前对话 |
| `/load <id>` | 加载历史对话 |
| `/history` | 列出所有已保存对话 |

---

## 智能体系统

### 哪吒（默认）

魔丸降世，性格叛逆又正义，嘴毒心软。自称"小爷"，说话带少年痞气。红队高手。

### 代码审计专家

源代码安全审查、漏洞分析、安全编码规范。

### 威胁情报分析师

APT 攻击分析、恶意软件行为、攻击链追踪、IOC 提取。

---

## DeepSeek API 集成

- **端点**: `POST https://api.deepseek.com/chat/completions`
- **余额查询**: `GET https://api.deepseek.com/user/balance`
- **协议**: SSE 流式传输
- **认证**: `Bearer Token`
- **定价**: 输入 ¥2.00 / 百万 Token，输出 ¥8.00 / 百万 Token

---

## 快捷键速查

### 全局

| 快捷键 | 功能 |
|--------|------|
| `Ctrl+C` | 退出 |
| `Ctrl+N` | 新建标签页 |
| `Ctrl+K` | 命令面板 |
| `Ctrl+B` | 折叠侧边栏 |
| `Ctrl+1/2/3` | 切换 Agent |
| `Ctrl+Tab` | 下一个 Agent |
| `Esc` | 关闭面板/拒绝工具调用 |

### 输入区

| 快捷键 | 功能 |
|--------|------|
| `Enter` | 发送消息 |
| `Backspace` | 删除字符 |
| `Up/Down` | 滚动消息 |
| `PgUp/PgDn` | 滚动 5 行 |

### 工具调用确认

| 按键 | 操作 |
|------|------|
| `Y` | 确认执行（写文件/读文件/建目录） |
| `N` | 拒绝 |
| `Esc` | 忽略（同拒绝） |

---

## 项目结构

```
nezha_cyber/
├── Cargo.toml
├── .env                    # API Token（不入版本控制）
├── config.yaml             # 应用配置（可选）
├── agents.yaml             # 智能体定义（可选）
├── src/
│   ├── main.rs             # 入口 + 事件循环 + 键盘处理
│   ├── action.rs           # Action 枚举（30+ 变体）
│   ├── app.rs              # App 全局状态 + update()
│   ├── api/
│   │   ├── types.rs        # 数据结构（Message, Usage, Role 等）
│   │   └── deepseek.rs     # DeepSeek SSE 流式客户端
│   ├── agent/
│   │   └── config.rs       # YAML 配置解析 + AppConfig
│   ├── persistence/
│   │   ├── mod.rs
│   │   └── db.rs           # SQLite CRUD（MemoryStore）
│   ├── tools/
│   │   ├── mod.rs
│   │   └── executor.rs     # 文件工具本地执行器 + 路径沙箱
│   └── ui/
│       ├── layout.rs       # 布局计算
│       ├── theme.rs        # 2 套主题
│       └── render.rs       # 9 个渲染函数
└── tests/
    └── integration_test.rs
```

---

## 开发指南

### 搭建

```bash
git clone https://gitcode.com/ctkqiang_sr/nezha_cyber.git
cd nezha_cyber
echo 'DEEPSEEK_TOKEN=sk-xxx' > .env
cargo build && cargo run
```

### 运行测试

```bash
cargo test   # 107 passed, 0 failed
```

### 生产构建

```bash
cargo build --release
```

---

## 故障排除

### API 返回错误

1. 检查 `.env` 中的 Token 是否有效且以 `sk-` 开头
2. 验证端点连通：`curl -H "Authorization: Bearer $DEEPSEEK_TOKEN" https://api.deepseek.com/user/balance`
3. 确认 API Base 为 `https://api.deepseek.com`（无 `/v1`）

### 余额查询失败

- 未设置 Token 时显示"检查中…"，设好 Token 后重启
- Token 过期/无效时侧边栏显示"余额不足"

### 终端显示异常

- 调整窗口大小自动重排
- 极端情况按 `Ctrl+C` 退出重进

---

## 路线图

### 已完成 (v0.1.0)

- [x] Elm Architecture 架构
- [x] DeepSeek SSE 流式对话
- [x] 哪吒角色人格注入
- [x] SQLite 对话持久化（/save /load /history）
- [x] 文件工具 + Claude Code 风格确认对话框
- [x] 命令面板 Ctrl+K 模糊搜索
- [x] Agent 切换（Ctrl+1/2/3 / Ctrl+Tab）
- [x] 账户余额实时显示
- [x] .env 文件支持
- [x] 107 单元测试

### 短期 (v0.2.0)

- [ ] 多行输入支持（Ctrl+J 换行）
- [ ] `@` 文件附件自动补全
- [ ] `/compact` `/fork` `/export` 命令执行引擎
- [ ] 自定义工具插件系统

### 中期 (v0.3.0)

- [ ] 会话导出为 Markdown / JSON / HTML
- [ ] 鼠标支持：侧边栏点击，滚轮滚动
- [ ] 自定义主题 YAML 配置

### 长期 (v1.0.0)

- [ ] gRPC / WebSocket 替代传输
- [ ] 终端录屏与审计回放
- [ ] 跨平台 CI/CD 自动化发布

---

## 贡献指南

1. Fork 仓库 → 从 `main` 创建分支
2. 遵循项目代码规范（见 `.trae/rules/project.md`）
3. 确保 `cargo build` 零 warning，`cargo test` 全绿
4. 使用 Conventional Commits 格式提交
5. 提交 Pull Request

---

## 许可证

MIT License — 详见 [LICENSE](https://gitcode.com/ctkqiang_sr/nezha_cyber)
