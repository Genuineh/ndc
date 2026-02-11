# NDC E2E 测试方案

## 1. 背景与目标

### 1.1 为什么需要E2E测试

NDC是一个复杂的自治系统，包含：
- 任务管理与执行
- 多种工具系统（FS/Git/Shell/Web等）
- 权限与安全机制
- 质量门禁验证
- 记忆系统
- REPL交互模式

单元测试无法验证这些组件之间的集成是否正常工作。E2E测试将验证完整用户场景。

### 1.2 测试目标

1. **验证核心用户场景** - 从CLI命令到系统响应的完整链路
2. **确保功能正确性** - 任务创建→执行→验证的完整流程
3. **检测回归问题** - 新代码不破坏已有功能
4. **提供信心** - 让开发者放心重构

---

## 2. 测试架构

### 2.1 测试分层

```
┌─────────────────────────────────────────────────────────────┐
│                    E2E 测试层                               │
│  ┌─────────────┐ ┌─────────────┐ ┌─────────────────────┐ │
│  │  CLI 测试   │ │ REPL 测试   │ │   集成场景测试      │ │
│  └─────────────┘ └─────────────┘ └─────────────────────┘ │
├─────────────────────────────────────────────────────────────┤
│                   服务层 (NDC Runtime)                      │
│  ┌─────────────┐ ┌─────────────┐ ┌─────────────────────┐ │
│  │  Executor  │ │  Workflow   │ │    ToolManager      │ │
│  └─────────────┘ └─────────────┘ └─────────────────────┘ │
├─────────────────────────────────────────────────────────────┤
│                    数据层                                   │
│  ┌─────────────┐ ┌─────────────┐ ┌─────────────────────┐ │
│  │  Memory    │ │  Storage    │ │   Task/Intent       │ │
│  └─────────────┘ └─────────────┘ └─────────────────────┘ │
└─────────────────────────────────────────────────────────────┘
```

### 2.2 测试类型

| 类型 | 文件 | 说明 |
|------|------|------|
| **CLI测试** | `e2e/cli_tests.rs` | 通过子进程调用 `cargo run --` |
| **API测试** | `e2e/api_tests.rs` | 直接调用Runtime API |
| **场景测试** | `e2e/scenario_tests.rs` | 完整用户故事验证 |

---

## 3. 测试场景设计

### 3.1 任务生命周期测试

```
测试用例 1.1: 完整任务生命周期
─────────────────────────────────────────
前置条件: NDC已安装，可访问临时项目目录

步骤:
1. 创建任务: ndc create "Add login feature"
   - 验证: 任务ID生成，状态为Pending

2. 查看任务: ndc status <task_id>
   - 验证: 显示任务详情

3. 开始执行: ndc run <task_id> --sync
   - 验证: 状态变为InProgress，执行完成

4. 验证状态: ndc status <task_id>
   - 验证: 状态变为Completed

5. 查看日志: ndc logs <task_id>
   - 验证: 包含执行步骤记录

预期结果: 任务完整执行，所有状态转换正确
```

```
测试用例 1.2: 任务回滚
─────────────────────────────────────────
步骤:
1. 创建并执行任务: 创建文件修改任务
2. 查看快照: ndc status <task_id> --snapshots
3. 回滚: ndc rollback <task_id> <snapshot_id>
4. 验证: 文件内容已还原
```

### 3.2 工具系统测试

```
测试用例 2.1: 文件系统工具
─────────────────────────────────────────
步骤:
1. 创建任务: ndc create "Test file operations"
2. 执行FS工具:
   - list <dir>     → 验证列出正确内容
   - read <file>    → 验证读取内容
   - write <file>   → 验证写入成功
   - edit <file>    → 验证编辑成功
   - glob "*.rs"    → 验证模式匹配
3. 验证文件状态正确
```

```
测试用例 2.2: Git工具
─────────────────────────────────────────
步骤:
1. 初始化git仓库
2. 创建任务: ndc create "Test git operations"
3. 执行Git工具:
   - git status     → 验证显示正确
   - git branch     → 验证分支列表
4. 提交更改: git commit
5. 验证提交历史
```

```
测试用例 2.3: Shell工具
─────────────────────────────────────────
步骤:
1. 创建任务: ndc create "Test shell"
2. 执行安全命令: echo "hello"
3. 验证输出: "hello"
4. 尝试危险命令: rm -rf / (应被阻止)
```

```
测试用例 2.4: Grep/Glob工具
─────────────────────────────────────────
步骤:
1. 创建包含代码的项目
2. 执行grep: "fn main" → 验证找到匹配
3. 执行glob: "**/*.rs" → 验证文件列表
```

### 3.3 质量门禁测试

```
测试用例 3.1: 测试执行
─────────────────────────────────────────
步骤:
1. 创建包含测试的项目
2. 创建任务: ndc create "Run tests"
3. 执行任务(带质量门禁)
4. 验证: 测试结果正确显示

预期结果: 测试通过/失败正确报告
```

```
测试用例 3.2: Lint检查
─────────────────────────────────────────
步骤:
1. 创建包含 lint 问题的代码
2. 执行任务
3. 验证: lint 警告正确报告
```

### 3.4 权限系统测试

```
测试用例 4.1: 权限请求流程
─────────────────────────────────────────
步骤:
1. 配置权限系统
2. 创建需要高权限的任务
3. 尝试执行 (应触发权限请求)
4. 验证权限请求正确显示
```

### 3.5 记忆系统测试

```
测试用例 5.1: 记忆存储与检索
─────────────────────────────────────────
步骤:
1. 创建任务: ndc create "Test memory"
2. 执行产生记忆的操作
3. 搜索记忆: ndc search "keyword"
4. 验证: 记忆正确存储和检索
```

### 3.6 REPL模式测试

```
测试用例 6.1: 自然语言理解
─────────────────────────────────────────
步骤:
1. 启动REPL: ndc repl
2. 输入: "Create a new function"
3. 验证:
   - 意图被正确识别
   - 任务被正确创建

步骤:
1. 输入: "Fix the bug in login"
2. 验证:
   - 意图: FixBug
   - 目标: bug
   - 实体: login
```

### 3.7 集成场景测试

```
测试用例 7.1: 完整功能测试 - 实现新功能
─────────────────────────────────────────
场景: 实现用户认证功能

步骤:
1. REPL输入: "Add user authentication"
2. 系统自动:
   - 解析意图 (CreateFeature)
   - 创建任务
   - 列出相关文件 (Glob)
   - 读取现有代码 (Read)
   - 创建新文件 (Write)
   - 运行测试 (QualityGate)
   - 记录决策 (Memory)

验证:
- 功能正确实现
- 测试通过
- 记忆已存储
```

---

## 4. 测试实现方案

### 4.1 测试项目结构

```
tests/
├── e2e/
│   ├── mod.rs              # 测试入口
│   ├── cli_tests.rs        # CLI命令测试
│   ├── task_tests.rs       # 任务生命周期测试
│   ├── tool_tests.rs       # 工具系统测试
│   ├── quality_gate_tests.rs # 质量门禁测试
│   ├── memory_tests.rs     # 记忆系统测试
│   ├── repl_tests.rs       # REPL测试
│   └── scenario_tests.rs   # 完整场景测试
│
├── fixtures/               # 测试数据
│   ├── simple_project/    # 简单测试项目
│   ├── rust_project/      # Rust项目
│   └── multi_file_project/ # 多文件项目
```

### 4.2 测试基础设施

```rust
// tests/e2e/mod.rs

/// 测试夹具管理器
pub struct TestFixture {
    temp_dir: TempDir,
    ndc_path: PathBuf,
    project: TestProject,
}

/// 测试项目创建
impl TestProject {
    pub fn new(name: &str) -> Self { ... }
    pub fn add_file(&self, path: &str, content: &str) { ... }
    pub fn add_rs_file(&self, name: &str, content: &str) { ... }
    pub fn add_test(&self, name: &str, test_code: &str) { ... }
}

/// NDC CLI 调用封装
impl NdcCli {
    pub fn new() -> Self { ... }
    pub fn create_task(&self, title: &str) -> TaskResult { ... }
    pub fn run_task(&self, id: &str, sync: bool) -> RunResult { ... }
    pub fn list_tasks(&self, state: Option<&str>) -> Vec<Task> { ... }
    pub fn status(&self, id: &str) -> TaskStatus { ... }
    pub fn logs(&self, id: &str, lines: usize) -> String { ... }
}
```

### 4.3 测试辅助函数

```rust
// 等待任务完成
async fn wait_for_completion(cli: &NdcCli, task_id: &str, timeout: Duration) -> TaskStatus {
    let start = Instant::now();
    loop {
        let status = cli.status(task_id);
        if status.is_terminal() {
            return status;
        }
        if start.elapsed() > timeout {
            panic!("Timeout waiting for task completion");
        }
        sleep(Duration::from_millis(100)).await;
    }
}

// 验证文件内容
fn assert_file_contains(path: &Path, expected: &str) {
    let content = fs::read_to_string(path).unwrap();
    assert!(content.contains(expected),
        "File {} should contain:\n{}\nActual:\n{}",
        path.display(), expected, content);
}
```

---

## 5. 测试用例详细设计

### 5.1 CLI测试 (cli_tests.rs)

```rust
#[tokio::test]
async fn test_create_task() {
    let fixture = TestFixture::new("test_create").await;
    let cli = NdcCli::new();

    // 执行
    let result = cli.create_task("Test task");

    // 验证
    assert!(result.success);
    assert!(result.task_id.starts_with("task-"));
    assert_eq!(result.state, "Pending");
}

#[tokio::test]
async fn test_create_and_run_task() {
    let fixture = TestFixture::new("test_run").await;
    fixture.add_file("test.txt", "hello");

    let cli = NdcCli::new();
    let create = cli.create_task("Test run").await;
    let run = cli.run_task(&create.task_id, true).await;

    assert!(run.success);
    assert!(run.logs.contains("Completed"));
}

#[tokio::test]
async fn test_quality_gate_failure_blocks_completion() {
    let fixture = TestFixture::new("test_qg");
    fixture.add_rs_file("lib.rs", r#"
        fn unused_function() {
            let x = 1; // dead code warning
        }
    "#);

    let cli = NdcCli::new();
    let create = cli.create_task("Add feature");
    let result = cli.run_task(&create.task_id, true).await;

    // 质量门禁失败，任务不应完成
    assert!(!result.success || result.state == "Blocked");
}
```

### 5.2 工具测试 (tool_tests.rs)

```rust
#[tokio::test]
async fn test_read_write_file() {
    let fixture = TestFixture::new("test_rw");
    let cli = NdcCli::new();
    let task = cli.create_task("Test RW").await;

    // Write
    let write_result = cli.exec_tool(&task, "write", json!({
        "path": "test.txt",
        "content": "Hello, NDC!"
    })).await;
    assert!(write_result.success);

    // Read
    let read_result = cli.exec_tool(&task, "read", json!({
        "path": "test.txt"
    })).await;
    assert!(read_result.success);
    assert!(read_result.output.contains("Hello, NDC!"));
}

#[tokio::test]
async fn test_glob_pattern() {
    let fixture = TestFixture::new("test_glob");
    fixture.add_rs_file("lib.rs", "fn main() {}");
    fixture.add_rs_file("utils.rs", "fn helper() {}");
    fixture.add_file("README.md", "# Test");

    let cli = NdcCli::new();
    let task = cli.create_task("Test glob").await;

    let result = cli.exec_tool(&task, "glob", json!({
        "pattern": "**/*.rs"
    })).await;

    assert!(result.success);
    assert!(result.output.contains("lib.rs"));
    assert!(result.output.contains("utils.rs"));
}

#[tokio::test]
async fn test_grep_search() {
    let fixture = TestFixture::new("test_grep");
    fixture.add_rs_file("lib.rs", r#"
        fn main() {
            println!("Hello");
        }
        fn helper() {
            println!("World");
        }
    "#);

    let cli = NdcCli::new();
    let task = cli.create_task("Test grep").await;

    let result = cli.exec_tool(&task, "grep", json!({
        "pattern": "println",
        "path": "lib.rs"
    })).await;

    assert!(result.success);
    assert!(result.output.contains("Hello"));
    assert!(result.output.contains("World"));
}

#[tokio::test]
async fn test_shell_safe_command() {
    let cli = NdcCli::new();
    let task = cli.create_task("Test shell").await;

    let result = cli.exec_tool(&task, "bash", json!({
        "command": "echo 'Hello World'"
    })).await;

    assert!(result.success);
    assert!(result.output.contains("Hello World"));
}

#[tokio::test]
async fn test_shell_blocks_dangerous_command() {
    let cli = NdcCli::new();
    let task = cli.create_task("Test dangerous").await;

    let result = cli.exec_tool(&task, "bash", json!({
        "command": "rm -rf /"
    })).await;

    // 应该被阻止
    assert!(!result.success);
    assert!(result.error.contains("blocked") || result.error.contains("dangerous"));
}
```

### 5.3 任务生命周期测试 (task_tests.rs)

```rust
#[tokio::test]
async fn test_task_state_transitions() {
    let fixture = TestFixture::new("test_states");
    let cli = NdcCli::new();

    // 1. 创建
    let task = cli.create_task("Test states").await;
    assert_eq!(task.state, "Pending");
    let task_id = task.task_id;

    // 2. 开始执行
    cli.run_task(&task_id, false).await;
    let status = cli.status(&task_id);
    assert!(matches!(status, TaskState::InProgress));

    // 3. 等待完成
    let completed = wait_for_completion(&cli, &task_id, Duration::from_secs(30)).await;
    assert!(matches!(completed, TaskState::Completed));
}

#[tokio::test]
async fn test_task_rollback() {
    let fixture = TestFixture::new("test_rollback");
    fixture.add_file("original.txt", "original content");

    let cli = NdcCli::new();
    let task = cli.create_task("Modify file").await;

    // 修改文件
    cli.exec_tool(&task, "write", json!({
        "path": "modified.txt",
        "content": "modified content"
    })).await;

    // 获取快照ID
    let snapshots = cli.get_snapshots(&task);
    assert!(!snapshots.is_empty());
    let snapshot_id = &snapshots[0].id;

    // 回滚
    let rollback = cli.rollback(&task, snapshot_id).await;
    assert!(rollback.success);
}

#[tokio::test]
async fn test_task_dependencies() {
    let cli = NdcCli::new();

    // 创建父任务
    let parent = cli.create_task("Parent task").await;

    // 创建子任务
    let child = cli.create_task("Child task")
        .with_parent(&parent)
        .await;

    // 验证依赖
    let status = cli.status(&parent);
    assert!(status.children.contains(&child.id));
}
```

### 5.4 REPL测试 (repl_tests.rs)

```rust
#[tokio::test]
async fn test_repl_create_task() {
    let mut repl = ReplSession::new();

    // 输入创建任务
    let response = repl.send("Create a new API endpoint").await;

    // 验证
    assert!(response.intent.contains("Create"));
    assert!(response.entity.contains("API"));
    assert!(response.task_id.starts_with("task-"));
}

#[tokio::test]
async fn test_repl_intent_parsing() {
    let mut repl = ReplSession::new();

    let cases = vec![
        ("Add login feature", RequirementIntent::CreateFeature),
        ("Fix the bug", RequirementIntent::FixBug),
        ("Refactor the code", RequirementIntent::Refactor),
        ("Add tests", RequirementIntent::Test),
        ("Document the API", RequirementIntent::Document),
    ];

    for (input, expected_intent) in cases {
        let response = repl.send(input).await;
        assert_eq!(response.intent, expected_intent,
            "Failed for: {}", input);
    }
}
```

### 5.5 质量门禁测试 (quality_gate_tests.rs)

```rust
#[tokio::test]
async fn test_quality_gate_runs_tests() {
    let fixture = TestFixture::new("test_tests");
    fixture.add_rs_file("lib.rs", r#"
        #[cfg(test)]
        mod tests {
            #[test]
            fn it_works() {
                assert_eq!(2 + 2, 4);
            }
        }
    "#);

    let cli = NdcCli::new();
    let task = cli.create_task("Run tests").await;
    let result = cli.run_task(&task, true).await;

    assert!(result.success);
    assert!(result.quality_gate_results.tests_passed > 0);
}

#[tokio::test]
async fn test_quality_gate_runs_clippy() {
    let fixture = TestFixture::new("test_clippy");
    fixture.add_rs_file("lib.rs", r#"
        fn main() {
            let x = 1; // dead code warning
        }
    "#);

    let cli = NdcCli::new();
    let task = cli.create_task("Run clippy").await;
    let result = cli.run_task(&task, true).await;

    // 应该检测到lint问题
    assert!(result.quality_gate_results.lint_warnings > 0 ||
            !result.quality_gate_results.lint_output.is_empty());
}
```

---

## 6. CI/CD集成

### 6.1 GitHub Actions工作流

```yaml
name: E2E Tests

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

jobs:
  e2e-tests:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Setup Rust
        uses: actions-rs/toolchain@v1
        with:
          toolchain: stable

      - name: Build NDC
        run: cargo build --release

      - name: Run E2E Tests
        run: |
          cargo test --test e2e -- --test-threads=4
        env:
          NDC_HOME: ${{ runner.temp }}/.ndc

      - name: Upload Test Results
        uses: actions/upload-artifact@v4
        if: always()
        with:
          name: e2e-test-results
          path: test-results/
```

---

## 7. 测试覆盖度目标

| 功能模块 | 核心测试用例 | 优先级 |
|---------|-------------|--------|
| CLI命令 | 8 | P0 |
| 任务管理 | 6 | P0 |
| 文件系统工具 | 5 | P0 |
| Git工具 | 4 | P1 |
| Shell工具 | 3 | P0 |
| Grep/Glob | 3 | P1 |
| 质量门禁 | 4 | P0 |
| REPL | 5 | P1 |
| 记忆系统 | 3 | P2 |
| 权限系统 | 3 | P2 |

**目标**: P0功能100%覆盖，P1功能80%覆盖

---

## 8. 执行方式

```bash
# 运行所有E2E测试
cargo test --test e2e

# 运行特定测试
cargo test --test e2e cli_tests
cargo test --test e2e task_lifecycle
cargo test --test e2e tool_tests

# 运行特定场景
cargo test --test e2e scenario_tests::full_feature_implementation

# 运行并显示详细输出
cargo test --test e2e -- --nocapture --test-threads=1
```

---

## 9. 实施步骤

1. **创建测试基础设施**
   - 创建 `tests/e2e/` 目录
   - 实现 `TestFixture` 和 `NdcCli` 封装
   - 创建测试夹具项目

2. **实现核心测试**
   - CLI命令测试
   - 任务生命周期测试
   - 工具系统测试

3. **扩展测试覆盖**
   - 质量门禁测试
   - REPL测试
   - 记忆系统测试

4. **集成CI/CD**
   - 配置GitHub Actions
   - 设置测试报告

5. **持续维护**
   - 新功能添加测试
   - 修复失败的测试
   - 定期更新测试数据
