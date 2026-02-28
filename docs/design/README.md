# Design Docs

本目录用于存放按主题拆分的详细设计文档（Design Spec）。

## 当前文档

- `p0-d-security-project-session.md`
  - 主题：P0-D 安全边界与项目级会话隔离
  - 状态：Draft（用于实现与验收基线）
- `p1-repl-ux-redesign.md`
  - 主题：P1-UX REPL TUI 布局与体验重设计
  - 状态：Draft
  - 内容：现状问题分析、新布局方案（5~6 区动态）、消息轮次模型、主题化颜色系统、交互改进、5 Phase 实现路径
- `p1-scene-adaptive-tui.md`
  - 主题：P1-Scene Context-Aware Adaptive Session TUI
  - 状态：✅ 已完成
  - 内容：repl.rs 模块化提取（9 子模块）、Scene 渲染提示、DiffPreview、工具类型强调色
- `p1-tui-crate-extraction.md`
  - 主题：P1-TuiCrate TUI 独立 Crate 提取
  - 状态：📋 规划完成，待实施
  - 内容：依赖分析、循环依赖消除（redaction 迁移至 core + AgentBackend trait 依赖反转）、4 Phase 实施计划
