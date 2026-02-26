# NDC — Claude 开发指南

## 项目概述

NDC（Neo Development Companion）是一个用 Rust 编写的 AI 驱动开发助手，采用 Cargo workspace 组织：

| Crate | 职责 |
|---|---|
| `crates/core` | 核心领域模型、事件与 Agent 抽象 |
| `crates/decision` | 决策引擎与规划逻辑 |
| `crates/interface` | TUI REPL、CLI 命令处理 |
| `crates/runtime` | 工具执行、任务调度、运行时环境 |
| `crates/storage` | 持久化存储抽象与实现 |
| `bin` | 二进制入口 `ndc` |

---

## 开发原则

### Red / Green TDD

**所有功能性代码变更必须遵循红绿测试驱动开发流程：**

```
Red  → 先写一个会失败的测试，明确描述期望行为
Green → 写最少量的生产代码使测试通过
Refactor → 在不破坏测试的前提下整理代码
```

具体规则：

1. **先写测试**：在新增或修改任何逻辑之前，先在对应模块的 `#[cfg(test)]` 块中写出失败的单元测试（`cargo test` 应当 **Red**）。
2. **最小实现**：只写让测试变绿所需的最少代码，不过度设计。
3. **提交粒度**：每个 Red→Green 循环对应一次原子提交（`feat:` / `fix:` / `refactor:`）。
4. **集成测试**：跨 crate 的行为在 `tests/` 目录下以集成测试覆盖，遵循相同的 Red→Green 流程。
5. **禁止跳过**：不允许使用 `#[ignore]` 规避失败测试；确实需要延期的用 `todo!()` 并附带追踪 issue。

```rust
// ✅ 正确：先有 Red 测试
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_feature_does_x() {
        let result = new_feature();
        assert_eq!(result, expected); // 先让它 Red
    }
}

// ✅ 然后写最少代码让它 Green
pub fn new_feature() -> Type {
    // 实现
}
```

---

## 构建与测试

```bash
# 构建所有 crate
cargo build --all-features

# 运行全部测试（必须全绿才能合并）
cargo test --workspace

# 运行单个 crate 测试
cargo test -p ndc-core

# Clippy（CI 要求零 warning）
cargo clippy --workspace --all-features -- -D warnings

# 格式化
cargo fmt --all
```

---

## 代码规范

- **Rust edition**: 2024，全 workspace 统一。
- **异步运行时**: Tokio（`features = ["full"]`）。
- **错误处理**: 用 `thiserror` 定义领域错误类型；禁止 `.unwrap()` 出现在非测试代码中。
- **日志**: 用 `tracing` 宏（`info!` / `debug!` / `warn!` / `error!`），禁止裸 `println!`。
- **安全**: 遵循 OWASP Top 10；所有外部输入必须校验，不允许命令注入路径。

---

## 目录约定

```
crates/<name>/src/lib.rs     # crate 公共 API
crates/<name>/src/           # 内部模块
docs/                        # 设计文档与工程约束
bin/main.rs                  # CLI 入口
```
