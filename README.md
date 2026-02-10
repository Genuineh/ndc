# NDC - 智能开发助手

> NDC (Nardos Development Client) - 智能开发助手，帮助你通过自然语言完成编码任务。

## 快速开始

### 1. 安装

```bash
# 克隆项目
git clone https://github.com/yourname/ndc.git
cd ndc

# 构建项目
cargo build --release

# 运行
./target/release/ndc --help
```

### 2. 第一次使用

```bash
# 查看帮助
./target/release/ndc --help

# 创建第一个任务
./target/release/ndc create "实现 Hello World" -d "创建一个打印 Hello World 的程序"

# 查看任务列表
./target/release/ndc list

# 启动交互式开发
./target/release/ndc repl
```

## 功能特性

| 模式 | 命令 | 用途 |
|------|------|------|
| CLI | `ndc <command>` | 快速单行操作 |
| REPL | `ndc repl` | 交互式对话开发 |
| Daemon | `ndc daemon` | gRPC 服务 |

## 常用命令速查

```bash
# 任务管理
ndc create "任务标题" -d "详细描述"     # 创建任务
ndc list                                # 列出所有任务
ndc list --state pending                 # 查看待办任务
ndc status <task-id>                    # 查看任务状态
ndc logs <task-id>                      # 查看执行日志
ndc run <task-id>                       # 执行任务
ndc run <task-id> --sync                # 同步执行（等待完成）
ndc rollback <task-id> latest            # 回滚到上一个快照

# 搜索
ndc search "关键词"                       # 搜索记忆

# 系统
ndc status-system                        # 查看系统状态
ndc repl                                # 启动交互模式
ndc daemon                              # 启动 gRPC 服务
```

## 示例：创建一个计算器

```bash
# 1. 启动 REPL
$ ndc repl

# 2. 创建任务
> create 实现一个计算器类，支持加减乘除

✅ 任务已创建: 01HABC123DEF456
标题: 实现一个计算器类

# 3. 执行任务
> run 01HABC123DEF456 --sync

⏳ 同步执行中...
✅ 任务完成! calculator.rs 已创建

# 4. 查看结果
> status 01HABC123DEF456
```

## 项目结构

```
ndc/
├── bin/                    # CLI 入口和 E2E 测试
│   ├── main.rs
│   └── tests/e2e/         # 端到端测试 (38个测试)
├── crates/
│   ├── interface/          # CLI、REPL、Daemon 接口
│   ├── core/              # 核心模型 (Task, Intent, Memory)
│   ├── decision/          # 决策引擎
│   └── runtime/           # 执行引擎、工具集
├── docs/                  # 文档
│   ├── USER_GUIDE.md      # 详细使用指南
│   ├── GRPC_CLIENT.md     # gRPC 客户端集成
│   └── LLM_INTEGRATION.md # LLM 集成说明
└── Cargo.toml
```

## 文档链接

- [用户指南](docs/USER_GUIDE.md) - 详细使用说明
- [gRPC 客户端集成](docs/GRPC_CLIENT.md) - 程序化集成
- [LLM 集成说明](docs/LLM_INTEGRATION.md) - LLM Provider 配置
- [测试计划](docs/E2E_TEST_PLAN_V2.md) - E2E 测试详情

## 测试

```bash
# 运行所有测试
cargo test --release

# 运行 E2E 测试
cargo test --test e2e --release

# 运行特定测试
cargo test --test e2e test_create_basic
```

## 系统要求

- Rust 1.70+
- Cargo
- (可选) OpenAI/Anthropic API Key 用于 LLM 功能

## 许可证

MIT
