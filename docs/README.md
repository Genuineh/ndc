# NDC 文档导航

## 项目概述

NDC (Neo Development Companion) - 生产级 AI 工程体系

## 设计文档

| 文档 | 说明 | 状态 |
|------|------|------|
| [design/2026-02-04-ndc-final-design.md](./design/2026-02-04-ndc-final-design.md) | 最终架构设计 | ✅ 完成 |
| [devman-feature-requests.md](./devman-feature-requests.md) | DevMan 功能需求 | 📋 已提交 |

## 快速链接

- **GitHub**: https://github.com/Genuineh/NDC
- **DevMan**: https://github.com/Genuineh/DevMan
- **DevMan MCP API**: https://github.com/Genuineh/DevMan/blob/main/docs/MCP_API.md

## 架构概览

```
NDC 核心层          Adapter 层          DevMan 后端
─────────────────────────────────────────────────────────
Decision Engine ←→ Task Adapter  ←→  Task Management
NDC Domain Model  ←→ Quality Adapter ←→ Quality Engine
Intent/Verdict    ←→ Tool Adapter   ←→ Tools
AgentRole         ←→ Knowledge Adapter ←→ Knowledge
MemoryStability  ←→                 ←→ Storage
```

## 实现阶段

1. **Phase 1**: 最小可运行核心 (MRC)
2. **Phase 2**: 完整适配
3. **Phase 3**: 交互层
4. **Phase 4**: 可观测性
5. **Phase 5**: 生产就绪

---

最后更新: 2026-02-04
