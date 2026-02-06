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

NDC REPL 支持多种开发意图：

| 意图类型 | 示例 | 说明 |
|----------|------|------|
| **创建任务** | `创建一个 HTTP 服务器` | 创建新任务 |
| **执行操作** | `运行测试` | 执行任务或测试 |
| **查看状态** | `查看日志` | 查看执行日志 |
| **Git 操作** | `查看 git 状态` | Git 操作 |
| **代码操作** | `修改配置文件` | 文件操作 |

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

NDC 支持通过配置文件自定义行为：

```yaml
# ~/.ndc/config.yaml
default_shell: bash
editor: vim
git_autocommit: false
max_concurrent_tasks: 4
quality_gates:
  - cargo check
  - cargo test
```

---

## 常见问题

### Q: 任务执行失败怎么办？

```bash
# 查看详细日志
ndc logs <task-id> --verbose

# 回滚到上一个快照
ndc rollback <task-id> latest
```

### Q: 如何终止正在运行的任务？

```bash
ndc stop <task-id>
```

### Q: REPL 不识别我的意图怎么办？

1. 使用更明确的关键字：`create`、`run`、`status`
2. 简化描述，一次一个操作
3. 使用 `help` 查看支持的命令

### Q: gRPC 连接失败？

1. 检查服务是否启动：`ndc daemon`
2. 确认端口配置正确
3. 检查防火墙设置

---

## 命令速查

```bash
# 任务管理
ndc create "标题" -d "描述"     # 创建任务
ndc list                         # 列出任务
ndc status <id>                  # 查看状态
ndc run <id>                     # 执行任务
ndc stop <id>                    # 终止任务
ndc logs <id>                    # 查看日志

# REPL
ndc repl                         # 启动 REPL

# 系统
ndc daemon --grpc                # 启动 gRPC
ndc search <关键词>              # 搜索记忆
ndc --help                       # 帮助
```

---

## 下一步

- [gRPC 客户端库文档](./GRPC_CLIENT.md) - 程序化集成
- [架构设计文档](./ARCHITECTURE.md) - 了解 NDC 内部原理
- [API 参考](./API.md) - 详细 API 文档
