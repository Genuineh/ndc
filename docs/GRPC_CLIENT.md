# NDC gRPC 客户端库使用文档

## 简介

`ndc-interface` 提供了 gRPC 客户端库，用于连接和操作 NDC gRPC 守护进程。

## 启用 gRPC 功能

在 `Cargo.toml` 中启用 `grpc` feature：

```toml
[dependencies]
ndc-interface = { path = "path/to/ndc/crates/interface", features = ["grpc"] }
```

## 快速开始

### 1. 基础连接

```rust
use ndc_interface::{NdcClient, create_client, ClientError};

#[tokio::main]
async fn main() -> Result<(), ClientError> {
    // 方式一：使用默认配置快速连接
    let client = create_client("127.0.0.1:50051").await?;

    // 方式二：创建自定义配置的客户端
    let client = NdcClient::new("127.0.0.1:50051");

    // 测试连接
    if client.is_healthy().await {
        println!("连接成功！");
    }

    Ok(())
}
```

### 2. 自定义配置

```rust
use ndc_interface::{NdcClient, ClientConfig};
use std::time::Duration;

let config = ClientConfig {
    address: "192.168.1.1:50051".to_string(),
    connect_timeout: Duration::from_secs(5),
    request_timeout: Duration::from_secs(60),
    max_retries: 5,
    base_retry_delay: Duration::from_millis(200),
    enable_pooling: true,
    pool_size: 8,
};

let client = NdcClient::with_config(config);
```

## API 参考

### NdcClient 方法

| 方法 | 描述 |
|------|------|
| `new(address)` | 创建默认配置的客户端 |
| `with_config(config)` | 使用自定义配置创建客户端 |
| `connect()` | 获取服务端点 URL |
| `is_healthy()` | 检查客户端健康状态 |
| `health_check()` | 执行健康检查 |

### 任务管理

```rust
// 创建任务
let response = client.create_task(
    "实现用户登录功能",
    "需要实现邮箱密码登录和 JWT 认证"
).await?;

// 获取任务
let task = client.get_task("01H...").await?;

// 列出任务
let response = client.list_tasks(10, "pending").await?;
for task in &response.tasks {
    println!("- {}: {}", task.id, task.title);
}

// 执行任务（同步）
let result = client.execute_task("01H...", true).await?;

// 回滚任务
let result = client.rollback_task("01H...", "snapshot-001").await?;
```

### 系统状态

```rust
let status = client.get_system_status().await?;
println!(
    "版本: {}, 任务总数: {}, 活跃任务: {}",
    status.version,
    status.total_tasks,
    status.active_tasks
);
```

## 错误处理

```rust
use ndc_interface::ClientError;

match client.health_check().await {
    Ok(response) => {
        println!("服务健康: {}", response.healthy);
    }
    Err(ClientError::ConnectionFailed(msg)) => {
        println!("连接失败: {}", msg);
    }
    Err(ClientError::Timeout) => {
        println!("请求超时");
    }
    Err(ClientError::NotFound(msg)) => {
        println!("资源不存在: {}", msg);
    }
    Err(ClientError::InvalidArgument(msg)) => {
        println!("参数错误: {}", msg);
    }
    Err(ClientError::ServerError(msg)) => {
        println!("服务器错误: {}", msg);
    }
    Err(ClientError::MaxRetriesExceeded { attempts, last_error }) => {
        println!("重试{}次后失败: {}", attempts, last_error);
    }
}
```

## 完整示例

```rust
use ndc_interface::{NdcClient, ClientError};

#[tokio::main]
async fn main() -> Result<(), ClientError> {
    // 创建客户端
    let client = NdcClient::new("127.0.0.1:50051");

    // 检查健康状态
    println!("检查服务健康状态...");
    let health = client.health_check().await?;
    println!("服务版本: {}", health.version);

    // 创建任务
    println!("\n创建新任务...");
    let task = client.create_task(
        "分析日志文件",
        "分析最近一周的访问日志，统计错误率"
    ).await?;
    println!("任务已创建: {} - {}", task.task.id, task.task.title);

    // 列出所有任务
    println!("\n任务列表:");
    let tasks = client.list_tasks(20, "").await?;
    for t in &tasks.tasks {
        println!("  - [{}] {}", t.state, t.title);
    }

    // 获取系统状态
    println!("\n系统状态:");
    let status = client.get_system_status().await?;
    println!("  总任务数: {}", status.total_tasks);

    Ok(())
}
```

## 配置选项

| 选项 | 默认值 | 说明 |
|------|--------|------|
| `address` | `"127.0.0.1:50051"` | 服务器地址 |
| `connect_timeout` | 10 秒 | 连接超时时间 |
| `request_timeout` | 30 秒 | 请求超时时间 |
| `max_retries` | 3 | 最大重试次数 |
| `base_retry_delay` | 100 毫秒 | 重试基础延迟 |
| `enable_pooling` | false | 启用连接池 |
| `pool_size` | 4 | 连接池大小 |

## 运行服务端

需要先启动 NDC gRPC 守护进程：

```bash
# 启动守护进程
ndc daemon --grpc --address 127.0.0.1:50051
```

然后客户端可以连接到此服务。

## 与其他 crate 的集成

```rust
// 在你的项目中
use ndc_interface::NdcClient;

async fn my_function() {
    let client = NdcClient::new("127.0.0.1:50051");
    // 使用客户端...
}
```
