---
name: mova-frontend
description: 处理 Mova 前端的 UI、交互、弹层、表单、卡片、播放器界面、样式规范和前端验证。用于 apps/mova-web 相关工作。
---

# Mova Frontend

这个 skill 专门处理 `apps/mova-web` 的前端实现细节。  
它负责 UI、交互、样式层级、视觉一致性和前端验证，不重复后端 / 数据库 / 扫描链路说明。

## 使用时机

- 修改 `apps/mova-web` 下的页面、组件、样式、交互
- 调整卡片、表单、标签、switch、icon button、弹窗、popover、menu
- 处理播放器 UI、详情页布局、设置页和管理页界面

## 代码组织

- 前端是独立的 Vite 应用：`apps/mova-web`
- 默认使用 `feature-name/index.tsx` + `feature-name.scss`
- 逻辑复杂时，优先把可测试的决策逻辑下沉到 `src/lib/`
- 前端代码统一使用箭头函数，包括 `src/lib`

## 视觉与交互规则

- 用户可见文案语言规则以 `AGENTS.md` 为准；这个 skill 只负责前端呈现方式
- 如果一个控件看起来 raw、突兀、像浏览器默认样式，要在同一轮里顺手修掉
- 不要为了在卡片里塞更多信息牺牲布局稳定性；宁可收缩内容，也不要把控件挤出边界
- 标签、按钮、switch、icon button、弹窗、popover 尽量复用现有共享样式或共享组件
- 当前项目里的弹层应该统一使用更厚的毛玻璃视觉，不要出现过薄、过透的 surface

## 卡片与布局偏好

- 卡片优先稳定、整洁、信息层级清楚
- 顶部信息区不要堆太多控件在同一行
- 当预览内容破坏布局时，优先改成按钮触发的二级面板
- 列表卡片优先保证主信息、主操作、状态可读，而不是把所有信息都强行塞进首屏

## 弹层规则

- modal、popover、menu 应尽量共用玻璃 surface 规范
- 本地弹出层如果后续还会复用，优先抽成公共组件
- 弹层出现遮挡时，优先检查 stacking context、overflow、section 层级，而不是盲目加大 `z-index`

## 前端测试与验证

- 避免低价值页面级 `tsx` 测试
- 优先测试纯函数、hooks、状态决策
- `tsx` 测试只留给高风险交互流，比如 realtime、player、复杂弹层行为
- 修改前端后，至少跑：
  - `pnpm -C apps/mova-web exec tsc -b --pretty false`
  - `pnpm -C apps/mova-web build`

## 和其他文档的边界

- 最高优先级规则看 `AGENTS.md`
- 仓库总览、后端职责、数据库规则看 `mova-workspace` skill
- 产品方向与当前范围优先看 `README.md`
- 这个 skill 只负责前端执行层面的稳定规则
