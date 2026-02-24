# NDC TODO / Backlog

> 更新时间：2026-02-24  
> 与 `docs/plan/current_plan.md` 对齐。

## 已完成（本轮修复）

- `run --message` 主链接入真实 Agent（非占位输出）
- 默认工具注册统一，支持 function-calling schema 透传
- OpenAI/OpenRouter tool-calling 请求协议打通
- `ndc_task_*` 从 mock 改为真实存储实现
- 工具执行与 TaskVerifier 使用同一份 runtime storage
- ReplToolExecutor 接入 `allow/ask/deny` 权限判定与交互确认
- Orchestrator 会话消息回写（用户/助手/工具）
- Discovery -> HardConstraints -> QualityGate 强制链路落地（执行阶段强制附加质量检查）
- WorkingMemory（Abstract/Raw/Hard）注入 Agent Prompt 主循环（增强系统提示词路径）
- 新增主链 smoke 测试：
  - `ndc_task_create -> ndc_task_update -> ndc_task_verify`
  - 文件工具调用 + 会话续接（Agent Orchestrator）
- Discovery 失败策略可配置：
  - `degrade`（默认，降级继续）
  - `block`（严格模式，阻断执行）
  - 支持 `runtime.discovery_failure_mode` 与 `NDC_DISCOVERY_FAILURE_MODE`
- WorkingMemory 注入升级为真实任务源：
  - 从活跃任务提取失败历史（Abstract）与当前文件/步骤（Raw）
  - 从质量门禁与 memory 访问记录提取约束文本注入（Hard-like constraints）
- 主链 smoke 扩展：
  - 覆盖 QualityGate 失败后的反馈续执行环路（orchestrator verification feedback loop）
  - 覆盖权限 `ask/deny` 分支（含 `NDC_AUTO_APPROVE_TOOLS` 自动确认）
- Hard Invariants 类型统一与结构化注入：
  - `WorkingMemory::VersionedInvariant.priority` 统一使用 Gold Memory `InvariantPriority`
  - `AgentMode` 已恢复结构化 Hard Invariants 注入（非纯文本拼接）
- Invariant Injector 优先级类型收敛完成：
  - `ai_agent/injectors/invariant.rs` 已复用 core `InvariantPriority`（去除第三套定义）
  - 修复 `InvariantInjector::default` 递归与 `get_active` 过滤逻辑
- Hard Invariants 回灌闭环落地：
  - `TaskVerifier::verify_and_track` 失败时固化 invariant（GoldMemory）
  - 同任务后续验证成功时累计 validated 计数
  - Orchestrator 自动验证路径已切换到 `verify_and_track`
- GoldMemory 持久化与会话复用打通：
  - `TaskStorage` 增加 memory 读写抽象，Verifier 可直接持久化 GoldMemory
  - GoldMemoryService 序列化入 runtime storage 并在新 verifier 实例自动加载
  - AgentMode 默认使用该持久化回灌链路
- Discovery/QualityGate 结构化事实映射接入：
  - `TaskVerifier` 将 `VerificationResult` 映射为结构化 rule/tags/evidence 写入 GoldMemory
  - 对重复失败按 task+failure_key 去重，并累计 `violation_count`
  - 质量门禁失败映射为 `Critical` 优先级事实
- GoldMemory schema 版本化与兼容迁移：
  - 持久化格式升级为 `gold_memory_service/v2` 包装载荷
  - 保持对 `gold_memory_service/v1` 历史数据读取兼容
  - 在回灌写入路径自动完成 `v1 -> v2` 迁移
- Discovery 执行阶段结构化信号回灌：
  - `Executor` 在 discovery 失败或产出 hard constraints 时写入结构化 system facts
  - 统一复用 GoldMemory 持久化 entry（`gold_memory_service/v2`）
  - 覆盖 e2e 验证（含并发环境变量隔离锁）
- GoldMemory 迁移审计元数据增强：
  - `v2` 载荷新增 `migration` 审计块（`from_version/migrated_at/trigger_task_id/trigger_source`）
  - verifier 与 executor 均写入统一审计字段
- `v1 -> v2` 迁移测试已覆盖审计字段断言
- Discovery/Verifier 双源事实统一去重与冲突合并策略已落地：
  - `GoldMemoryService::upsert_system_fact` 作为统一规则引擎
  - 统一 `dedupe_key`，重复事实走合并并升级优先级/证据聚合
- GoldMemory 事实检索工具已接入：
  - 新增 `ndc_memory_query`（按 `tags/priority/source` 查询）
  - 默认工具管理器与 tool registry 均已注册
  - e2e smoke 已覆盖查询链路

## P0（最高优先级：多轮对话可视化与实时状态）

> 目标：在多轮对话中实时可见 AI “正在做什么”、做到了哪一步、调用了哪些工具、为什么停下/等待输入。  
> 参考：`opencode` 的 `thinking` 显示、`tool_details`、`session_timeline`、`event.subscribe()` 事件流。

1. 统一事件模型（Orchestrator/REPL/gRPC）
   - 扩展并统一事件类型：`step_start` / `step_finish` / `tool_call_start` / `tool_call_end` / `reasoning` / `text` / `permission_asked` / `session_status` / `error`
   - 明确每类事件的字段（`session_id`、`message_id`、`tool_call_id`、时间戳、耗时、摘要）
   - 约束顺序与幂等语义，保证多轮与重试场景可回放
2. REPL 实时渲染（默认可读、细节可切换）
   - 新增 `"/thinking"`：切换 reasoning 显示（默认关闭，避免噪音）
   - 新增 `"/details"`：切换工具调用详细参数/结果显示
   - 新增 `"/timeline"`：查看当前会话步骤时间线（最近 N 条）
   - 对每次工具调用输出“开始/完成/失败 + 耗时”，并与最终回答分区展示
3. 多轮会话时间线持久化与重放
   - 将事件写入会话存储（最小必要字段 + 可裁剪 payload）
   - 提供会话重放接口：按时间/轮次恢复执行轨迹
   - 为后续 GUI 与 WebSocket/SSE 订阅提供统一数据源
4. 对外流式接口补齐（CLI/SDK 友好）
   - 在现有接口上提供稳定的流式事件订阅能力（与 REPL 同一事件源）
   - 文档化事件协议，确保外部前端可实时展示 agent 执行状态
5. 安全与隐私策略
   - 对 reasoning/tool 参数做脱敏策略（路径、密钥、token、隐私内容）
   - 提供配置项：`display_thinking`、`tool_details`、`timeline_limit`
6. 验收与测试（必须）
   - 新增 e2e：多轮 + 多次 tool call + 权限询问 + 中断恢复 + 时间线回放
   - REPL 快照测试：`thinking/details/timeline` 三种开关组合
   - 文档更新：`docs/USER_GUIDE.md` 增加“如何实时观察 AI 执行过程”

## P1（高优先级）

1. GoldMemory 检索结果接入 orchestrator 自动上下文选择（按任务上下文注入 Top-K facts）
2. Failure Taxonomy 接入重试与回滚策略（含 NonDeterministic）
3. Invariant 的 TTL/version/conflict 检查接入执行前阶段
4. Telemetry 指标落地（autonomous_rate / intervention_cost / token_efficiency）
5. MCP/Skills 接入默认工具发现链与权限治理链

## P2（后续增强）

1. 多 Agent 协同编排（planner / implementer / reviewer）
2. 文档自动回灌与知识库固化策略（阶段 8）
3. REPL 可视化进度与历史重放

## 验收门禁

每个 P0/P1 任务合并前必须满足：

1. `cargo check` 通过
2. `cargo test -q` 通过
3. 对应主链 smoke 测试通过
4. 文档同步更新（本文件 + 架构重规划）
