# NDC 增强E2E测试计划

## 1. 背景

当前E2E测试只有9个基础测试，需要全面增强以覆盖：
- 所有CLI命令
- 错误处理场景
- 参数验证
- 边界条件
- 状态转换

## 2. 测试分类

### 2.1 CLI命令测试 (覆盖所有10个命令)

| 命令 | 测试用例 | 说明 |
|------|----------|------|
| create | 8 | 基础创建、描述、类型、无效输入 |
| list | 5 | 过滤、限制、空列表 |
| status | 4 | 有效ID、无效ID、latest |
| logs | 4 | 行数限制、无效ID、有效日志 |
| run | 3 | 同步执行、异步(占位)、无效ID |
| rollback | 3 | 快照回滚、无效ID、latest |
| repl | 2 | help、启动 |
| daemon | 2 | help、启动 |
| search | 4 | 查询、过滤、限制 |
| status-system | 1 | 系统状态 |

### 2.2 错误处理测试

| 场景 | 测试 |
|------|------|
| 无效任务ID | ✅ |
| 任务不存在 | ✅ |
| 缺少必需参数 | ✅ |
| 无效参数值 | ✅ |
| 文件路径无效 | ✅ |
| 命令超时 | ✅ |

### 2.3 边界条件测试

| 场景 | 测试 |
|------|------|
| 极长标题 | ✅ |
| 特殊字符 | ✅ |
| Unicode支持 | ✅ |
| 空输入 | ✅ |
| 最大行数限制 | ✅ |

## 3. 测试结构

```rust
// bin/tests/e2e/mod.rs

// 模块结构
mod cli_tests;           // CLI命令测试
mod error_tests;         // 错误处理测试
mod boundary_tests;      // 边界条件测试
mod workflow_tests;      // 工作流测试
mod output_tests;        // 输出格式测试

// 基础设施增强
- TestProject: 测试项目创建
- TestStorage: 临时存储管理
- AssertHelpers: 断言辅助
```

## 4. 测试用例详细设计

### 4.1 Create命令测试

```rust
#[tokio::test]
async fn test_create_basic() {
    // 测试基本任务创建
    let result = cli.create_task("Simple task").await;
    assert!(result.success);
    assert!(result.task_id.len() >= 26);
}

#[tokio::test]
async fn test_create_with_description() {
    // 测试带描述创建
    let result = cli.create_task("Task with desc")
        .with_description("Long description...")
        .await;
    assert!(result.success);
}

#[tokio::test]
async fn test_create_empty_title() {
    // 测试空标题（应该失败或拒绝）
    let result = cli.create_task("").await;
    assert!(!result.success || result.error_contains("empty"));
}

#[tokio::test]
async fn test_create_unicode_title() {
    // 测试Unicode标题
    let result = cli.create_task("中文测试 🔧").await;
    assert!(result.success);
}

#[tokio::test]
async fn test_create_special_chars() {
    // 测试特殊字符
    let result = cli.create_task("Task with 'quotes' & \"double\"!").await;
    assert!(result.success);
}

#[tokio::test]
async fn test_create_very_long_title() {
    // 测试超长标题
    let long_title = "A".repeat(1000);
    let result = cli.create_task(&long_title).await;
    // 应该处理或拒绝
    assert!(result.success || result.error_contains("too long"));
}

#[tokio::test]
async fn test_create_multiple_tasks_unique_ids() {
    // 批量创建，验证ID唯一性
    let ids: Vec<_> = (0..10)
        .map(|_| cli.create_task(&format!("Task {}", _)).await.unwrap().task_id)
        .collect();
    // 验证所有ID唯一
    let unique: HashSet<_> = ids.iter().cloned().collect();
    assert_eq!(ids.len(), unique.len());
}
```

### 4.2 List命令测试

```rust
#[tokio::test]
async fn test_list_empty() {
    // 空列表
    let result = cli.list_tasks().await;
    assert!(result.is_empty() || result.contains("No tasks"));
}

#[tokio::test]
async fn test_list_with_tasks() {
    // 创建后列出
    cli.create_task("Test 1").await;
    cli.create_task("Test 2").await;

    let tasks = cli.list_tasks().await;
    assert!(tasks.len() >= 2);
}

#[tokio::test]
async fn test_list_with_limit() {
    // 测试limit参数
    let tasks = cli.list_tasks().with_limit(5).await;
    assert!(tasks.len() <= 5);
}

#[tokio::test]
async fn test_list_by_state() {
    // 测试状态过滤
    let pending = cli.list_tasks().with_state("Pending").await;
    // 验证过滤结果
}
```

### 4.3 Status命令测试

```rust
#[tokio::test]
async fn test_status_valid_task() {
    // 有效任务
    let task = cli.create_task("Test").await;
    let status = cli.status(&task.task_id).await;
    assert!(status.success);
    assert!(status.state == "Pending");
}

#[tokio::test]
async fn test_status_invalid_id() {
    // 无效ID格式
    let result = cli.status("invalid-id").await;
    assert!(!result.success);
    assert!(result.error_contains("invalid") || result.error_contains("not found"));
}

#[tokio::test]
async fn test_status_nonexistent() {
    // 不存在的任务
    let result = cli.status("01KH00000000000000000000000").await;
    assert!(!result.success);
}

#[tokio::test]
async fn test_status_latest() {
    // latest关键字
    let result = cli.status("latest").await;
    // 应该返回最新任务
}
```

### 4.4 Logs命令测试

```rust
#[tokio::test]
async fn test_logs_valid_task() {
    // 有效任务日志
    let task = cli.create_task("Test").await;
    let logs = cli.logs(&task.task_id).await;
    assert!(logs.contains(&task.task_id));
}

#[tokio::test]
async fn test_logs_with_lines() {
    // 行数限制
    let task = cli.create_task("Test").await;
    let logs = cli.logs(&task.task_id).with_lines(10).await;
    // 验证行数限制
}

#[tokio::test]
async fn test_logs_invalid_id() {
    // 无效ID
    let result = cli.logs("invalid").await;
    assert!(!result.success);
}
```

### 4.5 Run命令测试

```rust
#[tokio::test]
async fn test_run_sync() {
    // 同步执行
    let task = cli.create_task("Test").await;
    let result = cli.run(&task.task_id).sync().await;
    assert!(result.success || result.state == "InProgress");
}

#[tokio::test]
async fn test_run_invalid_id() {
    // 无效ID
    let result = cli.run("invalid-id").sync().await;
    assert!(!result.success);
}
```

### 4.6 Rollback命令测试

```rust
#[tokio::test]
async fn test_rollback_with_snapshot() {
    // 快照回滚
    let task = cli.create_task("Test").await;
    // 执行一些操作...
    let result = cli.rollback(&task.task_id).with_snapshot("snapshot-xxx").await;
    assert!(result.success || result.message_contains("rollback"));
}

#[tokio::test]
async fn test_rollback_latest() {
    // latest快照
    let task = cli.create_task("Test").await;
    let result = cli.rollback(&task.task_id).latest().await;
}

#[tokio::test]
async fn test_rollback_no_snapshots() {
    // 无快照
    let task = cli.create_task("Test").await;
    let result = cli.rollback(&task.task_id).await;
    // 应该处理无快照情况
}
```

### 4.7 Search命令测试

```rust
#[tokio::test]
async fn test_search_basic() {
    // 基本搜索
    let result = cli.search("test query").await;
    // 返回搜索结果
}

#[tokio::test]
async fn test_search_with_limit() {
    // 限制结果数
    let result = cli.search("test").with_limit(5).await;
    assert!(result.len() <= 5);
}

#[tokio::test]
async fn test_search_empty_results() {
    // 空结果
    let result = cli.search("nonexistent-xyz-123").await;
    assert!(result.is_empty() || result.contains("No matches"));
}

#[tokio::test]
async fn test_search_special_chars() {
    // 特殊字符搜索
    let result = cli.search("function() {}").await;
    // 应该能处理
}
```

### 4.8 Error处理测试

```rust
#[tokio::test]
async fn test_error_invalid_command() {
    // 无效命令
    let result = cli.run(&["invalid-command"]).await;
    assert!(!result.success);
}

#[tokio::test]
async fn test_error_missing_args() {
    // 缺少参数
    let result = cli.run(&["status"]).await; // 无task_id
    assert!(!result.success);
}

#[tokio::test]
async fn test_error_invalid_output_format() {
    // 无效输出格式
    let result = cli.run(&["--output", "invalid"]).await;
    assert!(!result.success);
}

#[tokio::test]
async fn test_error_permission_denied() {
    // 权限拒绝场景
    // 可能需要特殊配置
}
```

### 4.9 Boundary测试

```rust
#[tokio::test]
async fn test_boundary_unicode_title() {
    // Unicode标题
    let result = cli.create_task("中文标题 🎉 äöü ñ").await;
    assert!(result.success);
}

#[tokio::test]
async fn test_boundary_emoji_title() {
    // Emoji标题
    let result = cli.create_task("🚀 Test with emoji").await;
    assert!(result.success);
}

#[tokio::test]
async fn test_boundary_whitespace_title() {
    // 空白字符
    let result = cli.create_task("  Title with spaces  ").await;
    // 应该处理
}

#[tokio::test]
async fn test_boundary_empty_string() {
    // 空字符串
    let result = cli.create_task("").await;
    // 应该被拒绝
}

#[tokio::test]
async fn test_boundary_very_long_search() {
    // 超长搜索查询
    let long_query = "a".repeat(10000);
    let result = cli.search(&long_query).await;
    // 应该处理或拒绝
}
```

### 4.10 Output格式测试

```rust
#[tokio::test]
async fn test_output_format_pretty() {
    // Pretty格式
    let result = cli.run(&["--output", "pretty", "list"]).await;
    assert!(result.stdout.contains("Tasks:"));
}

#[tokio::test]
async fn test_output_format_json() {
    // JSON格式
    let result = cli.run(&["--output", "json", "list"]).await;
    assert!(result.stdout.starts_with("{") || result.stdout.starts_with("["));
}

#[tokio::test]
async fn test_output_format_minimal() {
    // Minimal格式
    let result = cli.run(&["--output", "minimal", "list"]).await;
    // 应该简洁输出
}
```

## 5. 基础设施增强

### 5.1 TestProject

```rust
pub struct TestProject {
    temp_dir: TempDir,
    cli: NdcCli,
}

impl TestProject {
    pub async fn new(name: &str) -> Self {
        let temp_dir = TempDir::with_prefix(format!("ndc-test-{}", name)).unwrap();
        let project_dir = temp_dir.path().join("project");
        std::fs::create_dir_all(&project_dir).unwrap();

        // 初始化Cargo项目
        Self::init_cargo(&project_dir);

        let cli = NdcCli::new(ndc_path())
            .with_project_root(project_dir.clone());

        Self { temp_dir, cli }
    }

    pub fn cli(&self) -> &NdcCli {
        &self.cli
    }

    pub fn project_path(&self) -> &Path {
        &self.project_dir
    }

    fn init_cargo(dir: &Path) {
        // 创建Cargo.toml
        // 创建src目录
        // 添加测试代码
    }
}
```

### 5.2 AssertHelpers

```rust
pub trait AssertHelpers {
    fn assert_success(&self);
    fn assert_error_contains(&self, substring: &str);
    fn assert_task_id_valid(&self);
    fn assert_state_valid(&self);
}

impl AssertHelpers for CliResult {
    fn assert_success(&self) {
        assert!(self.success, "Expected success but got error: {}", self.stderr);
    }

    fn assert_error_contains(&self, substring: &str) {
        assert!(self.stderr.contains(substring),
            "Expected error containing '{}', got: {}", substring, self.stderr);
    }
}
```

## 6. 执行计划

### Phase 1: 基础设施增强
1. 创建TestProject结构
2. 添加AssertHelpers
3. 改进测试隔离

### Phase 2: CLI命令测试
1. create命令 (8测试)
2. list命令 (5测试)
3. status命令 (4测试)
4. logs命令 (4测试)
5. 其他命令 (15测试)

### Phase 3: 错误和边界测试
1. 错误处理 (5测试)
2. 边界条件 (6测试)
3. 输出格式 (3测试)

### Phase 4: 验证
1. 运行所有测试
2. 修复失败测试
3. 更新文档

## 7. 预期结果

```
测试数量: 50+
测试覆盖: 95%+ CLI功能
测试分类:
  - CLI命令: 40+
  - 错误处理: 5
  - 边界条件: 6
  - 输出格式: 3
```

## 8. 验证方法

```bash
# 运行所有E2E测试
cargo test --test e2e --release

# 运行特定类别
cargo test --test e2e --release cli_tests::
cargo test --test e2e --release error_tests::
cargo test --test e2e --release boundary_tests::

# 运行单个测试
cargo test --test e2e --release test_create_basic

# 检查测试覆盖率
cargo test --test e2e --release -- --nocapture
```
