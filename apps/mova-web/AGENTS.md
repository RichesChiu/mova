# Mova Web AGENTS

本文件适用于 `apps/mova-web` 下的前端代码。公共协作规则统一看根目录 `AGENTS.md`，这里只保留前端执行细节。

## 职责范围

- 修改页面、组件、样式、交互、弹层、表单、卡片、播放器界面。
- 处理 `React + Vite` 前端验证。
- 前端结构或运行方式变化时，按根目录规则同步相关 markdown。

## 代码组织

- 默认使用 `feature-name/index.tsx` + `feature-name.scss`。
- 逻辑复杂时，优先把可测试的决策逻辑下沉到 `src/lib/`。
- 前端代码统一使用箭头函数，包括 `src/lib`。

## 视觉与交互

- 如果一个控件看起来 raw、突兀、像浏览器默认样式，要在同一轮里顺手修掉。
- 不要为了在卡片里塞更多信息牺牲布局稳定性；宁可收缩内容，也不要把控件挤出边界。
- 标签、按钮、switch、icon button、弹窗、popover、menu 尽量复用现有共享样式或共享组件。
- 弹层统一使用更厚的毛玻璃视觉，不要出现过薄、过透的 surface。

## 卡片与布局

- 卡片优先稳定、整洁、信息层级清楚。
- 顶部信息区不要堆太多控件在同一行。
- 当预览内容破坏布局时，优先改成按钮触发的二级面板。
- 列表卡片优先保证主信息、主操作、状态可读，而不是把所有信息都强行塞进首屏。

## 弹层

- modal、popover、menu 应尽量共用玻璃 surface 规范。
- 本地弹出层如果后续还会复用，优先抽成公共组件。
- 弹层出现遮挡时，优先检查 stacking context、overflow、section 层级，而不是盲目加大 `z-index`。

## 验证

- 避免低价值页面级 `tsx` 测试。
- 优先测试纯函数、hooks、状态决策。
- `tsx` 测试只留给高风险交互流，比如 realtime、player、复杂弹层行为。
- 修改前端后，至少跑：
  - `pnpm -C apps/mova-web exec tsc -b --pretty false`
  - `pnpm -C apps/mova-web build`
