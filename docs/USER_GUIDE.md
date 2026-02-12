# NDC 编码任务使用指南

> NDC (Nardos Development Client) - 智能开发助手

## 简介

NDC 是一个智能开发助手，支持通过 CLI、REPL 和 gRPC 三种方式完成编码任务。

## 安装与构建

```bash
# 构建项目
cargo build

# 启用 gRPC 功能
cargo build --features grpc

# 运行 CLI
./target/debug/ndc --help
```

## 使用方式概览

| 方式 | 命令 | 适用场景 |
|------|------|----------|
| CLI | `ndc <command>` | 快速操作、单行命令 |
| REPL | `ndc repl` | 交互式对话开发 |
| gRPC | `ndc daemon` | 程序化集成 |

---

## CLI 模式

### 基础命令

```bash
# 创建任务
ndc create "实现用户登录功能" -d "需要支持邮箱密码登录"

# 列出任务
ndc list
ndc list --state pending

# 查看任务状态
ndc status <task-id>

# 执行任务
ndc run <task-id>
ndc run <task-id> --sync

# 回滚任务
ndc rollback <task-id> <snapshot-id>

# 查看日志
ndc logs <task-id>

# 搜索记忆
ndc search "登录相关"

# 启动 REPL
ndc repl

# 启动 gRPC 守护进程
ndc daemon --grpc --address 127.0.0.1:50051
```

### CLI 命令详解

#### create - 创建任务

```bash
ndc create "任务标题" -d "任务描述" -t "tag1,tag2"

# 示例
ndc create "实现 REST API" -d "创建用户管理的 CRUD 接口" -d "使用 Actix-web 框架"
```

#### list - 任务列表

```bash
ndc list                      # 所有任务
ndc list --state pending      # 待执行
ndc list --state running      # 执行中
ndc list --state completed    # 已完成
ndc list --limit 10           # 限制数量
```

#### run - 执行任务

```bash
ndc run <task-id>             # 异步执行
ndc run <task-id> --sync      # 同步执行（等待完成）
ndc run <task-id> --verbose   # 详细输出
```

---

## REPL 模式 - 交互式开发

REPL 是 NDC 的核心交互模式，支持自然语言对话完成开发任务。

### 启动 REPL

```bash
ndc repl
```

### REPL 基本用法

```
> 你好！我可以帮你完成哪些开发任务？
> create 一个 HTTP 服务器处理 GET 请求
> list 查看当前任务
> run <task-id> 执行任务
> status <task-id> 检查状态
> undo 回退上一步操作
> clear 清空对话
> quit 退出
```

### REPL 意图解析

NDC REPL 使用 LLM 进行智能意图解析：

| 意图类型 | 示例 | 说明 |
|----------|------|------|
| **创建任务** | `创建一个 HTTP 服务器` | 创建新任务 |
| **执行操作** | `运行测试` | 执行任务或测试 |
| **查看状态** | `查看日志` | 查看执行日志 |
| **Git 操作** | `查看 git 状态` | Git 操作 |
| **代码操作** | `修改配置文件` | 文件操作 |

**LLM 配置**:
- 使用 `/model` 命令切换 Provider 和模型
- 支持的 Provider: MiniMax, OpenRouter, OpenAI, Anthropic, Ollama
- 环境变量使用 `NDC_` 前缀避免冲突

### REPL 完整示例

```
$ ndc repl

NDC REPL - 智能开发助手
输入 'help' 查看帮助，或描述你的任务。

> create 实现一个计算器类，支持加减乘除运算

✅ 任务已创建: 01HABC123DEF456
标题: 实现一个计算器类
描述: 支持加减乘除运算

> list
任务列表:
  [pending] 01HABC123DEF456 - 实现一个计算器类

> run 01HABC123DEF456
⏳ 任务执行中...
✅ 完成! 输出: calculator.rs 已创建

> run 01HABC123DEF456 --sync
⏳ 同步执行中...
✅ 任务完成!
```

### REPL 特殊功能

#### 从对话创建任务

在 REPL 中描述需求，NDC 会自动提取意图创建任务：

```
> 我需要实现用户认证功能，包括登录、注册和登出

✅ 已识别意图: CreateTask
✅ 任务已创建: 01HXYZ789...
```

#### 上下文保持

REPL 会记住之前的对话上下文：

```
> create 实现用户结构体
✅ 已创建任务: 01HUSER001

> 为它添加登录方法  (自动关联上文的用户结构体)
✅ 已添加到任务: 01HUSER001
```

---

## gRPC API - 程序化集成

### 启动 gRPC 服务

```bash
ndc daemon --grpc --address 127.0.0.1:50051
```

### Rust 客户端示例

```rust
use ndc_interface::{NdcClient, create_client};

#[tokio::main]
async fn main() -> Result<(), ClientError> {
    // 连接服务
    let client = create_client("127.0.0.1:50051").await?;

    // 创建任务
    let task = client.create_task(
        "实现 API 接口",
        "创建用户 CRUD API"
    ).await?;

    // 执行任务
    let result = client.execute_task(&task.task.id, true).await?;

    println!("任务状态: {}", result.status);
    println!("输出: {}", result.message);

    Ok(())
}
```

### gRPC 可用方法

| 方法 | 描述 |
|------|------|
| `health_check()` | 健康检查 |
| `create_task(title, description)` | 创建任务 |
| `get_task(task_id)` | 获取任务详情 |
| `list_tasks(limit, state_filter)` | 列出任务 |
| `execute_task(task_id, sync)` | 执行任务 |
| `rollback_task(task_id, snapshot_id)` | 回滚任务 |
| `get_system_status()` | 系统状态 |

---

## 完整开发流程示例

### 场景：实现一个 Web API

#### 步骤 1: 启动 REPL 并描述需求

```bash
$ ndc repl
> create 实现用户管理 REST API
> - GET /users - 列出所有用户
> - GET /users/:id - 获取单个用户
> - POST /users - 创建用户
> - PUT /users/:id - 更新用户
> - DELETE /users/:id - 删除用户
```

#### 步骤 2: 执行任务

```
> run <task-id> --sync
```

#### 步骤 3: 查看结果

```
> status <task-id>
> logs <task-id>
```

#### 步骤 4: 迭代改进

```
> create 为 API 添加 JWT 认证
> run <new-task-id>
```

---

## 工具集

NDC 提供以下安全工具：

### 文件操作 (fs)

| 操作 | 参数 | 说明 |
|------|------|------|
| read | path | 读取文件 |
| write | path, content | 写入文件 |
| create | path | 创建文件/目录 |
| delete | path | 删除文件/目录 |
| list | path | 列出目录 |

### Git 操作 (git)

| 操作 | 说明 |
|------|------|
| status | 查看工作区状态 |
| branch | 列出所有分支 |
| log | 查看提交历史 |
| commit | 提交更改 |
| diff_staged | 查看暂存区差异 |

### Shell 命令 (shell)

支持的命令：
- `cargo check` - 检查代码
- `cargo test` - 运行测试
- `cargo build` - 构建项目
- `cargo fmt` - 格式化代码
- `cargo clippy` - 代码检查

---

## 最佳实践

### 1. 任务分解

```
❌ 错误示例
> create 实现整个项目

✅ 正确示例
> create 初始化项目结构
> create 实现配置文件
> create 实现核心业务逻辑
> create 添加测试
> create 编写文档
```

### 2. 使用标签

```bash
ndc create "实现功能" -t "api,user,auth"
```

### 3. 定期执行测试

```bash
ndc run <task-id>
ndc run <test-task-id>
```

### 4. 使用 Git 跟踪

```bash
# 查看变更
ndc run <task-id>
git status

# 提交
git add .
git commit -m "feat: 实现功能"
```

---

## 配置文件

NDC 采用 OpenCode 风格的分层配置系统，支持多层级配置和环境变量覆盖。

### 配置分层

配置按以下优先级加载（优先级从低到高）：

| 层级 | 路径 | 说明 |
|------|------|------|
| 全局 | `/etc/ndc/config.yaml` | 系统级配置，所有用户生效 |
| 用户 | `~/.config/ndc/config.yaml` | 用户级配置，仅当前用户生效 |
| 项目 | `./.ndc/config.yaml` | 项目级配置，仅当前项目生效 |

**优先级规则**：项目 > 用户 > 全局（高层级配置覆盖低层级配置）

### 环境变量

可通过环境变量覆盖配置（推荐用于敏感信息和快速测试）：

| 环境变量 | 说明 | 默认值 |
|----------|------|--------|
| `NDC_LLM_PROVIDER` | LLM 提供商 | openai |
| `NDC_LLM_MODEL` | 模型名称 | gpt-4o |
| `NDC_LLM_API_KEY` | API Key | - |
| `NDC_LLM_BASE_URL` | API Base URL | - |
| `NDC_ORGANIZATION` | 组织 ID | - |
| `NDC_REPL_CONFIRMATION` | 确认模式 | true |
| `NDC_MAX_CONCURRENT_TASKS` | 最大并发数 | 4 |

#### MiniMax 专用环境变量

| 环境变量 | 说明 | 必需 |
|----------|------|------|
| `NDC_MINIMAX_API_KEY` | MiniMax API Key | 是 |
| `NDC_MINIMAX_GROUP_ID` | MiniMax Group ID | 推荐 |
| `NDC_MINIMAX_MODEL` | 模型名称 | 否（默认 m2.1-0107） |

### 完整配置示例

```yaml
# ~/.config/ndc/config.yaml

# LLM 配置
llm:
  enabled: true
  provider: openai
  model: gpt-4o
  # 使用 env:// 前缀从环境变量加载敏感信息
  api_key: env://OPENAI_API_KEY
  base_url: https://api.openai.com/v1
  temperature: 0.1
  max_tokens: 4096
  timeout: 60

# 多 Provider 配置
llm:
  providers:
    anthropic:
      name: anthropic
      type: anthropic
      model: claude-sonnet-4-5-20250929
      base_url: https://api.anthropic.com/v1
      api_key: env://ANTHROPIC_API_KEY

    ollama:
      name: ollama
      type: ollama
      model: llama3.2
      base_url: http://localhost:11434

# REPL 配置
repl:
  prompt: "ndc> "
  history_file: ~/.config/ndc/history.txt
  max_history: 1000
  show_thought: true
  auto_create_task: true
  session_timeout: 3600
  confirmation_mode: true

# Runtime 配置
runtime:
  max_concurrent_tasks: 4
  execution_timeout: 300
  working_dir: .
  quality_gates:
    - tests_pass
    - no_lint_errors
    - type_check

# Agent Profiles
agents:
  - name: default
    display_name: Default Agent
    description: General purpose agent with balanced settings
    provider: openai
    model: gpt-4o
    temperature: 0.1
    max_tokens: 4096
    max_tool_calls: 50
    enable_streaming: true
    auto_verify: true
    task_types:
      - "*"

  - name: implementer
    display_name: Code Implementer
    description: Specialized for implementing features and bug fixes
    provider: anthropic
    model: claude-sonnet-4-5-20250929
    temperature: 0.1
    max_tokens: 8192
    max_tool_calls: 100
    task_types:
      - implementation
      - bugfix
      - refactor
    priority: 10

  - name: verifier
    display_name: Code Verifier
    description: Specialized for verifying and reviewing code
    provider: openai
    model: gpt-4o
    temperature: 0.0
    max_tokens: 4096
    task_types:
      - verification
      - review
      - testing
    priority: 5
```

### 快速配置

**最小配置**（只需 API Key）：

```yaml
llm:
  provider: openai
  api_key: env://OPENAI_API_KEY
```

**使用本地 Ollama**：

```yaml
llm:
  provider: ollama
  model: llama3.2
  base_url: http://localhost:11434
```

**使用 MiniMax**：

```bash
# 设置环境变量
export NDC_MINIMAX_API_KEY="your-api-key"
export NDC_MINIMAX_GROUP_ID="your-group-id"  # 可选，但推荐
export NDC_MINIMAX_MODEL="m2.1-0107"  # 可选，默认 m2.1-0107

# REPL 中切换模型
/model minimax/m2.1-0107
```

---

## 常见问题

### Q: 如何配置 MiniMax？

```bash
# 1. 获取 MiniMax API Key
#    访问 https://api.minimax.chat 注册并获取 API Key

# 2. 设置环境变量
export NDC_MINIMAX_API_KEY="your-api-key"
export NDC_MINIMAX_GROUP_ID="your-group-id"  # 可选，但推荐

# 3. 启动 REPL 并切换到 MiniMax
ndc repl
/model minimax
```

或者在配置文件中设置：

```yaml
# ~/.config/ndc/config.yaml
llm:
  provider: minimax
  api_key: env://NDC_MINIMAX_API_KEY
  organization: env://NDC_MINIMAX_GROUP_ID  # Group ID
  model: m2.1-0107
```

**可用模型**：
- `m2.1-0107` - 最新一代模型（默认）
- `abab6.5s-chat` - 快速响应模型
- `abab6.5-chat` - 标准模型

### Q: REPL 中如何切换模型？

```bash
/model minimax                    # MiniMax 默认模型
/model minimax/m2.1-0107         # MiniMax M2.1
/model openai/gpt-4o             # OpenAI GPT-4o
/model anthropic/claude-sonnet    # Anthropic Claude
```

### Q: gRPC 连接失败？

1. 检查服务是否启动：`ndc daemon`
2. 确认端口配置正确
3. 检查防火墙设置

---

## 命令速查

```bash
# REPL 交互模式（主要使用方式）
ndc repl                         # 启动交互式 REPL
ndc run -m "自然语言描述"          # 单次执行

# 切换模型
/model minimax                    # 使用 MiniMax 默认模型
/model minimax/m2.1-0107         # 使用 MiniMax M2.1 模型
/model openai/gpt-4o             # 使用 OpenAI GPT-4o
/model anthropic/claude-sonnet   # 使用 Anthropic Claude

# 管理命令
ndc daemon                       # 启动后台守护进程
ndc status-system                # 显示系统状态

# REPL 内命令
/help                            # 显示帮助
/clear                          # 清屏
exit / quit / q                  # 退出 REPL
```
ndc daemon --grpc                # 启动 gRPC
ndc search <关键词>              # 搜索记忆
ndc --help                       # 帮助
```

---

## 下一步

- [gRPC 客户端库文档](./GRPC_CLIENT.md) - 程序化集成
- [架构设计文档](./ARCHITECTURE.md) - 了解 NDC 内部原理
- [API 参考](./API.md) - 详细 API 文档
